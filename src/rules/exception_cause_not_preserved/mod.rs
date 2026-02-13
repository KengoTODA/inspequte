use std::cell::Cell;
use std::collections::BTreeSet;
use std::sync::OnceLock;

use anyhow::Result;
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::dataflow::opcode_semantics::{ApplyOutcome, ValueDomain, apply_default_semantics};
use crate::dataflow::stack_machine::{StackMachine, StackMachineConfig};
use crate::dataflow::worklist::{
    BlockEndStep, InstructionStep, WorklistSemantics, WorklistState, analyze_method,
};
use crate::descriptor::{ReturnKind, method_param_count, method_return_kind};
use crate::engine::AnalysisContext;
use crate::ir::{CallKind, CallSite, Instruction, InstructionKind, Method};
use crate::opcodes;
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

const MAX_TRACKED_STACK_DEPTH: usize = 24;
const MAX_TRACKED_ALLOCATIONS: usize = 4;

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

                        let mut seen_findings = BTreeSet::new();
                        for handler_pc in handler_offsets(method) {
                            for throw_offset in analyze_handler(method, handler_pc)? {
                                if !seen_findings.insert((handler_pc, throw_offset)) {
                                    continue;
                                }

                                let message = result_message(
                                    "Catch handler throws a new exception without preserving the original cause; pass the caught exception as a cause or call initCause/addSuppressed before throwing.",
                                );
                                let line = method.line_for_offset(throw_offset);
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum Value {
    Other,
    Caught,
    New(u32),
}

/// Value-domain adapter used by shared default opcode semantics.
struct ExceptionValueDomain;

impl ValueDomain<Value> for ExceptionValueDomain {
    fn unknown_value(&self) -> Value {
        Value::Other
    }

    fn scalar_value(&self) -> Value {
        Value::Other
    }
}

/// Symbolic execution state at a specific instruction position.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct ExecutionState {
    block_start: u32,
    instruction_index: usize,
    machine: StackMachine<Value>,
    preserved_allocations: BTreeSet<u32>,
}

impl WorklistState for ExecutionState {
    fn block_start(&self) -> u32 {
        self.block_start
    }

    fn instruction_index(&self) -> usize {
        self.instruction_index
    }

    fn set_position(&mut self, block_start: u32, instruction_index: usize) {
        self.block_start = block_start;
        self.instruction_index = instruction_index;
    }
}

/// Dataflow callbacks for catch-handler symbolic execution.
struct HandlerSemantics {
    handler_pc: u32,
    debug_enabled: bool,
    stack_depth_dumped: Cell<bool>,
}

impl HandlerSemantics {
    fn new(handler_pc: u32) -> Self {
        Self {
            handler_pc,
            debug_enabled: debug_stack_dump_enabled(),
            stack_depth_dumped: Cell::new(false),
        }
    }
}

impl WorklistSemantics for HandlerSemantics {
    type State = ExecutionState;
    type Finding = u32;

    fn initial_states(&self, _method: &Method) -> Vec<Self::State> {
        vec![ExecutionState {
            block_start: self.handler_pc,
            instruction_index: 0,
            machine: initial_machine(),
            preserved_allocations: BTreeSet::new(),
        }]
    }

    fn canonicalize_state(&self, state: &mut Self::State) {
        canonicalize_state(state);
    }

    fn transfer_instruction(
        &self,
        method: &Method,
        instruction: &Instruction,
        state: &mut Self::State,
    ) -> Result<InstructionStep<Self::Finding>> {
        if is_return_opcode(instruction.opcode) {
            apply_stack_effect(method, instruction, state)?;
            return Ok(InstructionStep::terminate_path());
        }

        if instruction.opcode == opcodes::ATHROW {
            let thrown = state.machine.pop();
            if let Value::New(allocation_offset) = thrown
                && !state.preserved_allocations.contains(&allocation_offset)
            {
                return Ok(InstructionStep::terminate_path().with_finding(instruction.offset));
            }
            return Ok(InstructionStep::terminate_path());
        }

        apply_stack_effect(method, instruction, state)?;
        prune_preserved_allocations(state);
        if self.debug_enabled
            && !self.stack_depth_dumped.get()
            && state.machine.stack_len() >= MAX_TRACKED_STACK_DEPTH
        {
            dump_stack_depth(method, self.handler_pc, instruction, state);
            self.stack_depth_dumped.set(true);
        }

        Ok(InstructionStep::continue_path())
    }

    fn on_block_end(
        &self,
        _method: &Method,
        state: &Self::State,
        successors: &[u32],
    ) -> Result<BlockEndStep<Self::State, Self::Finding>> {
        // Keep execution inside catch-handler suffix to avoid exploring pre-handler loops.
        let bounded_successors = successors
            .iter()
            .copied()
            .filter(|successor| *successor >= self.handler_pc)
            .collect::<Vec<_>>();
        Ok(BlockEndStep::follow_all_successors(
            state,
            &bounded_successors,
        ))
    }
}

fn handler_offsets(method: &Method) -> Vec<u32> {
    let mut offsets = BTreeSet::new();
    for handler in &method.exception_handlers {
        offsets.insert(handler.handler_pc);
    }
    offsets.into_iter().collect()
}

fn analyze_handler(method: &Method, handler_pc: u32) -> Result<Vec<u32>> {
    let semantics = HandlerSemantics::new(handler_pc);
    let findings = analyze_method(method, &semantics)?;
    Ok(findings
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect())
}

fn initial_machine() -> StackMachine<Value> {
    let mut machine = StackMachine::with_config(
        Value::Other,
        StackMachineConfig {
            max_stack_depth: Some(MAX_TRACKED_STACK_DEPTH),
            max_locals: None,
            max_symbolic_identities: Some(MAX_TRACKED_ALLOCATIONS),
        },
    );
    machine.push(Value::Caught);
    machine
}

fn apply_stack_effect(
    method: &Method,
    instruction: &Instruction,
    state: &mut ExecutionState,
) -> Result<()> {
    let domain = ExceptionValueDomain;
    if instruction.opcode != opcodes::NEW
        && apply_default_semantics(
            &mut state.machine,
            method,
            instruction.offset as usize,
            instruction.opcode,
            &domain,
        ) == ApplyOutcome::Applied
    {
        return Ok(());
    }

    match instruction.opcode {
        opcodes::NEW => {
            state.machine.push(Value::New(instruction.offset));
        }
        opcodes::AALOAD => {
            state.machine.pop_n(2);
            state.machine.push(Value::Other);
        }
        opcodes::AASTORE => {
            state.machine.pop_n(3);
        }
        opcodes::IF_ACMPEQ | opcodes::IF_ACMPNE => {
            state.machine.pop_n(2);
        }
        // Primitive and non-reference loads not covered by table-driven defaults.
        0x15..=0x18 | 0x1a..=0x29 => {
            state.machine.push(Value::Other);
        }
        // Primitive array loads.
        0x2e..=0x31 | 0x33..=0x35 => {
            state.machine.pop_n(2);
            state.machine.push(Value::Other);
        }
        // Primitive stores not covered by table-driven defaults.
        0x36 | 0x38 | 0x3b..=0x3e | 0x43..=0x46 => {
            state.machine.pop_n(1);
        }
        // Primitive stores not covered by table-driven defaults.
        0x37 | 0x39 | 0x3f..=0x42 | 0x47..=0x4a => state.machine.pop_n(2),
        // Primitive array stores.
        0x4f..=0x52 | 0x54..=0x56 => {
            state.machine.pop_n(3);
        }
        // Stack shuffling opcodes.
        0x5a..=0x5e => {
            state.machine.push(Value::Other);
        }
        0x5f => {
            let right = state.machine.pop();
            let left = state.machine.pop();
            state.machine.push(right);
            state.machine.push(left);
        }
        // Primitive arithmetic.
        0x60..=0x73 | 0x78..=0x83 | 0x94..=0x98 => {
            state.machine.pop_n(2);
            state.machine.push(Value::Other);
        }
        0x74..=0x77 | 0x85..=0x93 => {
            state.machine.pop_n(1);
            state.machine.push(Value::Other);
        }
        // iinc has no stack effect.
        0x84 => {}
        // Legacy subroutine opcodes.
        opcodes::JSR | opcodes::JSR_W => {
            state.machine.push(Value::Other);
        }
        opcodes::GOTO | opcodes::GOTO_W => {}
        // Field access.
        0xb2 => {
            state.machine.push(Value::Other);
        }
        0xb3 => {
            state.machine.pop_n(1);
        }
        0xb4 => {
            state.machine.pop_n(1);
            state.machine.push(Value::Other);
        }
        0xb5 => {
            state.machine.pop_n(2);
        }
        // INVOKEDYNAMIC is handled from InstructionKind to apply descriptor-based stack effects.
        opcodes::INVOKEDYNAMIC => {}
        // Array/type/monitor opcodes.
        opcodes::NEWARRAY | opcodes::ANEWARRAY | opcodes::ARRAYLENGTH | 0xc0 | 0xc1 => {
            state.machine.pop_n(1);
            state.machine.push(Value::Other);
        }
        0xc2 | 0xc3 => {
            state.machine.pop_n(1);
        }
        opcodes::MULTIANEWARRAY => {
            let dims = method
                .bytecode
                .get(instruction.offset as usize + 3)
                .copied()
                .unwrap_or(1);
            state.machine.pop_n(dims as usize);
            state.machine.push(Value::Other);
        }
        _ => {}
    }

    match &instruction.kind {
        InstructionKind::Invoke(call) => handle_invoke(call, state)?,
        InstructionKind::InvokeDynamic { descriptor } => {
            handle_invoke_dynamic_descriptor(descriptor, state)?
        }
        _ => {}
    }

    Ok(())
}

fn handle_invoke(call: &CallSite, state: &mut ExecutionState) -> Result<()> {
    let param_count = method_param_count(&call.descriptor)?;
    let mut args = Vec::with_capacity(param_count);
    for _ in 0..param_count {
        args.push(state.machine.pop());
    }

    let receiver = if call.kind == CallKind::Static {
        None
    } else {
        Some(state.machine.pop())
    };

    let has_caught_argument = args.iter().any(|value| matches!(value, Value::Caught));

    if call.name == "<init>" {
        if let Some(Value::New(allocation_offset)) = receiver {
            if has_caught_argument {
                state.preserved_allocations.insert(allocation_offset);
            }
        }
        return Ok(());
    }

    let mut return_value = match method_return_kind(&call.descriptor)? {
        ReturnKind::Void => None,
        ReturnKind::Primitive | ReturnKind::Reference => Some(Value::Other),
    };

    if call.name == "initCause" {
        if has_caught_argument {
            if let Some(Value::New(allocation_offset)) = receiver {
                state.preserved_allocations.insert(allocation_offset);
            }
        }
        return_value = receiver;
    } else if call.name == "addSuppressed"
        && has_caught_argument
        && let Some(Value::New(allocation_offset)) = receiver
    {
        state.preserved_allocations.insert(allocation_offset);
    }

    if let Some(value) = return_value {
        state.machine.push(value);
    }

    Ok(())
}

fn handle_invoke_dynamic_descriptor(descriptor: &str, state: &mut ExecutionState) -> Result<()> {
    let param_count = method_param_count(descriptor)?;
    state.machine.pop_n(param_count);

    if method_return_kind(descriptor)? != ReturnKind::Void {
        state.machine.push(Value::Other);
    }

    Ok(())
}

fn prune_preserved_allocations(state: &mut ExecutionState) {
    let tracked_allocations = state
        .machine
        .enforce_symbolic_identity_cap_u32(
            |value| match value {
                Value::New(offset) => Some(*offset),
                _ => None,
            },
            |value| *value = Value::Other,
        )
        .unwrap_or_default();
    state.machine.retain_locals(|_, value| match *value {
        Value::Caught => true,
        Value::New(offset) => tracked_allocations.contains(&offset),
        Value::Other => false,
    });
    state
        .preserved_allocations
        .retain(|offset| tracked_allocations.contains(offset));
}

fn debug_stack_dump_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("INSPEQUTE_DEBUG_EXCEPTION_CAUSE_STACK")
            .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    })
}

