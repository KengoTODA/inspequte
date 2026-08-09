use std::collections::HashMap;
use std::str::FromStr;

use anyhow::{Context, Result};
use jdescriptor::{MethodDescriptor, TypeDescriptor};
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::dataflow::opcode_semantics::{
    ApplyOutcome, SemanticsCoverage, SemanticsDebugConfig, SemanticsHooks, ValueDomain,
    apply_semantics, opcode_semantics_debug_enabled,
};
use crate::dataflow::stack_machine::StackMachine;
use crate::engine::AnalysisContext;
use crate::ir::{CallKind, CallSite, Instruction, InstructionKind, Method};
use crate::opcodes;
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

const RULE_ID: &str = "COROUTINE_CONTEXT_WITH_JOB";
const COROUTINE_CONTEXT_CLASS: &str = "kotlin/coroutines/CoroutineContext";
const CONTEXT_PLUS_DESCRIPTOR: &str =
    "(Lkotlin/coroutines/CoroutineContext;)Lkotlin/coroutines/CoroutineContext;";

/// Rule that detects coroutine builder and withContext calls whose context contains a Job element.
#[derive(Default)]
pub(crate) struct CoroutineContextWithJobRule;

crate::register_rule!(CoroutineContextWithJobRule);

impl Rule for CoroutineContextWithJobRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "COROUTINE_CONTEXT_WITH_JOB",
            name: "Coroutine context with Job",
            description: "Coroutine builder and withContext calls that pass a CoroutineContext containing a Job element break structured concurrency",
        }
    }

    fn run(&self, context: &AnalysisContext) -> Result<Vec<SarifResult>> {
        if !context.has_kotlinx_coroutines() {
            return Ok(Vec::new());
        }

        let mut findings = Vec::new();
        for class in context.analysis_target_classes() {
            let mut attributes = vec![KeyValue::new("inspequte.class", class.name.clone())];
            if let Some(uri) = context.class_artifact_uri(class) {
                attributes.push(KeyValue::new("inspequte.artifact_uri", uri));
            }
            let class_findings = context.with_span(
                "scan.class",
                &attributes,
                || -> Result<Vec<RuleFinding>> {
                    let artifact_uri = context.class_artifact_uri(class);
                    let mut class_findings = Vec::new();
                    for method in &class.methods {
                        if method.bytecode.is_empty() {
                            continue;
                        }
                        class_findings.extend(analyze_method(
                            &class.name,
                            method,
                            artifact_uri.as_deref(),
                        )?);
                    }
                    Ok(class_findings)
                },
            )?;
            findings.extend(class_findings);
        }

        findings.sort_by(|left, right| {
            left.class_name
                .cmp(&right.class_name)
                .then(left.method_name.cmp(&right.method_name))
                .then(left.method_descriptor.cmp(&right.method_descriptor))
                .then(left.offset.cmp(&right.offset))
        });

        Ok(findings
            .into_iter()
            .map(|finding| {
                let location = method_location_with_line(
                    &finding.class_name,
                    &finding.method_name,
                    &finding.method_descriptor,
                    finding.artifact_uri.as_deref(),
                    finding.line,
                );
                SarifResult::builder()
                    .message(result_message(finding.message))
                    .locations(vec![location])
                    .build()
            })
            .collect())
    }
}

/// Job factory that produced a tracked context value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum JobFactory {
    Job,
    SupervisorJob,
}

impl JobFactory {
    fn display(self) -> &'static str {
        match self {
            JobFactory::Job => "Job()",
            JobFactory::SupervisorJob => "SupervisorJob()",
        }
    }
}

/// Abstract value tracked by the intra-method stack machine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TrackedValue {
    Unknown,
    Scalar,
    JobContext(JobFactory),
}

/// Value-domain adapter used by shared default opcode semantics.
struct ContextValueDomain;

impl ValueDomain<TrackedValue> for ContextValueDomain {
    fn unknown_value(&self) -> TrackedValue {
        TrackedValue::Unknown
    }

    fn scalar_value(&self) -> TrackedValue {
        TrackedValue::Scalar
    }
}

/// Semantics hook that keeps tracked values intact across checkcast instructions.
struct PreserveCheckcastHook;

