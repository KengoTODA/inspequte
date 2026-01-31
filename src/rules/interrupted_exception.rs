use std::collections::{BTreeMap, BTreeSet, VecDeque};

use anyhow::Result;
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::engine::AnalysisContext;
use crate::ir::{BasicBlock, Instruction, InstructionKind, Method};
use crate::opcodes;
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

/// Rule that ensures InterruptedException handlers restore interrupt status.
#[derive(Default)]
pub(crate) struct InterruptedExceptionRule;

crate::register_rule!(InterruptedExceptionRule);

impl Rule for InterruptedExceptionRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "INTERRUPTED_EXCEPTION_NOT_RESTORED",
            name: "InterruptedException not properly handled",
            description: "Restore interrupt status when catching InterruptedException",
        }
    }

    fn run(&self, context: &AnalysisContext) -> Result<Vec<SarifResult>> {
        let mut results = Vec::new();
        for class in &context.classes {
            if !context.is_analysis_target_class(class) {
                continue;
            }
            let mut attributes = vec![KeyValue::new("inspequte.class", class.name.clone())];
            if let Some(uri) = context.class_artifact_uri(class) {
                attributes.push(KeyValue::new("inspequte.artifact_uri", uri));
            }
            let class_results =
                context.with_span("rule.class", &attributes, || -> Result<Vec<SarifResult>> {
                    let mut class_results = Vec::new();
                    for method in &class.methods {
                        if method.bytecode.is_empty() {
                            continue;
                        }
                        let mut handled_handlers = BTreeSet::new();
                        for handler in &method.exception_handlers {
                            if !is_interrupted_exception_handler(handler.catch_type.as_deref()) {
                                continue;
                            }
                            if !handled_handlers.insert(handler.handler_pc) {
                                continue;
                            }
                            let instructions = collect_reachable_instructions(method, handler.handler_pc);
                            if !handler_restores_interrupt(&instructions) {
                                let message = result_message(format!(
                                    "InterruptedException is caught but interrupt status is not restored in {}.{}{}",
                                    class.name, method.name, method.descriptor
                                ));
                                let line = method.line_for_offset(handler.handler_pc);
                                let artifact_uri = context.class_artifact_uri(class);
                                let location = method_location_with_line(
                                    &class.name,
                                    &method.name,
                                    &method.descriptor,
                                    artifact_uri.as_deref(),
                                    line,
                                );
                                class_results.push(
                                    SarifResult::builder()
                                        .message(message)
                                        .locations(vec![location])
                                        .build(),
                                );
                            }
                        }
                    }
                    Ok(class_results)
                })?;
            results.extend(class_results);
        }
        Ok(results)
    }
}

fn is_interrupted_exception_handler(catch_type: Option<&str>) -> bool {
    matches!(
        catch_type,
        Some("java/lang/InterruptedException")
            | Some("java/lang/Exception")
            | Some("java/lang/Throwable")
    )
}

fn collect_reachable_instructions<'a>(method: &'a Method, handler_pc: u32) -> Vec<&'a Instruction> {
    let block_map = block_map(method);
    if !block_map.contains_key(&handler_pc) {
        return Vec::new();
    }
    let edge_map = edge_map(method);
    let mut visited = BTreeSet::new();
    let mut queue = VecDeque::new();
    let mut instructions = Vec::new();

    queue.push_back(handler_pc);
    while let Some(offset) = queue.pop_front() {
        if !visited.insert(offset) {
            continue;
        }
        let Some(block) = block_map.get(&offset) else {
            continue;
        };
        for instruction in &block.instructions {
            instructions.push(instruction);
        }
        if let Some(next_blocks) = edge_map.get(&offset) {
            for next in next_blocks {
                if block_map.contains_key(next) {
                    queue.push_back(*next);
                }
            }
        }
    }

    instructions
}

fn block_map<'a>(method: &'a Method) -> BTreeMap<u32, &'a BasicBlock> {
    let mut map = BTreeMap::new();
    for block in &method.cfg.blocks {
        map.insert(block.start_offset, block);
    }
    map
}

fn edge_map(method: &Method) -> BTreeMap<u32, Vec<u32>> {
    let mut map: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for edge in &method.cfg.edges {
        map.entry(edge.from).or_default().push(edge.to);
    }
    for targets in map.values_mut() {
        targets.sort();
        targets.dedup();
    }
    map
}