fn dump_stack_depth(
    method: &Method,
    handler_pc: u32,
    instruction: &Instruction,
    state: &ExecutionState,
) {
    if !debug_stack_dump_enabled() {
        return;
    }

    eprintln!(
        "exception_cause_not_preserved debug: stack depth reached limit method={}{} handler_pc={} offset={} opcode=0x{:02x} depth={} top={:?}",
        method.name,
        method.descriptor,
        handler_pc,
        instruction.offset,
        instruction.opcode,
        state.machine.stack_len(),
        state
            .machine
            .stack_values()
            .iter()
            .rev()
            .take(8)
            .collect::<Vec<_>>()
    );
}

fn canonicalize_state(state: &mut ExecutionState) {
    let mapping = state.machine.canonicalize_symbolic_ids_u32(
        |value| match value {
            Value::New(offset) => Some(*offset),
            _ => None,
        },
        |value, mapped| *value = Value::New(mapped),
        state.preserved_allocations.iter().copied(),
    );
    state.preserved_allocations = state
        .preserved_allocations
        .iter()
        .filter_map(|offset| mapping.get(offset).copied())
        .collect();
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
            MethodX();
        } catch (Exception varOne) {
            throw new RuntimeException("failed");
        }
    }

    private void MethodX() {
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
            MethodX();
        } catch (Exception varOne) {
            throw new RuntimeException("failed", varOne);
        }
    }

    private void MethodX() {
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
    public void methodThree() throws Exception {
        try {
            MethodX();
        } catch (Exception varOne) {
            throw varOne;
        }
    }

    private void MethodX() {
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
            MethodX();
        } catch (Exception varOne) {
            RuntimeException varTwo = new RuntimeException("failed");
            varTwo.initCause(varOne);
            throw varTwo;
        }
    }

    private void MethodX() {
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
    fn exception_cause_reports_path_without_preserve() {
        let sources = vec![SourceFile {
            path: "com/example/ClassE.java".to_string(),
            contents: r#"
package com.example;

public class ClassE {
    public void methodFive(boolean varTwo) {
        try {
            MethodX();
        } catch (Exception varOne) {
            RuntimeException varThree = new RuntimeException("failed");
            if (varTwo) {
                varThree.initCause(varOne);
            }
            throw varThree;
        }
    }

    private void MethodX() {
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
    fn exception_cause_allows_add_suppressed() {
        let sources = vec![SourceFile {
            path: "com/example/ClassF.java".to_string(),
            contents: r#"
package com.example;

public class ClassF {
    public void methodSix() {
        try {
            MethodX();
        } catch (Exception varOne) {
            RuntimeException varTwo = new RuntimeException("failed");
            varTwo.addSuppressed(varOne);
            throw varTwo;
        }
    }

    private void MethodX() {
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
            path: "com/example/ClassG.java".to_string(),
            contents: r#"
package com.example;

public class ClassG {
    public void methodSeven() {
        RuntimeException varTwo = new RuntimeException("failed");
        try {
            MethodX();
        } catch (Exception varOne) {
            throw varTwo;
        }
    }

    private void MethodX() {
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
    fn exception_cause_reports_after_primitive_loop_in_catch() {
        let sources = vec![SourceFile {
            path: "com/example/ClassH.java".to_string(),
            contents: r#"
package com.example;

public class ClassH {
    public void methodEight(int varTwo) {
        try {
            MethodX();
        } catch (Exception varOne) {
            int varThree = 0;
            while (varThree < varTwo) {
                varThree++;
            }
            throw new RuntimeException("failed");
        }
    }

    private void MethodX() {
        throw new IllegalStateException("boom");
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_sources(sources);
        assert!(messages.iter().any(|msg| msg.contains("original cause")));
    }
}