impl SemanticsHooks<TrackedValue> for PreserveCheckcastHook {
    fn pre_apply(
        &mut self,
        _machine: &mut StackMachine<TrackedValue>,
        _method: &Method,
        _offset: usize,
        opcode: u8,
    ) -> ApplyOutcome {
        if opcode == opcodes::CHECKCAST {
            // checkcast keeps the same value on the stack; the default
            // semantics would replace it with an unknown value and lose taint.
            ApplyOutcome::Applied
        } else {
            ApplyOutcome::NotHandled
        }
    }
}

/// One offending builder or withContext call site.
#[derive(Clone, Debug, Eq, PartialEq)]
struct RuleFinding {
    class_name: String,
    method_name: String,
    method_descriptor: String,
    artifact_uri: Option<String>,
    line: Option<u32>,
    offset: u32,
    message: String,
}

/// Cached descriptor summary used to avoid repeated parse work in invoke handling.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CallDescriptorSummary {
    arg_count: usize,
    has_return: bool,
    context_param_index: Option<usize>,
}

fn analyze_method(
    class_name: &str,
    method: &Method,
    artifact_uri: Option<&str>,
) -> Result<Vec<RuleFinding>> {
    let instructions = collect_instructions(method);
    let mut findings = Vec::new();
    let mut machine = StackMachine::new(TrackedValue::Unknown);
    let domain = ContextValueDomain;
    let mut hooks = PreserveCheckcastHook;
    let mut coverage = SemanticsCoverage::default();
    let debug = SemanticsDebugConfig {
        enabled: opcode_semantics_debug_enabled(),
        rule_id: RULE_ID,
    };
    let mut descriptor_cache = HashMap::new();

    for instruction in &instructions {
        match &instruction.kind {
            InstructionKind::Invoke(call) => {
                let summary = call_descriptor_summary(&mut descriptor_cache, &call.descriptor)?;
                if let Some(builder) = builder_name(call)
                    && let Some(context_index) = summary.context_param_index
                {
                    let stack = machine.stack_values();
                    if stack.len() >= summary.arg_count
                        && let TrackedValue::JobContext(factory) =
                            stack[stack.len() - summary.arg_count + context_index]
                    {
                        findings.push(RuleFinding {
                            class_name: class_name.to_string(),
                            method_name: method.name.clone(),
                            method_descriptor: method.descriptor.clone(),
                            artifact_uri: artifact_uri.map(ToOwned::to_owned),
                            line: method.line_for_offset(instruction.offset),
                            offset: instruction.offset,
                            message: finding_message(
                                builder,
                                class_name,
                                &method.name,
                                &method.descriptor,
                                factory,
                            ),
                        });
                    }
                }
                apply_invoke_transfer(&mut machine, call, summary);
            }
            InstructionKind::InvokeDynamic { descriptor, .. } => {
                let summary = call_descriptor_summary(&mut descriptor_cache, descriptor)?;
                machine.pop_n(summary.arg_count);
                machine.push(TrackedValue::Scalar);
            }
            _ => {
                apply_semantics(
                    &mut machine,
                    method,
                    instruction.offset as usize,
                    instruction.opcode,
                    &domain,
                    &mut hooks,
                    &mut coverage,
                    debug,
                );
            }
        }
    }

    Ok(findings)
}

fn finding_message(
    builder: &str,
    class_name: &str,
    method_name: &str,
    method_descriptor: &str,
    factory: JobFactory,
) -> String {
    format!(
        "{} call in {}.{}{} passes a coroutine context containing a Job element created by {}; the coroutine will not be a child of the calling scope, so cancellation and failure propagation break. Remove the Job element from the context, or create an explicit CoroutineScope if an independent lifecycle is intended.",
        builder,
        class_name,
        method_name,
        method_descriptor,
        factory.display(),
    )
}