fn handler_restores_interrupt(instructions: &[&Instruction]) -> bool {
    let mut seen_current_thread = false;
    for instruction in instructions {
        if instruction.opcode == opcodes::ATHROW {
            return true;
        }
        let InstructionKind::Invoke(call) = &instruction.kind else {
            continue;
        };
        if call.owner == "java/lang/Thread"
            && call.name == "currentThread"
            && call.descriptor == "()Ljava/lang/Thread;"
        {
            seen_current_thread = true;
            continue;
        }
        if call.owner == "java/lang/Thread" && call.name == "interrupt" && call.descriptor == "()V"
        {
            if seen_current_thread {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use crate::test_harness::{JvmTestHarness, Language, SourceFile};

    fn analyze_sources(sources: Vec<SourceFile>) -> Vec<String> {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let output = harness
            .compile_and_analyze(Language::Java, &sources, &[])
            .expect("run harness analysis");
        output
            .results
            .iter()
            .filter(|result| {
                result.rule_id.as_deref() == Some("INTERRUPTED_EXCEPTION_NOT_RESTORED")
            })
            .filter_map(|result| result.message.text.clone())
            .collect()
    }

    #[test]
    fn interrupted_exception_reports_missing_restore() {
        let sources = vec![SourceFile {
            path: "com/example/ClassAlpha.java".to_string(),
            contents: r#"
package com.example;

public class ClassAlpha {
    public void methodOne() {
        try {
            Thread.sleep(10);
        } catch (InterruptedException varOne) {
            System.out.println("Interrupted");
        }
    }
}
"#
            .to_string(),
        }];
        let messages = analyze_sources(sources);
        assert!(messages.iter().any(|msg| msg.contains("interrupt status")));
    }

    #[test]
    fn interrupted_exception_allows_restore() {
        let sources = vec![SourceFile {
            path: "com/example/ClassBeta.java".to_string(),
            contents: r#"
package com.example;

public class ClassBeta {
    public void methodTwo() {
        try {
            Thread.sleep(10);
        } catch (InterruptedException varOne) {
            Thread.currentThread().interrupt();
        }
    }
}
"#
            .to_string(),
        }];
        let messages = analyze_sources(sources);
        assert!(messages.is_empty());
    }

    #[test]
    fn interrupted_exception_allows_rethrow() {
        let sources = vec![SourceFile {
            path: "com/example/ClassGamma.java".to_string(),
            contents: r#"
package com.example;

public class ClassGamma {
    public void methodThree() {
        try {
            Thread.sleep(10);
        } catch (InterruptedException varOne) {
            throw new RuntimeException(varOne);
        }
    }
}
"#
            .to_string(),
        }];
        let messages = analyze_sources(sources);
        assert!(messages.is_empty());
    }

    #[test]
    fn interrupted_exception_allows_throw_declaration() {
        let sources = vec![SourceFile {
            path: "com/example/ClassEta.java".to_string(),
            contents: r#"
package com.example;

public class ClassEta {
    public void methodThree() throws InterruptedException {
        Thread.sleep(10);
    }
}
"#
            .to_string(),
        }];
        let messages = analyze_sources(sources);
        assert!(messages.is_empty());
    }

    #[test]
    fn interrupted_exception_reports_multi_catch() {
        let sources = vec![SourceFile {
            path: "com/example/ClassDelta.java".to_string(),
            contents: r#"
package com.example;

public class ClassDelta {
    public void methodFour() {
        try {
            Thread.sleep(10);
        } catch (InterruptedException | IllegalArgumentException varOne) {
            System.out.println(varOne);
        }
    }
}
"#
            .to_string(),
        }];
        let messages = analyze_sources(sources);
        assert!(messages.iter().any(|msg| msg.contains("interrupt status")));
    }

    #[test]
    fn interrupted_exception_reports_throwable_catch() {
        let sources = vec![SourceFile {
            path: "com/example/ClassTheta.java".to_string(),
            contents: r#"
package com.example;

public class ClassTheta {
    public void methodFive() {
        try {
            Thread.sleep(10);
        } catch (Throwable varOne) {
            System.out.println(varOne);
        }
    }
}
"#
            .to_string(),
        }];
        let messages = analyze_sources(sources);
        assert!(messages.iter().any(|msg| msg.contains("interrupt status")));
    }

    #[test]
    fn interrupted_exception_reports_exception_catch() {
        let sources = vec![SourceFile {
            path: "com/example/ClassEpsilon.java".to_string(),
            contents: r#"
package com.example;

public class ClassEpsilon {
    public void methodFive() {
        try {
            Thread.sleep(10);
        } catch (Exception varOne) {
            System.out.println(varOne);
        }
    }
}
"#
            .to_string(),
        }];
        let messages = analyze_sources(sources);
        assert!(messages.iter().any(|msg| msg.contains("interrupt status")));
    }

    #[test]
    fn interrupted_exception_allows_logging_and_restore() {
        let sources = vec![SourceFile {
            path: "com/example/ClassIota.java".to_string(),
            contents: r#"
package com.example;

public class ClassIota {
    public void methodSix() {
        try {
            Thread.sleep(10);
        } catch (InterruptedException varOne) {
            System.out.println(varOne);
            Thread.currentThread().interrupt();
        }
    }
}
"#
            .to_string(),
        }];
        let messages = analyze_sources(sources);
        assert!(messages.is_empty());
    }

    #[test]
    fn interrupted_exception_allows_conditional_restore() {
        let sources = vec![SourceFile {
            path: "com/example/ClassKappa.java".to_string(),
            contents: r#"
package com.example;

public class ClassKappa {
    public void methodSeven(boolean flagOne) {
        try {
            Thread.sleep(10);
        } catch (InterruptedException varOne) {
            if (flagOne) {
                Thread.currentThread().interrupt();
            }
        }
    }
}
"#
            .to_string(),
        }];
        let messages = analyze_sources(sources);
        assert!(messages.is_empty());
    }

    #[test]
    fn interrupted_exception_allows_finally_restore() {
        let sources = vec![SourceFile {
            path: "com/example/ClassZeta.java".to_string(),
            contents: r#"
package com.example;

public class ClassZeta {
    public void methodSix() {
        boolean flagOne = false;
        try {
            Thread.sleep(10);
        } catch (InterruptedException varOne) {
            flagOne = true;
        } finally {
            if (flagOne) {
                Thread.currentThread().interrupt();
            }
        }
    }
}
"#
            .to_string(),
        }];
        let messages = analyze_sources(sources);
        assert!(messages.is_empty());
    }
}
