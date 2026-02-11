use std::collections::{BTreeMap, BTreeSet, VecDeque};

use anyhow::Result;
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::descriptor::{ReturnKind, method_param_count, method_return_kind};
use crate::engine::AnalysisContext;
use crate::ir::{BasicBlock, CallKind, Instruction, InstructionKind, Method};
use crate::opcodes;
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

/// Rule that detects catch handlers that drop the original exception cause.
#[derive(Default)]
pub(crate) struct ExceptionCauseNotPreservedRule;

crate::register_rule!(ExceptionCauseNotPreservedRule);

impl Rule for ExceptionCauseNotPreservedRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "EXCEPTION_CAUSE_NOT_PRESERVED",
            name: "Exception cause not preserved",
            description: "Catch handlers that throw new exceptions without preserving the cause",
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
                            if !handled_handlers.insert(handler.handler_pc) {
                                continue;
                            }
                            let blocks = collect_reachable_blocks(method, handler.handler_pc);
                            if blocks.is_empty() {
                                continue;
                            }
                            let findings = analyze_handler(method, &blocks);
                            for finding in findings {
                                let message = result_message(
                                    "Catch handler throws a new exception without preserving the original cause; pass the caught exception as a cause or call initCause/addSuppressed before throwing."
                                        .to_string(),
                                );
                                let line = method.line_for_offset(finding.throw_offset);
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Value {
    Unknown,
    Other,
    Caught,
    New(usize),
}

/// Tracks newly created exception instances and whether they preserve the cause.
struct NewTracker {
    next_id: usize,
    preserved: BTreeMap<usize, bool>,
}

impl NewTracker {
    fn new() -> Self {
        Self {
            next_id: 0,
            preserved: BTreeMap::new(),
        }
    }

    fn allocate(&mut self) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.preserved.insert(id, false);
        Value::New(id)
    }

    fn mark_preserved(&mut self, id: usize) {
        if let Some(entry) = self.preserved.get_mut(&id) {
            *entry = true;
        }
    }

    fn is_preserved(&self, id: usize) -> bool {
        self.preserved.get(&id).copied().unwrap_or(false)
    }
}

#[derive(Clone, Copy, Debug)]
struct ThrowFinding {
    throw_offset: u32,
}

fn analyze_handler(method: &Method, blocks: &[&BasicBlock]) -> Vec<ThrowFinding> {
    let mut instructions: Vec<&Instruction> = blocks
        .iter()
        .flat_map(|block| block.instructions.iter())
        .collect();
    instructions.sort_by_key(|inst| inst.offset);

    let mut caught_local: Option<usize> = None;
    let mut locals: Vec<Value> = Vec::new();
    let mut stack: Vec<Value> = Vec::new();
    let mut tracker = NewTracker::new();
    let mut findings = Vec::new();

    for instruction in instructions {
        match instruction.opcode {
            opcodes::ASTORE
            | opcodes::ASTORE_0
            | opcodes::ASTORE_1
            | opcodes::ASTORE_2
            | opcodes::ASTORE_3 => {
                let Some(index) = local_index_for(method, instruction) else {
                    continue;
                };
                ensure_local(&mut locals, index);
                if caught_local.is_none() {
                    caught_local = Some(index);
                    locals[index] = Value::Caught;
                } else {
                    let value = stack.pop().unwrap_or(Value::Unknown);
                    locals[index] = value;
                }
            }
            opcodes::ALOAD
            | opcodes::ALOAD_0
            | opcodes::ALOAD_1
            | opcodes::ALOAD_2
            | opcodes::ALOAD_3 => {
                let Some(index) = local_index_for(method, instruction) else {
                    stack.push(Value::Unknown);
                    continue;
                };
                ensure_local(&mut locals, index);
                if caught_local == Some(index) {
                    stack.push(Value::Caught);
                } else {
                    stack.push(locals[index]);
                }
            }
            opcodes::NEW => {
                stack.push(tracker.allocate());
            }
            opcodes::DUP => {
                if let Some(value) = stack.last().copied() {
                    stack.push(value);
                }
            }
            opcodes::ACONST_NULL
            | opcodes::ICONST_M1
            | opcodes::ICONST_0
            | opcodes::ICONST_1
            | opcodes::ICONST_2
            | opcodes::ICONST_3
            | opcodes::ICONST_4
            | opcodes::ICONST_5
            | opcodes::BIPUSH
            | opcodes::SIPUSH
            | opcodes::LDC
            | opcodes::LDC_W
            | opcodes::LDC2_W => {
                stack.push(Value::Other);
            }
            opcodes::POP => {
                stack.pop();
            }
            opcodes::POP2 => {
                stack.pop();
                stack.pop();
            }
            opcodes::ATHROW => {
                let value = stack.pop().unwrap_or(Value::Unknown);
                if let Value::New(id) = value {
                    if !tracker.is_preserved(id) {
                        findings.push(ThrowFinding {
                            throw_offset: instruction.offset,
                        });
                    }
                }
            }
            _ => {}
        }

        if let InstructionKind::Invoke(call) = &instruction.kind {
            handle_invoke(call, &mut stack, &mut tracker);
        }
    }

    findings
}