fn apply_invoke_transfer(
    machine: &mut StackMachine<TrackedValue>,
    call: &CallSite,
    summary: CallDescriptorSummary,
) {
    let mut tainted_argument = None;
    for _ in 0..summary.arg_count {
        let value = machine.pop();
        if matches!(value, TrackedValue::JobContext(_)) {
            tainted_argument = Some(value);
        }
    }
    let receiver = if call.kind == CallKind::Static {
        None
    } else {
        Some(machine.pop())
    };

    let result = if let Some(factory) = job_factory(call) {
        Some(TrackedValue::JobContext(factory))
    } else if is_context_plus(call) {
        // plus keeps every element of both operands, so taint from either
        // the receiver or the argument survives in the combined context.
        match receiver {
            Some(value @ TrackedValue::JobContext(_)) => Some(value),
            _ => tainted_argument.or(Some(TrackedValue::Scalar)),
        }
    } else if summary.has_return {
        Some(TrackedValue::Scalar)
    } else {
        None
    };
    if let Some(value) = result {
        machine.push(value);
    }
}

fn collect_instructions(method: &Method) -> Vec<&Instruction> {
    let mut instructions: Vec<_> = method
        .cfg
        .blocks
        .iter()
        .flat_map(|block| block.instructions.iter())
        .collect();
    instructions.sort_by_key(|instruction| instruction.offset);
    instructions
}

fn stripped_default_name(name: &str) -> &str {
    name.strip_suffix("$default").unwrap_or(name)
}

fn job_factory(call: &CallSite) -> Option<JobFactory> {
    if call.kind != CallKind::Static {
        return None;
    }
    match (call.owner.as_str(), stripped_default_name(&call.name)) {
        ("kotlinx/coroutines/JobKt", "Job") => Some(JobFactory::Job),
        ("kotlinx/coroutines/SupervisorKt", "SupervisorJob") => Some(JobFactory::SupervisorJob),
        _ => None,
    }
}

fn builder_name(call: &CallSite) -> Option<&'static str> {
    if call.kind != CallKind::Static {
        return None;
    }
    match (call.owner.as_str(), stripped_default_name(&call.name)) {
        ("kotlinx/coroutines/BuildersKt", "launch") => Some("launch"),
        ("kotlinx/coroutines/BuildersKt", "async") => Some("async"),
        ("kotlinx/coroutines/BuildersKt", "withContext") => Some("withContext"),
        ("kotlinx/coroutines/channels/ProduceKt", "produce") => Some("produce"),
        ("kotlinx/coroutines/channels/ActorKt", "actor") => Some("actor"),
        _ => None,
    }
}

fn is_context_plus(call: &CallSite) -> bool {
    call.name == "plus" && call.descriptor == CONTEXT_PLUS_DESCRIPTOR
}

