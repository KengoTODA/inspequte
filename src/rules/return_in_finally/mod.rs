use std::collections::{BTreeMap, BTreeSet, VecDeque};

use anyhow::Result;
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::engine::AnalysisContext;
use crate::ir::{BasicBlock, Method};
use crate::opcodes;
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

/// Rule that detects return statements executed inside finally blocks.
#[derive(Default)]
pub(crate) struct ReturnInFinallyRule;

crate::register_rule!(ReturnInFinallyRule);

impl Rule for ReturnInFinallyRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "RETURN_IN_FINALLY",
            name: "Return in finally",
            description: "Return statements in finally blocks override exceptions or prior returns",
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

                        let handler_offsets = finally_handler_offsets(method);
                        if handler_offsets.is_empty() {
                            continue;
                        }

                        let block_map = block_map(method);
                        let successor_map = successor_map(method);
                        let mut seen_offsets = BTreeSet::new();

                        for handler_pc in handler_offsets {
                            let handler_blocks =
                                handler_blocks(handler_pc, &block_map, &successor_map);
                            for block_start in handler_blocks {
                                let Some(block) = block_map.get(&block_start) else {
                                    continue;
                                };
                                for instruction in &block.instructions {
                                    if !is_return_opcode(instruction.opcode) {
                                        continue;
                                    }
                                    if !seen_offsets.insert(instruction.offset) {
                                        continue;
                                    }
                                    let message = result_message(
                                        "Return in finally overrides exceptions or prior returns. Move the return outside the finally block or return after the try/finally.",
                                    );
                                    let line = method.line_for_offset(instruction.offset);
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
                    }
                    Ok(class_results)
                })?;

            results.extend(class_results);
        }
        Ok(results)
    }
}

fn finally_handler_offsets(method: &Method) -> Vec<u32> {
    let mut offsets: Vec<u32> = method
        .exception_handlers
        .iter()
        .filter(|handler| handler.catch_type.is_none())
        .map(|handler| handler.handler_pc)
        .collect();
    offsets.sort();
    offsets.dedup();
    offsets
}

fn block_map(method: &Method) -> BTreeMap<u32, &BasicBlock> {
    let mut map = BTreeMap::new();
    for block in &method.cfg.blocks {
        map.insert(block.start_offset, block);
    }
    map
}

fn successor_map(method: &Method) -> BTreeMap<u32, Vec<u32>> {
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

fn handler_blocks(
    handler_pc: u32,
    block_map: &BTreeMap<u32, &BasicBlock>,
    successor_map: &BTreeMap<u32, Vec<u32>>,
) -> BTreeSet<u32> {
    if !block_map.contains_key(&handler_pc) {
        return BTreeSet::new();
    }

    let mut visited = BTreeSet::new();
    let mut queue = VecDeque::new();
    queue.push_back(handler_pc);

    while let Some(block_start) = queue.pop_front() {
        if !visited.insert(block_start) {
            continue;
        }
        let Some(successors) = successor_map.get(&block_start) else {
            continue;
        };
        for successor in successors {
            if !visited.contains(successor) {
                queue.push_back(*successor);
            }
        }
    }

    visited
}

fn is_return_opcode(opcode: u8) -> bool {
    matches!(
        opcode,
        opcodes::IRETURN
            | opcodes::LRETURN
            | opcodes::FRETURN
            | opcodes::DRETURN
            | opcodes::ARETURN
            | opcodes::RETURN
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_harness::{JvmTestHarness, Language, SourceFile};

    fn return_messages(output: &crate::test_harness::AnalysisOutput) -> Vec<String> {
        output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("RETURN_IN_FINALLY"))
            .filter_map(|result| result.message.text.clone())
            .collect()
    }

    #[test]
    fn return_in_finally_reports_return_in_finally_block() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "example/ClassA.java".to_string(),
            contents: r#"
package example;

public class ClassA {
    int MethodX() {
        try {
            throw new RuntimeException("fail");
        } finally {
            return 1;
        }
    }
}
"#
            .to_string(),
        }];

        let analysis = harness
            .compile_and_analyze(Language::Java, &sources, &[])
            .expect("compile and analyze");

        let messages = return_messages(&analysis);
        assert_eq!(messages.len(), 1);
        assert!(messages[0].contains("Return in finally overrides exceptions"));
    }

    #[test]
    fn return_in_finally_ignores_return_after_try_finally() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "example/ClassB.java".to_string(),
            contents: r#"
package example;

public class ClassB {
    int MethodY() {
        int varOne;
        try {
            varOne = 1;
        } finally {
            varOne = 2;
        }
        return varOne;
    }
}
"#
            .to_string(),
        }];

        let analysis = harness
            .compile_and_analyze(Language::Java, &sources, &[])
            .expect("compile and analyze");

        let messages = return_messages(&analysis);
        assert!(
            messages.is_empty(),
            "expected no RETURN_IN_FINALLY, got {messages:?}"
        );
    }

    #[test]
    fn return_in_finally_reports_return_overriding_prior_return() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "example/ClassC.java".to_string(),
            contents: r#"
package example;

public class ClassC {
    int MethodZ() {
        try {
            return 1;
        } finally {
            return 2;
        }
    }
}
"#
            .to_string(),
        }];

        let analysis = harness
            .compile_and_analyze(Language::Java, &sources, &[])
            .expect("compile and analyze");

        let messages = return_messages(&analysis);
        assert_eq!(messages.len(), 1);
    }
}