fn handle_invoke(call: &crate::ir::CallSite, stack: &mut Vec<Value>, tracker: &mut NewTracker) {
    let param_count = method_param_count(&call.descriptor).unwrap_or(0);
    let mut args = Vec::with_capacity(param_count);
    for _ in 0..param_count {
        args.push(stack.pop().unwrap_or(Value::Unknown));
    }
    let receiver = if call.kind == CallKind::Static {
        None
    } else {
        Some(stack.pop().unwrap_or(Value::Unknown))
    };
    let has_caught = args.iter().any(|value| matches!(value, Value::Caught));

    if call.name == "<init>" {
        if let Some(Value::New(id)) = receiver {
            if has_caught {
                tracker.mark_preserved(id);
            }
        }
        return;
    }

    if (call.name == "initCause" || call.name == "addSuppressed") && has_caught {
        if let Some(Value::New(id)) = receiver {
            tracker.mark_preserved(id);
        }
    }

    let return_kind = method_return_kind(&call.descriptor).unwrap_or(ReturnKind::Void);
    if return_kind != ReturnKind::Void {
        stack.push(Value::Other);
    }
}

fn collect_reachable_blocks<'a>(method: &'a Method, handler_pc: u32) -> Vec<&'a BasicBlock> {
    let block_map = block_map(method);
    if !block_map.contains_key(&handler_pc) {
        return Vec::new();
    }
    let edge_map = edge_map(method);
    let mut visited = BTreeSet::new();
    let mut queue = VecDeque::new();

    queue.push_back(handler_pc);
    while let Some(offset) = queue.pop_front() {
        if !visited.insert(offset) {
            continue;
        }
        if let Some(next_blocks) = edge_map.get(&offset) {
            for next in next_blocks {
                if block_map.contains_key(next) {
                    queue.push_back(*next);
                }
            }
        }
    }

    visited
        .into_iter()
        .filter_map(|offset| block_map.get(&offset).copied())
        .collect()
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

fn local_index_for(method: &Method, instruction: &Instruction) -> Option<usize> {
    let offset = instruction.offset as usize;
    match instruction.opcode {
        opcodes::ASTORE | opcodes::ALOAD => {
            method.bytecode.get(offset + 1).copied().map(|v| v as usize)
        }
        opcodes::ASTORE_0 | opcodes::ALOAD_0 => Some(0),
        opcodes::ASTORE_1 | opcodes::ALOAD_1 => Some(1),
        opcodes::ASTORE_2 | opcodes::ALOAD_2 => Some(2),
        opcodes::ASTORE_3 | opcodes::ALOAD_3 => Some(3),
        _ => None,
    }
}

fn ensure_local(locals: &mut Vec<Value>, index: usize) {
    if locals.len() <= index {
        locals.resize(index + 1, Value::Unknown);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_harness::{JvmTestHarness, Language, SourceFile};

    fn analyze_sources(sources: Vec<SourceFile>) -> Vec<String> {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let output = harness
            .compile_and_analyze(Language::Java, &sources, &[])
            .expect("run harness analysis");
        output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("EXCEPTION_CAUSE_NOT_PRESERVED"))
            .filter_map(|result| result.message.text.clone())
            .collect()
    }

    #[test]
    fn exception_cause_reports_missing_cause() {
        let sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;

public class ClassA {
    public void methodOne() {
        try {
            MethodY();
        } catch (Exception varOne) {
            throw new RuntimeException("failed");
        }
    }

    private void MethodY() {
        throw new IllegalStateException("boom");
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_sources(sources);
        assert!(messages.iter().any(|msg| msg.contains("original cause")));
    }

    #[test]
    fn exception_cause_allows_constructor_cause() {
        let sources = vec![SourceFile {
            path: "com/example/ClassB.java".to_string(),
            contents: r#"
package com.example;

public class ClassB {
    public void methodTwo() {
        try {
            MethodY();
        } catch (Exception varOne) {
            throw new RuntimeException("failed", varOne);
        }
    }

    private void MethodY() {
        throw new IllegalStateException("boom");
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_sources(sources);
        assert!(messages.is_empty());
    }

    #[test]
    fn exception_cause_allows_rethrow() {
        let sources = vec![SourceFile {
            path: "com/example/ClassC.java".to_string(),
            contents: r#"
package com.example;

public class ClassC {
    public void methodThree() {
        try {
            MethodY();
        } catch (Exception varOne) {
            throw varOne;
        }
    }

    private void MethodY() {
        throw new IllegalStateException("boom");
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_sources(sources);
        assert!(messages.is_empty());
    }

    #[test]
    fn exception_cause_allows_init_cause() {
        let sources = vec![SourceFile {
            path: "com/example/ClassD.java".to_string(),
            contents: r#"
package com.example;

public class ClassD {
    public void methodFour() {
        try {
            MethodY();
        } catch (Exception varOne) {
            RuntimeException varTwo = new RuntimeException("failed");
            varTwo.initCause(varOne);
            throw varTwo;
        }
    }

    private void MethodY() {
        throw new IllegalStateException("boom");
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_sources(sources);
        assert!(messages.is_empty());
    }

    #[test]
    fn exception_cause_allows_add_suppressed() {
        let sources = vec![SourceFile {
            path: "com/example/ClassE.java".to_string(),
            contents: r#"
package com.example;

public class ClassE {
    public void methodFive() {
        try {
            MethodY();
        } catch (Exception varOne) {
            RuntimeException varTwo = new RuntimeException("failed");
            varTwo.addSuppressed(varOne);
            throw varTwo;
        }
    }

    private void MethodY() {
        throw new IllegalStateException("boom");
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_sources(sources);
        assert!(messages.is_empty());
    }

    #[test]
    fn exception_cause_ignores_external_exception_instance() {
        let sources = vec![SourceFile {
            path: "com/example/ClassF.java".to_string(),
            contents: r#"
package com.example;

public class ClassF {
    public void methodSix() {
        RuntimeException varTwo = new RuntimeException("failed");
        try {
            MethodY();
        } catch (Exception varOne) {
            throw varTwo;
        }
    }

    private void MethodY() {
        throw new IllegalStateException("boom");
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_sources(sources);
        assert!(messages.is_empty());
    }
}