fn call_descriptor_summary<'a>(
    cache: &mut HashMap<&'a str, CallDescriptorSummary>,
    descriptor: &'a str,
) -> Result<CallDescriptorSummary> {
    if let Some(summary) = cache.get(descriptor) {
        return Ok(*summary);
    }

    let parsed = MethodDescriptor::from_str(descriptor).context("parse call descriptor")?;
    let context_param_index = parsed.parameter_types().iter().position(|param| {
        matches!(param, TypeDescriptor::Object(name) if name == COROUTINE_CONTEXT_CLASS)
    });
    let summary = CallDescriptorSummary {
        arg_count: parsed.parameter_types().len(),
        has_return: !matches!(parsed.return_type(), TypeDescriptor::Void),
        context_param_index,
    };
    cache.insert(descriptor, summary);
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{EngineOutput, build_context};
    use crate::descriptor::method_param_count;
    use crate::ir::{BasicBlock, Class, ControlFlowGraph, MethodAccess, MethodNullness};
    use crate::test_harness::{JvmTestHarness, Language, SourceFile};

    fn rule_messages(output: &EngineOutput) -> Vec<String> {
        output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some(RULE_ID))
            .filter_map(|result| result.message.text.clone())
            .collect()
    }

    fn coroutines_stub_sources() -> Vec<SourceFile> {
        vec![
            SourceFile {
                path: "kotlinx/coroutines/Job.kt".to_string(),
                contents: r#"
@file:JvmName("JobKt")

package kotlinx.coroutines

import kotlin.coroutines.AbstractCoroutineContextElement
import kotlin.coroutines.CoroutineContext

interface Job : CoroutineContext.Element {
    companion object Key : CoroutineContext.Key<Job>
    fun cancel()
}

interface CompletableJob : Job {
    suspend fun join()
}

private class JobImpl(private val parent: Job?) :
    AbstractCoroutineContextElement(Job), CompletableJob {
    override fun cancel() {}
    override suspend fun join() {}
}

fun Job(parent: Job? = null): CompletableJob = JobImpl(parent)
"#
                .to_string(),
            },
            SourceFile {
                path: "kotlinx/coroutines/Supervisor.kt".to_string(),
                contents: r#"
@file:JvmName("SupervisorKt")

package kotlinx.coroutines

fun SupervisorJob(parent: Job? = null): CompletableJob = Job(parent)
"#
                .to_string(),
            },
            SourceFile {
                path: "kotlinx/coroutines/CoroutineScope.kt".to_string(),
                contents: r#"
@file:JvmName("CoroutineScopeKt")

package kotlinx.coroutines

import kotlin.coroutines.CoroutineContext

interface CoroutineScope {
    val coroutineContext: CoroutineContext
}

private class ContextScope(override val coroutineContext: CoroutineContext) : CoroutineScope

fun CoroutineScope(context: CoroutineContext): CoroutineScope = ContextScope(context)
"#
                .to_string(),
            },
            SourceFile {
                path: "kotlinx/coroutines/CoroutineStart.kt".to_string(),
                contents: r#"
package kotlinx.coroutines

enum class CoroutineStart { DEFAULT, LAZY }
"#
                .to_string(),
            },
            SourceFile {
                path: "kotlinx/coroutines/Deferred.kt".to_string(),
                contents: r#"
package kotlinx.coroutines

interface Deferred<out T> : Job {
    suspend fun await(): T
}
"#
                .to_string(),
            },
            SourceFile {
                path: "kotlinx/coroutines/Dispatchers.kt".to_string(),
                contents: r#"
package kotlinx.coroutines

import kotlin.coroutines.AbstractCoroutineContextElement
import kotlin.coroutines.CoroutineContext

abstract class CoroutineDispatcher : AbstractCoroutineContextElement(Key) {
    companion object Key : CoroutineContext.Key<CoroutineDispatcher>
}

private class StubDispatcher : CoroutineDispatcher()

object Dispatchers {
    val Default: CoroutineDispatcher = StubDispatcher()
    val IO: CoroutineDispatcher = StubDispatcher()
}
"#
                .to_string(),
            },
            SourceFile {
                path: "kotlinx/coroutines/CoroutineName.kt".to_string(),
                contents: r#"
package kotlinx.coroutines

import kotlin.coroutines.AbstractCoroutineContextElement
import kotlin.coroutines.CoroutineContext

data class CoroutineName(val name: String) : AbstractCoroutineContextElement(CoroutineName) {
    companion object Key : CoroutineContext.Key<CoroutineName>
}
"#
                .to_string(),
            },
            SourceFile {
                path: "kotlinx/coroutines/Builders.kt".to_string(),
                contents: r#"
@file:JvmName("BuildersKt")

package kotlinx.coroutines

import kotlin.coroutines.CoroutineContext
import kotlin.coroutines.EmptyCoroutineContext

fun CoroutineScope.launch(
    context: CoroutineContext = EmptyCoroutineContext,
    start: CoroutineStart = CoroutineStart.DEFAULT,
    block: suspend CoroutineScope.() -> Unit,
): Job = TODO()

fun <T> CoroutineScope.async(
    context: CoroutineContext = EmptyCoroutineContext,
    start: CoroutineStart = CoroutineStart.DEFAULT,
    block: suspend CoroutineScope.() -> T,
): Deferred<T> = TODO()

suspend fun <T> withContext(
    context: CoroutineContext,
    block: suspend CoroutineScope.() -> T,
): T = TODO()
"#
                .to_string(),
            },
            SourceFile {
                path: "kotlinx/coroutines/channels/Produce.kt".to_string(),
                contents: r#"
@file:JvmName("ProduceKt")

package kotlinx.coroutines.channels

import kotlin.coroutines.CoroutineContext
import kotlin.coroutines.EmptyCoroutineContext
import kotlinx.coroutines.CoroutineScope

interface ReceiveChannel<out E>
interface SendChannel<in E>
interface ProducerScope<in E> : CoroutineScope, SendChannel<E>

fun <E> CoroutineScope.produce(
    context: CoroutineContext = EmptyCoroutineContext,
    capacity: Int = 0,
    block: suspend ProducerScope<E>.() -> Unit,
): ReceiveChannel<E> = TODO()
"#
                .to_string(),
            },
            SourceFile {
                path: "kotlinx/coroutines/channels/Actor.kt".to_string(),
                contents: r#"
@file:JvmName("ActorKt")

package kotlinx.coroutines.channels

import kotlin.coroutines.CoroutineContext
import kotlin.coroutines.EmptyCoroutineContext
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart

interface ActorScope<E> : CoroutineScope

fun <E> CoroutineScope.actor(
    context: CoroutineContext = EmptyCoroutineContext,
    capacity: Int = 0,
    start: CoroutineStart = CoroutineStart.DEFAULT,
    block: suspend ActorScope<E>.() -> Unit,
): SendChannel<E> = TODO()
"#
                .to_string(),
            },
        ]
    }

    fn compile_and_analyze(sources: Vec<SourceFile>) -> EngineOutput {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        harness
            .compile_and_analyze(Language::Kotlin, &sources, &[])
            .expect("run harness analysis")
    }

    fn sources_with(contents: &str) -> Vec<SourceFile> {
        let mut sources = coroutines_stub_sources();
        sources.push(SourceFile {
            path: "com/example/ClassA.kt".to_string(),
            contents: contents.to_string(),
        });
        sources
    }

    #[test]
    fn coroutine_context_with_job_skips_when_kotlinx_coroutines_is_absent() {
        let context = build_context(
            vec![Class {
                name: "com/example/ClassA".to_string(),
                source_file: None,
                super_name: None,
                interfaces: Vec::new(),
                type_parameters: Vec::new(),
                referenced_classes: Vec::new(),
                fields: Vec::new(),
                methods: Vec::new(),
                annotation_defaults: Vec::new(),
                artifact_index: 0,
                is_record: false,
            }],
            &[],
        );

        assert!(!context.has_kotlinx_coroutines());
        let results = CoroutineContextWithJobRule
            .run(&context)
            .expect("run rule without kotlinx.coroutines");
        assert!(results.is_empty());
    }

    #[test]
    fn coroutine_context_with_job_reports_job_passed_to_launch() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = sources_with(
            r#"
package com.example

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch

fun funOne(scope: CoroutineScope) {
    scope.launch(Job()) { }
}
"#,
        );
        let compiled = harness
            .compile(Language::Kotlin, &sources, &[])
            .expect("compile Kotlin sources");

        let first = harness
            .analyze(compiled.classes_dir(), &[])
            .expect("run first analysis");
        let second = harness
            .analyze(compiled.classes_dir(), &[])
            .expect("run second analysis");

        let messages = rule_messages(&first);
        assert_eq!(messages.len(), 1, "expected one finding, got {messages:?}");
        assert_eq!(
            messages[0],
            "launch call in com/example/ClassAKt.funOne(Lkotlinx/coroutines/CoroutineScope;)V \
             passes a coroutine context containing a Job element created by Job(); the coroutine \
             will not be a child of the calling scope, so cancellation and failure propagation \
             break. Remove the Job element from the context, or create an explicit \
             CoroutineScope if an independent lifecycle is intended."
        );
        assert_eq!(
            messages,
            rule_messages(&second),
            "findings must be deterministic across repeated runs"
        );
    }

    #[test]
    fn coroutine_context_with_job_reports_supervisor_job_passed_to_async() {
        let sources = sources_with(
            r#"
package com.example

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async

fun funOne(scope: CoroutineScope) {
    scope.async(SupervisorJob()) { }
}
"#,
        );
        let messages = rule_messages(&compile_and_analyze(sources));
        assert_eq!(messages.len(), 1, "expected one finding, got {messages:?}");
        assert!(messages[0].starts_with("async call in com/example/ClassAKt.funOne"));
        assert!(messages[0].contains("created by SupervisorJob()"));
    }

    #[test]
    fn coroutine_context_with_job_reports_job_passed_to_with_context() {
        let sources = sources_with(
            r#"
package com.example

import kotlinx.coroutines.Job
import kotlinx.coroutines.withContext

suspend fun funOne() {
    withContext(Job()) { }
}
"#,
        );
        let messages = rule_messages(&compile_and_analyze(sources));
        assert_eq!(messages.len(), 1, "expected one finding, got {messages:?}");
        assert!(messages[0].starts_with("withContext call in com/example/ClassAKt.funOne"));
        assert!(messages[0].contains("created by Job()"));
    }

    #[test]
    fn coroutine_context_with_job_reports_job_combined_through_plus_chain() {
        let sources = sources_with(
            r#"
package com.example

import kotlinx.coroutines.CoroutineName
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch

fun funOne(scope: CoroutineScope) {
    scope.launch(Dispatchers.IO + Job() + CoroutineName("x")) { }
}
"#,
        );
        let messages = rule_messages(&compile_and_analyze(sources));
        assert_eq!(messages.len(), 1, "expected one finding, got {messages:?}");
        assert!(messages[0].starts_with("launch call in com/example/ClassAKt.funOne"));
        assert!(messages[0].contains("created by Job()"));
    }

    #[test]
    fn coroutine_context_with_job_reports_job_stored_in_local_variable() {
        let sources = sources_with(
            r#"
package com.example

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch

fun funOne(scope: CoroutineScope) {
    val varOne = Job()
    scope.launch(varOne) { }
}
"#,
        );
        let messages = rule_messages(&compile_and_analyze(sources));
        assert_eq!(messages.len(), 1, "expected one finding, got {messages:?}");
        assert!(messages[0].starts_with("launch call in com/example/ClassAKt.funOne"));
        assert!(messages[0].contains("created by Job()"));
    }

    #[test]
    fn coroutine_context_with_job_ignores_context_without_job() {
        let sources = sources_with(
            r#"
package com.example

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch

fun funOne(scope: CoroutineScope) {
    scope.launch(Dispatchers.IO) { }
}

fun funTwo(scope: CoroutineScope) {
    scope.launch { }
}
"#,
        );
        let messages = rule_messages(&compile_and_analyze(sources));
        assert!(
            messages.is_empty(),
            "did not expect findings without a Job element: {messages:?}"
        );
    }

    #[test]
    fn coroutine_context_with_job_ignores_root_scope_construction() {
        let sources = sources_with(
            r#"
package com.example

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job

fun funOne(): CoroutineScope {
    return CoroutineScope(Job() + Dispatchers.IO)
}
"#,
        );
        let messages = rule_messages(&compile_and_analyze(sources));
        assert!(
            messages.is_empty(),
            "did not expect findings for root scope construction: {messages:?}"
        );
    }

    #[test]
    fn coroutine_context_with_job_ignores_job_used_only_for_lifecycle_control() {
        let sources = sources_with(
            r#"
package com.example

import kotlinx.coroutines.Job

suspend fun funOne() {
    val varOne = Job()
    varOne.cancel()
    varOne.join()
}
"#,
        );
        let messages = rule_messages(&compile_and_analyze(sources));
        assert!(
            messages.is_empty(),
            "did not expect findings when the job never reaches a builder: {messages:?}"
        );
    }

    #[test]
    fn coroutine_context_with_job_reports_only_offending_builder_call() {
        let sources = sources_with(
            r#"
package com.example

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch

fun funOne(scope: CoroutineScope) {
    scope.launch(Dispatchers.IO) { }
    scope.launch(Job()) { }
}
"#,
        );
        let messages = rule_messages(&compile_and_analyze(sources));
        assert_eq!(messages.len(), 1, "expected one finding, got {messages:?}");
        assert!(messages[0].starts_with("launch call in com/example/ClassAKt.funOne"));
        assert!(messages[0].contains("created by Job()"));
    }

    #[test]
    fn coroutine_context_with_job_reports_channel_builders() {
        let sources = sources_with(
            r#"
package com.example

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.channels.actor
import kotlinx.coroutines.channels.produce

fun funOne(scope: CoroutineScope) {
    scope.produce<Int>(Job()) { }
    scope.actor<Int>(SupervisorJob()) { }
}
"#,
        );
        let messages = rule_messages(&compile_and_analyze(sources));
        assert_eq!(messages.len(), 2, "expected two findings, got {messages:?}");
        assert!(messages.iter().any(|message| {
            message.starts_with("produce call in com/example/ClassAKt.funOne")
                && message.contains("created by Job()")
        }));
        assert!(messages.iter().any(|message| {
            message.starts_with("actor call in com/example/ClassAKt.funOne")
                && message.contains("created by SupervisorJob()")
        }));
    }

    #[test]
    fn coroutine_context_with_job_tracks_context_across_invokedynamic() {
        let sources = sources_with(
            r#"
package com.example

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch

fun funOne(scope: CoroutineScope) {
    val varOne = Runnable { }
    varOne.run()
    scope.launch(Job()) { }
}
"#,
        );
        let messages = rule_messages(&compile_and_analyze(sources));
        assert_eq!(messages.len(), 1, "expected one finding, got {messages:?}");
        assert!(messages[0].starts_with("launch call in com/example/ClassAKt.funOne"));
    }

    fn synthetic_method(name: &str, bytecode: Vec<u8>, cfg: ControlFlowGraph) -> Method {
        Method {
            name: name.to_string(),
            descriptor: "()V".to_string(),
            signature: None,
            access: MethodAccess {
                is_public: true,
                is_static: true,
                is_synchronized: false,
                is_abstract: false,
                is_synthetic: false,
                is_bridge: false,
            },
            nullness: MethodNullness::unknown(method_param_count("()V").expect("param count")),
            type_use: None,
            bytecode,
            line_numbers: Vec::new(),
            cfg,
            calls: Vec::new(),
            string_literals: Vec::new(),
            exception_handlers: Vec::new(),
            local_variables: Vec::new(),
            local_variable_types: Vec::new(),
        }
    }

    fn synthetic_class(methods: Vec<Method>) -> Class {
        Class {
            name: "com/example/ClassA".to_string(),
            source_file: None,
            super_name: None,
            interfaces: Vec::new(),
            type_parameters: Vec::new(),
            referenced_classes: vec!["kotlinx/coroutines/BuildersKt".to_string()],
            fields: Vec::new(),
            methods,
            annotation_defaults: Vec::new(),
            artifact_index: 0,
            is_record: false,
        }
    }

    #[test]
    fn coroutine_context_with_job_skips_methods_without_bytecode() {
        let cfg = ControlFlowGraph {
            blocks: Vec::new(),
            edges: Vec::new(),
        };
        let context = build_context(
            vec![synthetic_class(vec![synthetic_method(
                "methodX",
                Vec::new(),
                cfg,
            )])],
            &[],
        );

        assert!(context.has_kotlinx_coroutines());
        let results = CoroutineContextWithJobRule
            .run(&context)
            .expect("run rule over bytecode-less method");
        assert!(results.is_empty());
    }

    #[test]
    fn coroutine_context_with_job_propagates_malformed_call_descriptor_error() {
        let cfg = ControlFlowGraph {
            blocks: vec![BasicBlock {
                start_offset: 0,
                end_offset: 3,
                instructions: vec![Instruction {
                    offset: 0,
                    opcode: opcodes::INVOKESTATIC,
                    kind: InstructionKind::Invoke(CallSite {
                        owner: "com/example/ClassB".to_string(),
                        name: "methodY".to_string(),
                        descriptor: "(broken".to_string(),
                        kind: CallKind::Static,
                        offset: 0,
                    }),
                }],
            }],
            edges: Vec::new(),
        };
        let context = build_context(
            vec![synthetic_class(vec![synthetic_method(
                "methodX",
                vec![0],
                cfg,
            )])],
            &[],
        );

        let error = CoroutineContextWithJobRule
            .run(&context)
            .expect_err("malformed call descriptor must surface as an error");
        assert!(error.to_string().contains("parse call descriptor"));
    }
}
