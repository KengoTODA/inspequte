use std::collections::{BTreeMap, BTreeSet, VecDeque};

use anyhow::Result;
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::dataflow::opcode_semantics::{
    ApplyOutcome, SemanticsCoverage, SemanticsDebugConfig, SemanticsHooks, ValueDomain,
    apply_semantics,
};
use crate::dataflow::stack_machine::StackMachine;
use crate::dataflow::worklist::{
    BlockEndStep, InstructionStep, WorklistSemantics, WorklistState, analyze_method,
};
use crate::descriptor::{ReturnKind, method_param_count, method_return_kind};
use crate::engine::AnalysisContext;
use crate::ir::{CallKind, CallSite, Class, EdgeKind, Instruction, InstructionKind, Method};
use crate::opcodes;
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

const MAX_TRACKED_STACK_DEPTH: usize = 32;

/// Rule that detects locally created executor services without guaranteed shutdown.
#[derive(Default)]
pub(crate) struct ExecutorServiceNotShutdownRule;

crate::register_rule!(ExecutorServiceNotShutdownRule);

impl Rule for ExecutorServiceNotShutdownRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "EXECUTOR_SERVICE_NOT_SHUTDOWN",
            name: "ExecutorService not shut down",
            description: "Locally created executor services should be shut down on every exit path",
        }
    }

    fn run(&self, context: &AnalysisContext) -> Result<Vec<SarifResult>> {
        let mut results = Vec::new();
        let class_map = context
            .all_classes()
            .map(|class| (class.name.clone(), class))
            .collect::<BTreeMap<_, _>>();

        for class in context.analysis_target_classes() {
            let mut attributes = vec![KeyValue::new("inspequte.class", class.name.clone())];
            if let Some(uri) = context.class_artifact_uri(class) {
                attributes.push(KeyValue::new("inspequte.artifact_uri", uri));
            }

            let class_results =
                context.with_span("rule.class", &attributes, || -> Result<Vec<SarifResult>> {
                    let mut class_results = Vec::new();
                    for method in &class.methods {
                        if method.bytecode.is_empty() || method.cfg.blocks.is_empty() {
                            continue;
                        }

                        for creation_offset in analyze_executor_lifecycle(method, &class_map)? {
                            let message = result_message(format!(
                                "ExecutorService created in {}.{}{} may exit without shutdown(); call shutdown(), shutdownNow(), or close() before the method returns.",
                                class.name, method.name, method.descriptor
                            ));
                            let line = method.line_for_offset(creation_offset);
                            let location = method_location_with_line(
                                &class.name,
                                &method.name,
                                &method.descriptor,
                                context.class_artifact_uri(class).as_deref(),
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
                    Ok(class_results)
                })?;
            results.extend(class_results);
        }

        Ok(results)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum Value {
    Unknown,
    Symbol(u32),
}

/// Value-domain adapter for default opcode semantics.
struct ExecutorValueDomain;

impl ValueDomain<Value> for ExecutorValueDomain {
    fn unknown_value(&self) -> Value {
        Value::Unknown
    }

    fn scalar_value(&self) -> Value {
        Value::Unknown
    }
}

/// Symbolic execution state for local executor ownership tracking.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct ExecutionState {
    block_start: u32,
    instruction_index: usize,
    machine: StackMachine<Value>,
    active_executors: BTreeSet<u32>,
    branch_filter: Option<BranchFilter>,
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

/// Dataflow callbacks for local executor lifecycle analysis.
struct ExecutorLifecycleSemantics<'a> {
    entry_block: u32,
    class_map: &'a BTreeMap<String, &'a Class>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum BranchFilter {
    TakeBranch,
    TakeFallThrough,
}

impl WorklistSemantics for ExecutorLifecycleSemantics<'_> {
    type State = ExecutionState;
    type Finding = u32;

    fn initial_states(&self, _method: &Method) -> Vec<Self::State> {
        vec![ExecutionState {
            block_start: self.entry_block,
            instruction_index: 0,
            machine: StackMachine::with_config(
                Value::Unknown,
                crate::dataflow::stack_machine::StackMachineConfig {
                    max_stack_depth: Some(MAX_TRACKED_STACK_DEPTH),
                    max_locals: None,
                    max_symbolic_identities: None,
                },
            ),
            active_executors: BTreeSet::new(),
            branch_filter: None,
        }]
    }

    fn transfer_instruction(
        &self,
        method: &Method,
        instruction: &Instruction,
        state: &mut Self::State,
    ) -> Result<InstructionStep<Self::Finding>> {
        state.branch_filter = None;
        match instruction.opcode {
            opcodes::AASTORE => {
                escape_top_symbol(state);
                state.machine.pop_n(3);
            }
            opcodes::ARETURN => {
                escape_value(state.machine.pop(), state);
            }
            opcodes::ATHROW => {
                escape_value(state.machine.pop(), state);
            }
            0xb3 => {
                escape_top_symbol(state);
                state.machine.pop_n(1);
            }
            0xb5 => {
                escape_top_symbol(state);
                state.machine.pop_n(2);
            }
            opcodes::IFNULL | opcodes::IFNONNULL => {
                let value = state.machine.pop();
                if let Value::Symbol(symbol) = value
                    && state.active_executors.contains(&symbol)
                {
                    state.branch_filter = Some(if instruction.opcode == opcodes::IFNULL {
                        BranchFilter::TakeFallThrough
                    } else {
                        BranchFilter::TakeBranch
                    });
                }
            }
            _ => match &instruction.kind {
                InstructionKind::Invoke(call) => handle_invoke(call, state, self.class_map)?,
                InstructionKind::InvokeDynamic { descriptor, .. } => {
                    handle_invoke_dynamic(descriptor, state)?
                }
                _ => apply_stack_effect(method, instruction, state),
            },
        }

        Ok(InstructionStep::continue_path())
    }

    fn on_block_end(
        &self,
        method: &Method,
        state: &Self::State,
        successors: &[u32],
    ) -> Result<BlockEndStep<Self::State, Self::Finding>> {
        if successors.is_empty() {
            let mut step = BlockEndStep::terminal();
            for creation_offset in &state.active_executors {
                step = step.with_finding(*creation_offset);
            }
            return Ok(step);
        }

        let filtered_successors = match state.branch_filter {
            Some(filter) => branch_successors(method, state.block_start, filter),
            None => Vec::new(),
        };
        if !filtered_successors.is_empty() {
            return Ok(follow_successors_without_branch_filter(
                state,
                &filtered_successors,
            ));
        }

        Ok(follow_successors_without_branch_filter(state, successors))
    }
}

fn analyze_executor_lifecycle(
    method: &Method,
    class_map: &BTreeMap<String, &Class>,
) -> Result<Vec<u32>> {
    let entry_block = method
        .cfg
        .blocks
        .iter()
        .map(|block| block.start_offset)
        .min()
        .unwrap_or(0);
    let semantics = ExecutorLifecycleSemantics {
        entry_block,
        class_map,
    };
    let findings = analyze_method(method, &semantics)?;
    Ok(findings
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect())
}

fn apply_stack_effect(method: &Method, instruction: &Instruction, state: &mut ExecutionState) {
    let domain = ExecutorValueDomain;
    let mut hook = ExecutorSemanticsHook {
        allocation_offset: instruction.offset,
    };
    let mut coverage = SemanticsCoverage::default();
    let _ = apply_semantics(
        &mut state.machine,
        method,
        instruction.offset as usize,
        instruction.opcode,
        &domain,
        &mut hook,
        &mut coverage,
        SemanticsDebugConfig::default(),
    );
}

/// Rule-specific hook that gives each new reference allocation a symbolic ID.
struct ExecutorSemanticsHook {
    allocation_offset: u32,
}

impl SemanticsHooks<Value> for ExecutorSemanticsHook {
    fn pre_apply(
        &mut self,
        machine: &mut StackMachine<Value>,
        _method: &Method,
        _offset: usize,
        opcode: u8,
    ) -> ApplyOutcome {
        if opcode == opcodes::NEW {
            machine.push(Value::Symbol(self.allocation_offset));
            return ApplyOutcome::Applied;
        }
        ApplyOutcome::NotHandled
    }
}

fn handle_invoke(
    call: &CallSite,
    state: &mut ExecutionState,
    class_map: &BTreeMap<String, &Class>,
) -> Result<()> {
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

    if call.name == "<init>" {
        if let Some(Value::Symbol(symbol)) = receiver
            && is_executor_constructor(call, class_map)
        {
            state.active_executors.insert(symbol);
        }
        for value in args {
            escape_value(value, state);
        }
        return Ok(());
    }

    let shutdown_receiver = if is_shutdown_call(call) {
        receiver.and_then(|value| match value {
            Value::Symbol(symbol) if state.active_executors.contains(&symbol) => Some(symbol),
            _ => None,
        })
    } else {
        None
    };

    for value in args {
        escape_value(value, state);
    }

    if let Some(symbol) = shutdown_receiver {
        state.active_executors.remove(&symbol);
    }

    if is_executor_factory_call(call) {
        let symbol = call.offset;
        state.active_executors.insert(symbol);
        state.machine.push(Value::Symbol(symbol));
        return Ok(());
    }

    match method_return_kind(&call.descriptor)? {
        ReturnKind::Void => {}
        ReturnKind::Primitive | ReturnKind::Reference => state.machine.push(Value::Unknown),
    }

    Ok(())
}

fn handle_invoke_dynamic(descriptor: &str, state: &mut ExecutionState) -> Result<()> {
    let param_count = method_param_count(descriptor)?;
    let mut args = Vec::with_capacity(param_count);
    for _ in 0..param_count {
        args.push(state.machine.pop());
    }
    for value in args {
        escape_value(value, state);
    }
    if method_return_kind(descriptor)? != ReturnKind::Void {
        state.machine.push(Value::Unknown);
    }
    Ok(())
}

fn branch_successors(method: &Method, block_start: u32, filter: BranchFilter) -> Vec<u32> {
    let mut selected = method
        .cfg
        .edges
        .iter()
        .filter(|edge| edge.from == block_start)
        .filter_map(|edge| match (filter, edge.kind) {
            (BranchFilter::TakeBranch, EdgeKind::Branch) => Some(edge.to),
            (BranchFilter::TakeFallThrough, EdgeKind::FallThrough) => Some(edge.to),
            _ => None,
        })
        .collect::<Vec<_>>();
    selected.sort();
    selected.dedup();
    selected
}

fn follow_successors_without_branch_filter(
    state: &ExecutionState,
    successors: &[u32],
) -> BlockEndStep<ExecutionState, u32> {
    let mut next = state.clone();
    next.branch_filter = None;
    BlockEndStep::follow_all_successors(&next, successors)
}

fn escape_top_symbol(state: &mut ExecutionState) {
    if let Some(value) = state.machine.stack_values().last().copied() {
        escape_value(value, state);
    }
}

fn escape_value(value: Value, state: &mut ExecutionState) {
    if let Value::Symbol(symbol) = value {
        state.active_executors.remove(&symbol);
    }
}

fn is_shutdown_call(call: &CallSite) -> bool {
    matches!(
        (call.name.as_str(), call.descriptor.as_str()),
        ("shutdown", "()V") | ("shutdownNow", "()Ljava/util/List;") | ("close", "()V")
    )
}

fn is_executor_factory_call(call: &CallSite) -> bool {
    if call.kind != CallKind::Static || call.owner != "java/util/concurrent/Executors" {
        return false;
    }

    matches!(
        (call.name.as_str(), call.descriptor.as_str()),
        ("newCachedThreadPool", "()Ljava/util/concurrent/ExecutorService;")
            | (
                "newCachedThreadPool",
                "(Ljava/util/concurrent/ThreadFactory;)Ljava/util/concurrent/ExecutorService;"
            )
            | ("newFixedThreadPool", "(I)Ljava/util/concurrent/ExecutorService;")
            | (
                "newFixedThreadPool",
                "(ILjava/util/concurrent/ThreadFactory;)Ljava/util/concurrent/ExecutorService;"
            )
            | ("newSingleThreadExecutor", "()Ljava/util/concurrent/ExecutorService;")
            | (
                "newSingleThreadExecutor",
                "(Ljava/util/concurrent/ThreadFactory;)Ljava/util/concurrent/ExecutorService;"
            )
            | (
                "newSingleThreadScheduledExecutor",
                "()Ljava/util/concurrent/ScheduledExecutorService;"
            )
            | (
                "newSingleThreadScheduledExecutor",
                "(Ljava/util/concurrent/ThreadFactory;)Ljava/util/concurrent/ScheduledExecutorService;"
            )
            | (
                "newScheduledThreadPool",
                "(I)Ljava/util/concurrent/ScheduledExecutorService;"
            )
            | (
                "newScheduledThreadPool",
                "(ILjava/util/concurrent/ThreadFactory;)Ljava/util/concurrent/ScheduledExecutorService;"
            )
            | ("newWorkStealingPool", "()Ljava/util/concurrent/ExecutorService;")
            | ("newWorkStealingPool", "(I)Ljava/util/concurrent/ExecutorService;")
            | (
                "newThreadPerTaskExecutor",
                "(Ljava/util/concurrent/ThreadFactory;)Ljava/util/concurrent/ExecutorService;"
            )
            | (
                "newVirtualThreadPerTaskExecutor",
                "()Ljava/util/concurrent/ExecutorService;"
            )
    )
}

fn is_executor_constructor(call: &CallSite, class_map: &BTreeMap<String, &Class>) -> bool {
    call.name == "<init>" && is_executor_service_type(&call.owner, class_map)
}

fn is_executor_service_type(name: &str, class_map: &BTreeMap<String, &Class>) -> bool {
    if is_known_executor_service_name(name) {
        return true;
    }

    let mut queue = VecDeque::from([name.to_string()]);
    let mut seen = BTreeSet::new();
    while let Some(next) = queue.pop_front() {
        if !seen.insert(next.clone()) {
            continue;
        }
        if is_known_executor_service_name(&next) {
            return true;
        }
        let Some(class) = class_map.get(&next) else {
            continue;
        };
        if let Some(super_name) = &class.super_name {
            queue.push_back(super_name.clone());
        }
        for interface in &class.interfaces {
            queue.push_back(interface.clone());
        }
    }

    false
}

fn is_known_executor_service_name(name: &str) -> bool {
    matches!(
        name,
        "java/util/concurrent/ExecutorService"
            | "java/util/concurrent/ScheduledExecutorService"
            | "java/util/concurrent/AbstractExecutorService"
            | "java/util/concurrent/ThreadPoolExecutor"
            | "java/util/concurrent/ScheduledThreadPoolExecutor"
            | "java/util/concurrent/ForkJoinPool"
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
            .filter(|result| {
                result.rule_id.as_deref() == Some("EXECUTOR_SERVICE_NOT_SHUTDOWN")
            })
            .filter_map(|result| result.message.text.clone())
            .collect()
    }

    #[test]
    fn reports_factory_created_executor_without_shutdown() {
        let sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;

import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

public class ClassA {
    public void methodX() {
        ExecutorService varOne = Executors.newSingleThreadExecutor();
        varOne.submit(() -> {});
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_sources(sources);
        assert_eq!(messages.len(), 1);
        assert!(messages[0].contains("may exit without shutdown"));
    }

    #[test]
    fn does_not_report_executor_shutdown_in_finally() {
        let sources = vec![SourceFile {
            path: "com/example/ClassB.java".to_string(),
            contents: r#"
package com.example;

import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

public class ClassB {
    public void methodY() {
        ExecutorService varOne = Executors.newFixedThreadPool(1);
        try {
            varOne.submit(() -> {});
        } finally {
            varOne.shutdown();
        }
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_sources(sources);
        assert!(messages.is_empty(), "did not expect finding: {messages:?}");
    }

    #[test]
    fn reports_early_return_before_shutdown() {
        let sources = vec![SourceFile {
            path: "com/example/ClassC.java".to_string(),
            contents: r#"
package com.example;

import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

public class ClassC {
    public void methodZ(boolean varOne) {
        ExecutorService varTwo = Executors.newSingleThreadExecutor();
        if (varOne) {
            return;
        }
        varTwo.shutdown();
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_sources(sources);
        assert_eq!(messages.len(), 1);
    }

    #[test]
    fn does_not_report_executor_stored_to_field() {
        let sources = vec![SourceFile {
            path: "com/example/ClassD.java".to_string(),
            contents: r#"
package com.example;

import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

public class ClassD {
    private ExecutorService varOne;

    public void methodW() {
        ExecutorService varTwo = Executors.newSingleThreadExecutor();
        this.varOne = varTwo;
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_sources(sources);
        assert!(messages.is_empty(), "did not expect finding: {messages:?}");
    }

    #[test]
    fn does_not_report_executor_returned_to_caller() {
        let sources = vec![SourceFile {
            path: "com/example/ClassE.java".to_string(),
            contents: r#"
package com.example;

import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

public class ClassE {
    public ExecutorService methodV() {
        ExecutorService varOne = Executors.newSingleThreadExecutor();
        return varOne;
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_sources(sources);
        assert!(messages.is_empty(), "did not expect finding: {messages:?}");
    }

    #[test]
    fn does_not_report_executor_passed_to_helper() {
        let sources = vec![SourceFile {
            path: "com/example/ClassF.java".to_string(),
            contents: r#"
package com.example;

import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

public class ClassF {
    public void methodU() {
        ExecutorService varOne = Executors.newSingleThreadExecutor();
        methodT(varOne);
    }

    private void methodT(ExecutorService varOne) {}
}
"#
            .to_string(),
        }];

        let messages = analyze_sources(sources);
        assert!(messages.is_empty(), "did not expect finding: {messages:?}");
    }

    #[test]
    fn does_not_report_try_with_resources_close() {
        let sources = vec![SourceFile {
            path: "com/example/ClassG.java".to_string(),
            contents: r#"
package com.example;

import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

public class ClassG {
    public void methodS() {
        try (ExecutorService varOne = Executors.newVirtualThreadPerTaskExecutor()) {
            varOne.submit(() -> {});
        }
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_sources(sources);
        assert!(messages.is_empty(), "did not expect finding: {messages:?}");
    }
}
