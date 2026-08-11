use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use anyhow::Result;
use serde_sarif::sarif::Result as SarifResult;

use crate::engine::AnalysisContext;
use crate::ir::{CallSite, Class, Method};
use crate::rules::{
    EvidenceLimits, EvidenceStep, ResultEvidence, Rule, RuleMetadata, method_location_with_line,
    result_message,
};

const BUILDERS_CLASS: &str = "kotlinx/coroutines/BuildersKt";
const SUSPEND_SHAPE_SUFFIX: &str = "Lkotlin/coroutines/Continuation;)Ljava/lang/Object;";
const SUSPEND_LAMBDA_SUPER: &str = "kotlin/coroutines/jvm/internal/SuspendLambda";
const RESTRICTED_SUSPEND_LAMBDA_SUPER: &str =
    "kotlin/coroutines/jvm/internal/RestrictedSuspendLambda";
const INVOKE_SUSPEND_NAME: &str = "invokeSuspend";
const INVOKE_SUSPEND_DESCRIPTOR: &str = "(Ljava/lang/Object;)Ljava/lang/Object;";
const MAX_RENDERED_FRAMES: usize = 10;
const EDGE_RENDERED_FRAMES: usize = 5;

/// Key identifying an analysis target method by class name, method name, and descriptor.
type MethodKey<'a> = (&'a str, &'a str, &'a str);

/// Rule that detects runBlocking call sites reachable from a coroutine through plain call chains.
#[derive(Default)]
pub(crate) struct RunBlockingReachableFromCoroutineRule;

crate::register_rule!(RunBlockingReachableFromCoroutineRule);

impl Rule for RunBlockingReachableFromCoroutineRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "RUN_BLOCKING_REACHABLE_FROM_COROUTINE",
            name: "runBlocking reachable from coroutine",
            description: "runBlocking calls reachable from a coroutine through plain function calls block a shared coroutine thread",
        }
    }

    fn run(&self, context: &AnalysisContext) -> Result<Vec<SarifResult>> {
        if !context.has_kotlinx_coroutines() {
            return Ok(Vec::new());
        }

        let mut roots: Vec<MethodKey> = Vec::new();
        let mut has_reportable_sink = false;
        for class in context.analysis_target_classes() {
            for method in &class.methods {
                if is_coroutine_root(class, method) {
                    roots.push((
                        class.name.as_str(),
                        method.name.as_str(),
                        method.descriptor.as_str(),
                    ));
                }
                if !has_reportable_sink
                    && !matches_suspend_function_shape(&method.descriptor)
                    && method.calls.iter().any(is_run_blocking_call)
                {
                    has_reportable_sink = true;
                }
            }
        }
        if roots.is_empty() || !has_reportable_sink {
            return Ok(Vec::new());
        }

        let mut class_index: BTreeMap<&str, &Class> = BTreeMap::new();
        let mut declared_methods: BTreeSet<MethodKey> = BTreeSet::new();
        for class in context.analysis_target_classes() {
            class_index.insert(class.name.as_str(), class);
            for method in &class.methods {
                declared_methods.insert((
                    class.name.as_str(),
                    method.name.as_str(),
                    method.descriptor.as_str(),
                ));
            }
        }

        let mut successors: BTreeMap<MethodKey, BTreeSet<MethodKey>> = BTreeMap::new();
        for class in context.analysis_target_classes() {
            for method in &class.methods {
                let from = (
                    class.name.as_str(),
                    method.name.as_str(),
                    method.descriptor.as_str(),
                );
                for call in &method.calls {
                    if let Some(target) = resolve_call_target(&class_index, &declared_methods, call)
                    {
                        successors.entry(from).or_default().insert(target);
                    }
                }
            }
        }

        let parents = shortest_chain_parents(&roots, &successors);

        let mut findings = Vec::new();
        for class in context.analysis_target_classes() {
            let mut artifact_uri: Option<Option<String>> = None;
            for method in &class.methods {
                if matches_suspend_function_shape(&method.descriptor) {
                    continue;
                }
                let key = (
                    class.name.as_str(),
                    method.name.as_str(),
                    method.descriptor.as_str(),
                );
                if !parents.contains_key(&key) {
                    continue;
                }
                let mut chain: Option<Vec<MethodKey>> = None;
                for call in &method.calls {
                    if !is_run_blocking_call(call) {
                        continue;
                    }
                    let chain = chain.get_or_insert_with(|| chain_method_keys(key, &parents));
                    let frames: Vec<_> = chain.iter().copied().map(render_frame).collect();
                    findings.push(RuleFinding {
                        class_name: class.name.clone(),
                        method_name: method.name.clone(),
                        method_descriptor: method.descriptor.clone(),
                        artifact_uri: artifact_uri
                            .get_or_insert_with(|| context.class_artifact_uri(class))
                            .clone(),
                        line: method.line_for_offset(call.offset),
                        offset: call.offset,
                        message: build_message(&frames),
                        call_chain: materialize_call_chain(chain, &class_index, context),
                    });
                }
            }
        }

        findings.sort_by(|left, right| {
            left.class_name
                .cmp(&right.class_name)
                .then_with(|| left.method_name.cmp(&right.method_name))
                .then_with(|| left.method_descriptor.cmp(&right.method_descriptor))
                .then_with(|| left.offset.cmp(&right.offset))
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
                let related_locations: Vec<_> = finding
                    .call_chain
                    .iter()
                    .enumerate()
                    .map(|(index, frame)| frame.evidence_step(index))
                    .collect();
                let mut witness = related_locations.clone();
                witness.push(EvidenceStep::new(
                    location.clone(),
                    "runBlocking is invoked here.",
                    ["call", "danger"],
                    Some(finding.call_chain.len() as i64),
                ));
                let evidence = ResultEvidence::new(
                    "Coroutine call chain leading to runBlocking.",
                    "coroutine-call-chain",
                    related_locations,
                    witness,
                )
                .to_sarif(EvidenceLimits::default());
                SarifResult::builder()
                    .message(result_message(finding.message))
                    .locations(vec![location])
                    .related_locations(evidence.related_locations)
                    .code_flows(evidence.code_flows)
                    .build()
            })
            .collect())
    }
}

/// Finding data collected before SARIF conversion.
struct RuleFinding {
    class_name: String,
    method_name: String,
    method_descriptor: String,
    artifact_uri: Option<String>,
    line: Option<u32>,
    offset: u32,
    message: String,
    call_chain: Vec<CallChainFrame>,
}

/// Owned method metadata for one frame in the selected coroutine call chain.
#[derive(Clone, Debug)]
struct CallChainFrame {
    class_name: String,
    method_name: String,
    method_descriptor: String,
    artifact_uri: Option<String>,
    line: Option<u32>,
}

impl CallChainFrame {
    fn evidence_step(&self, nesting_level: usize) -> EvidenceStep {
        let display_name = format!(
            "{}.{}{}",
            self.class_name.replace('/', "."),
            self.method_name,
            self.method_descriptor
        );
        let message = if nesting_level == 0 {
            format!("Coroutine execution enters {display_name}.")
        } else {
            format!("The selected call chain reaches {display_name}.")
        };
        EvidenceStep::new(
            method_location_with_line(
                &self.class_name,
                &self.method_name,
                &self.method_descriptor,
                self.artifact_uri.as_deref(),
                self.line,
            ),
            message,
            ["enter", "function"],
            Some(nesting_level as i64),
        )
    }
}

fn materialize_call_chain(
    chain: &[MethodKey],
    class_index: &BTreeMap<&str, &Class>,
    context: &AnalysisContext,
) -> Vec<CallChainFrame> {
    chain
        .iter()
        .filter_map(|(class_name, method_name, descriptor)| {
            let class = class_index.get(class_name).copied()?;
            let method = class
                .methods
                .iter()
                .find(|method| method.name == *method_name && method.descriptor == *descriptor)?;
            Some(CallChainFrame {
                class_name: (*class_name).to_string(),
                method_name: (*method_name).to_string(),
                method_descriptor: (*descriptor).to_string(),
                artifact_uri: context.class_artifact_uri(class),
                line: method
                    .line_numbers
                    .iter()
                    .min_by_key(|entry| entry.start_pc)
                    .map(|entry| entry.line),
            })
        })
        .collect()
}

fn is_run_blocking_call(call: &CallSite) -> bool {
    call.owner == BUILDERS_CLASS
        && (call.name == "runBlocking" || call.name == "runBlocking$default")
}

fn matches_suspend_function_shape(descriptor: &str) -> bool {
    descriptor
        .strip_suffix(SUSPEND_SHAPE_SUFFIX)
        .is_some_and(|prefix| !prefix.ends_with('['))
}

fn is_coroutine_root(class: &Class, method: &Method) -> bool {
    if class.super_name.as_deref() == Some(RESTRICTED_SUSPEND_LAMBDA_SUPER) {
        return false;
    }
    if matches_suspend_function_shape(&method.descriptor) {
        return true;
    }
    method.name == INVOKE_SUSPEND_NAME
        && method.descriptor == INVOKE_SUSPEND_DESCRIPTOR
        && class.super_name.as_deref() == Some(SUSPEND_LAMBDA_SUPER)
}

/// Resolves a call edge to the exact named target within analysis target
/// classes, following JVM resolution order. The owner class is probed first,
/// then its superclass chain, then superinterfaces breadth first, so inherited
/// concrete methods and interface default methods both resolve. Edges never
/// expand to overriding implementations in subclasses.
fn resolve_call_target<'a>(
    class_index: &BTreeMap<&'a str, &'a Class>,
    declared_methods: &BTreeSet<MethodKey<'a>>,
    call: &'a CallSite,
) -> Option<MethodKey<'a>> {
    let owner: &'a Class = class_index.get(call.owner.as_str()).copied()?;
    let probe = (
        owner.name.as_str(),
        call.name.as_str(),
        call.descriptor.as_str(),
    );
    if let Some(found) = declared_methods.get(&probe) {
        return Some(*found);
    }
    resolve_inherited_call_target(class_index, declared_methods, call, owner)
}

/// Walks the superclass chain and then the superinterfaces of the call owner,
/// returning the nearest inherited declaration of the called method. Abstract
/// declarations resolve like any other declaration and act as dead ends,
/// which keeps the no-override stance.
fn resolve_inherited_call_target<'a>(
    class_index: &BTreeMap<&'a str, &'a Class>,
    declared_methods: &BTreeSet<MethodKey<'a>>,
    call: &'a CallSite,
    owner: &'a Class,
) -> Option<MethodKey<'a>> {
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    seen.insert(owner.name.as_str());
    let mut interfaces: VecDeque<&str> = owner.interfaces.iter().map(String::as_str).collect();
    let mut cursor = owner.super_name.as_deref();
    while let Some(class_name) = cursor {
        if !seen.insert(class_name) {
            break;
        }
        let Some(current) = class_index.get(class_name).copied() else {
            break;
        };
        let probe = (
            current.name.as_str(),
            call.name.as_str(),
            call.descriptor.as_str(),
        );
        if let Some(found) = declared_methods.get(&probe) {
            return Some(*found);
        }
        interfaces.extend(current.interfaces.iter().map(String::as_str));
        cursor = current.super_name.as_deref();
    }
    while let Some(interface_name) = interfaces.pop_front() {
        if !seen.insert(interface_name) {
            continue;
        }
        let Some(current) = class_index.get(interface_name).copied() else {
            continue;
        };
        let probe = (
            current.name.as_str(),
            call.name.as_str(),
            call.descriptor.as_str(),
        );
        if let Some(found) = declared_methods.get(&probe) {
            return Some(*found);
        }
        interfaces.extend(current.interfaces.iter().map(String::as_str));
    }
    None
}

/// Multi-source BFS from the root set. It records one parent pointer per
/// reachable method, with roots mapped to `None`, so every reachable method
/// has a shortest chain back to a root. When two same-level parents discover a
/// method, the parent whose materialized frame chain is lexicographically
/// smaller wins, which selects the lexicographically smallest frame sequence
/// among equally short chains.
fn shortest_chain_parents<'a>(
    roots: &[MethodKey<'a>],
    successors: &BTreeMap<MethodKey<'a>, BTreeSet<MethodKey<'a>>>,
) -> BTreeMap<MethodKey<'a>, Option<MethodKey<'a>>> {
    let mut parents: BTreeMap<MethodKey, Option<MethodKey>> = BTreeMap::new();
    let mut current: Vec<MethodKey> = Vec::new();
    for root in roots {
        if parents.contains_key(root) {
            continue;
        }
        parents.insert(*root, None);
        current.push(*root);
    }
    while !current.is_empty() {
        let mut next: BTreeMap<MethodKey, MethodKey> = BTreeMap::new();
        for from in &current {
            let Some(targets) = successors.get(from) else {
                continue;
            };
            for target in targets {
                if parents.contains_key(target) {
                    continue;
                }
                match next.entry(*target) {
                    Entry::Vacant(entry) => {
                        entry.insert(*from);
                    }
                    Entry::Occupied(mut entry) => {
                        if chain_frames(*from, &parents) < chain_frames(*entry.get(), &parents) {
                            entry.insert(*from);
                        }
                    }
                }
            }
        }
        current = next.keys().copied().collect();
        for (target, parent) in next {
            parents.insert(target, Some(parent));
        }
    }
    parents
}

/// Materializes the root-to-method frame chain for a reachable method by
/// walking parent pointers. Every parent sits one BFS level closer to a root,
/// so the walk terminates at a parentless root.
fn chain_frames<'a>(
    key: MethodKey<'a>,
    parents: &BTreeMap<MethodKey<'a>, Option<MethodKey<'a>>>,
) -> Vec<String> {
    chain_method_keys(key, parents)
        .into_iter()
        .map(render_frame)
        .collect()
}

fn chain_method_keys<'a>(
    key: MethodKey<'a>,
    parents: &BTreeMap<MethodKey<'a>, Option<MethodKey<'a>>>,
) -> Vec<MethodKey<'a>> {
    let mut frames = Vec::new();
    let mut cursor = Some(key);
    while let Some(current) = cursor {
        frames.push(current);
        cursor = *parents
            .get(&current)
            .expect("chain member must have a parent entry");
    }
    frames.reverse();
    frames
}

fn render_frame(key: MethodKey) -> String {
    format!("{}.{}", key.0.replace('/', "."), key.1)
}

fn render_chain(frames: &[String]) -> String {
    if frames.len() <= MAX_RENDERED_FRAMES {
        return frames.join(" -> ");
    }
    let mut parts: Vec<&str> = Vec::with_capacity(MAX_RENDERED_FRAMES + 1);
    parts.extend(frames[..EDGE_RENDERED_FRAMES].iter().map(String::as_str));
    parts.push("...");
    parts.extend(
        frames[frames.len() - EDGE_RENDERED_FRAMES..]
            .iter()
            .map(String::as_str),
    );
    parts.join(" -> ")
}

fn build_message(frames: &[String]) -> String {
    format!(
        "runBlocking is reachable from a coroutine and blocks a shared coroutine thread. Potential call stack: {} -> runBlocking. Call the suspend code directly instead of wrapping it in runBlocking.",
        render_chain(frames)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::descriptor::method_param_count;
    use crate::engine::{EngineOutput, build_context};
    use crate::ir::{
        CallKind, CallSite, Class, ControlFlowGraph, Method, MethodAccess, MethodNullness,
    };
    use crate::test_harness::{CompileOutput, JvmTestHarness, Language, SourceFile};

    const RULE_ID: &str = "RUN_BLOCKING_REACHABLE_FROM_COROUTINE";
    const RUN_BLOCKING_DESCRIPTOR: &str =
        "(Lkotlin/coroutines/CoroutineContext;Lkotlin/jvm/functions/Function2;)Ljava/lang/Object;";

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
                path: "kotlinx/coroutines/CoroutineScope.kt".to_string(),
                contents: r#"
package kotlinx.coroutines

interface CoroutineScope

class Job
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

fun <T> runBlocking(
    context: CoroutineContext = EmptyCoroutineContext,
    block: suspend CoroutineScope.() -> T,
): T {
    throw UnsupportedOperationException()
}

fun CoroutineScope.launch(block: suspend CoroutineScope.() -> Unit): Job {
    throw UnsupportedOperationException()
}
"#
                .to_string(),
            },
        ]
    }

    /// Compiles the kotlinx.coroutines stubs into a classpath entry. The
    /// returned compile output owns the compiled directory, so it must stay
    /// alive while the classpath is in use.
    fn compile_stub_classpath(harness: &JvmTestHarness) -> CompileOutput {
        harness
            .compile(Language::Kotlin, &coroutines_stub_sources(), &[])
            .expect("compile coroutine stubs")
    }

    /// Compiles the kotlinx.coroutines stubs into a separate classpath entry and
    /// analyzes the given source as the only analysis target, mirroring real
    /// projects where kotlinx.coroutines is a dependency jar.
    fn analyze_with_stub_classpath(path: &str, contents: &str) -> EngineOutput {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let stubs = compile_stub_classpath(&harness);
        let classpath = vec![stubs.classes_dir().to_path_buf()];
        let sources = vec![SourceFile {
            path: path.to_string(),
            contents: contents.to_string(),
        }];
        harness
            .compile_and_analyze(Language::Kotlin, &sources, &classpath)
            .expect("run harness analysis")
    }

    fn expected_message(chain: &str) -> String {
        format!(
            "runBlocking is reachable from a coroutine and blocks a shared coroutine thread. Potential call stack: {chain} -> runBlocking. Call the suspend code directly instead of wrapping it in runBlocking."
        )
    }

    fn ir_method(name: &str, descriptor: &str, calls: Vec<CallSite>) -> Method {
        Method {
            name: name.to_string(),
            descriptor: descriptor.to_string(),
            signature: None,
            access: MethodAccess {
                is_public: true,
                is_static: false,
                is_synchronized: false,
                is_abstract: false,
                is_synthetic: false,
                is_bridge: false,
            },
            nullness: MethodNullness::unknown(
                method_param_count(descriptor).expect("parse method descriptor"),
            ),
            type_use: None,
            bytecode: Vec::new(),
            line_numbers: Vec::new(),
            cfg: ControlFlowGraph {
                blocks: Vec::new(),
                edges: Vec::new(),
            },
            calls,
            string_literals: Vec::new(),
            exception_handlers: Vec::new(),
            local_variables: Vec::new(),
            local_variable_types: Vec::new(),
        }
    }

    fn ir_class(
        name: &str,
        super_name: Option<&str>,
        referenced_classes: Vec<&str>,
        methods: Vec<Method>,
    ) -> Class {
        ir_class_with_interfaces(name, super_name, Vec::new(), referenced_classes, methods)
    }

    fn ir_class_with_interfaces(
        name: &str,
        super_name: Option<&str>,
        interfaces: Vec<&str>,
        referenced_classes: Vec<&str>,
        methods: Vec<Method>,
    ) -> Class {
        Class {
            name: name.to_string(),
            source_file: None,
            super_name: super_name.map(ToOwned::to_owned),
            interfaces: interfaces.into_iter().map(ToOwned::to_owned).collect(),
            type_parameters: Vec::new(),
            referenced_classes: referenced_classes
                .into_iter()
                .map(ToOwned::to_owned)
                .collect(),
            fields: Vec::new(),
            methods,
            annotation_defaults: Vec::new(),
            artifact_index: 0,
            is_record: false,
        }
    }

    fn ir_call(owner: &str, name: &str, descriptor: &str, offset: u32) -> CallSite {
        CallSite {
            owner: owner.to_string(),
            name: name.to_string(),
            descriptor: descriptor.to_string(),
            kind: CallKind::Static,
            offset,
        }
    }

    #[test]
    fn skips_when_kotlinx_coroutines_absent() {
        let context = build_context(
            vec![ir_class("com/example/ClassA", None, Vec::new(), Vec::new())],
            &[],
        );

        assert!(!context.has_kotlinx_coroutines());
        let results = RunBlockingReachableFromCoroutineRule
            .run(&context)
            .expect("run rule without framework");
        assert!(results.is_empty());
    }

    #[test]
    fn resolves_call_edges_through_superclass_chain() {
        let classes = vec![
            ir_class(
                "com/example/ClassA",
                Some("java/lang/Object"),
                Vec::new(),
                vec![ir_method(
                    "methodX",
                    "(Lkotlin/coroutines/Continuation;)Ljava/lang/Object;",
                    vec![ir_call("com/example/ClassChild", "methodY", "()V", 0)],
                )],
            ),
            ir_class(
                "com/example/ClassChild",
                Some("com/example/ClassBase"),
                Vec::new(),
                Vec::new(),
            ),
            ir_class(
                "com/example/ClassBase",
                Some("java/lang/Object"),
                vec!["kotlinx/coroutines/BuildersKt"],
                vec![ir_method(
                    "methodY",
                    "()V",
                    vec![ir_call(
                        "kotlinx/coroutines/BuildersKt",
                        "runBlocking",
                        RUN_BLOCKING_DESCRIPTOR,
                        4,
                    )],
                )],
            ),
        ];
        let context = build_context(classes, &[]);

        assert!(context.has_kotlinx_coroutines());
        let results = RunBlockingReachableFromCoroutineRule
            .run(&context)
            .expect("run rule");
        let messages: Vec<_> = results
            .iter()
            .filter_map(|result| result.message.text.clone())
            .collect();
        assert_eq!(messages.len(), 1, "expected one finding, got {messages:?}");
        assert_eq!(
            messages[0],
            expected_message("com.example.ClassA.methodX -> com.example.ClassBase.methodY")
        );
    }

    #[test]
    fn resolves_call_edges_through_interface_default_method() {
        let classes = vec![
            ir_class(
                "com/example/ClassA",
                Some("java/lang/Object"),
                Vec::new(),
                vec![ir_method(
                    "methodX",
                    "(Lkotlin/coroutines/Continuation;)Ljava/lang/Object;",
                    vec![ir_call("com/example/ClassImpl", "methodY", "()V", 0)],
                )],
            ),
            ir_class_with_interfaces(
                "com/example/ClassImpl",
                Some("java/lang/Object"),
                vec!["com/example/InterfaceBase"],
                Vec::new(),
                Vec::new(),
            ),
            ir_class(
                "com/example/InterfaceBase",
                Some("java/lang/Object"),
                vec!["kotlinx/coroutines/BuildersKt"],
                vec![ir_method(
                    "methodY",
                    "()V",
                    vec![ir_call(
                        "kotlinx/coroutines/BuildersKt",
                        "runBlocking",
                        RUN_BLOCKING_DESCRIPTOR,
                        4,
                    )],
                )],
            ),
        ];
        let context = build_context(classes, &[]);

        let results = RunBlockingReachableFromCoroutineRule
            .run(&context)
            .expect("run rule");
        let messages: Vec<_> = results
            .iter()
            .filter_map(|result| result.message.text.clone())
            .collect();
        assert_eq!(messages.len(), 1, "expected one finding, got {messages:?}");
        assert_eq!(
            messages[0],
            expected_message("com.example.ClassA.methodX -> com.example.InterfaceBase.methodY")
        );
    }

    #[test]
    fn suspend_shape_requires_continuation_last_parameter() {
        assert!(matches_suspend_function_shape(
            "(Lkotlin/coroutines/Continuation;)Ljava/lang/Object;"
        ));
        assert!(matches_suspend_function_shape(
            "(ILkotlin/coroutines/Continuation;)Ljava/lang/Object;"
        ));
        assert!(!matches_suspend_function_shape(
            "([Lkotlin/coroutines/Continuation;)Ljava/lang/Object;"
        ));
        assert!(!matches_suspend_function_shape(
            "(I[[Lkotlin/coroutines/Continuation;)Ljava/lang/Object;"
        ));
    }

    #[test]
    fn does_not_expand_edges_to_overriding_subclass() {
        let classes = vec![
            ir_class(
                "com/example/ClassA",
                Some("java/lang/Object"),
                Vec::new(),
                vec![ir_method(
                    "methodX",
                    "(Lkotlin/coroutines/Continuation;)Ljava/lang/Object;",
                    vec![ir_call("com/example/ClassBase", "methodY", "()V", 0)],
                )],
            ),
            ir_class(
                "com/example/ClassBase",
                Some("java/lang/Object"),
                vec!["kotlinx/coroutines/BuildersKt"],
                vec![ir_method("methodY", "()V", Vec::new())],
            ),
            ir_class(
                "com/example/ClassChild",
                Some("com/example/ClassBase"),
                Vec::new(),
                vec![ir_method(
                    "methodY",
                    "()V",
                    vec![ir_call(
                        "kotlinx/coroutines/BuildersKt",
                        "runBlocking",
                        RUN_BLOCKING_DESCRIPTOR,
                        4,
                    )],
                )],
            ),
        ];
        let context = build_context(classes, &[]);

        let results = RunBlockingReachableFromCoroutineRule
            .run(&context)
            .expect("run rule");
        assert!(
            results.is_empty(),
            "did not expect findings through overriding implementations"
        );
    }

    #[test]
    fn reports_suspend_chain_through_plain_helper() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let stubs = compile_stub_classpath(&harness);
        let classpath = vec![stubs.classes_dir().to_path_buf()];
        let sources = vec![SourceFile {
            path: "com/example/FileOne.kt".to_string(),
            contents: r#"
package com.example

import kotlinx.coroutines.runBlocking

suspend fun methodOne() {
    methodTwo()
}

fun methodTwo() {
    runBlocking { }
}
"#
            .to_string(),
        }];
        let compiled = harness
            .compile(Language::Kotlin, &sources, &classpath)
            .expect("compile sources");
        let output = harness
            .analyze(compiled.classes_dir(), &classpath)
            .expect("run harness analysis");

        let messages = rule_messages(&output);
        assert_eq!(messages.len(), 1, "expected one finding, got {messages:?}");
        assert_eq!(
            messages[0],
            expected_message("com.example.FileOneKt.methodOne -> com.example.FileOneKt.methodTwo")
        );
        let finding = output
            .results
            .iter()
            .find(|result| result.rule_id.as_deref() == Some(RULE_ID))
            .expect("finding present");
        let start_line = finding
            .locations
            .as_ref()
            .and_then(|locations| locations.first())
            .and_then(|location| location.physical_location.as_ref())
            .and_then(|physical| physical.region.as_ref())
            .and_then(|region| region.start_line);
        assert!(start_line.is_some(), "expected call site line information");
        let related_locations = finding
            .related_locations
            .as_ref()
            .expect("call-chain related locations");
        assert_eq!(related_locations.len(), 2);
        assert_eq!(
            related_locations
                .iter()
                .map(|location| location.id)
                .collect::<Vec<_>>(),
            [Some(0), Some(1)]
        );
        let flow_locations = &finding.code_flows.as_ref().expect("call-chain code flow")[0]
            .thread_flows[0]
            .locations;
        assert_eq!(flow_locations.len(), 3);
        assert_eq!(
            flow_locations
                .iter()
                .map(|location| location.execution_order)
                .collect::<Vec<_>>(),
            [Some(0), Some(1), Some(2)]
        );
        let flow_names: Vec<_> = flow_locations
            .iter()
            .filter_map(|step| step.location.as_ref())
            .filter_map(|location| location.logical_locations.as_ref())
            .filter_map(|locations| locations.first())
            .filter_map(|location| location.name.as_deref())
            .collect();
        assert!(flow_names[0].contains("methodOne"));
        assert!(flow_names[1].contains("methodTwo"));
        assert!(flow_names[2].contains("methodTwo"));

        let second_output = harness
            .analyze(compiled.classes_dir(), &classpath)
            .expect("run harness analysis again");
        assert_eq!(
            messages,
            rule_messages(&second_output),
            "repeated analysis must be deterministic"
        );
        let second_finding = second_output
            .results
            .iter()
            .find(|result| result.rule_id.as_deref() == Some(RULE_ID))
            .expect("second finding present");
        assert_eq!(
            serde_json::to_value(finding.code_flows.as_ref()).expect("serialize first evidence"),
            serde_json::to_value(second_finding.code_flows.as_ref())
                .expect("serialize second evidence"),
            "ordered evidence must be deterministic"
        );
    }

    #[test]
    fn reports_depth_two_chain_with_explicit_context() {
        let output = analyze_with_stub_classpath(
            "com/example/FileOne.kt",
            r#"
package com.example

import kotlin.coroutines.EmptyCoroutineContext
import kotlinx.coroutines.runBlocking

suspend fun methodOne() {
    methodTwo()
}

fun methodTwo() {
    methodThree()
}

fun methodThree() {
    runBlocking(EmptyCoroutineContext) { }
}
"#,
        );
        let messages = rule_messages(&output);
        assert_eq!(messages.len(), 1, "expected one finding, got {messages:?}");
        assert_eq!(
            messages[0],
            expected_message(
                "com.example.FileOneKt.methodOne -> com.example.FileOneKt.methodTwo -> com.example.FileOneKt.methodThree"
            )
        );
    }

    #[test]
    fn reports_helper_called_from_builder_lambda() {
        let output = analyze_with_stub_classpath(
            "com/example/FileOne.kt",
            r#"
package com.example

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking

fun methodOne(varOne: CoroutineScope) {
    varOne.launch { methodTwo() }
}

fun methodTwo() {
    runBlocking { }
}
"#,
        );
        let messages = rule_messages(&output);
        assert_eq!(messages.len(), 1, "expected one finding, got {messages:?}");
        assert_eq!(
            messages[0],
            expected_message(
                "com.example.FileOneKt$methodOne$1.invokeSuspend -> com.example.FileOneKt.methodTwo"
            )
        );
    }

    #[test]
    fn reports_run_blocking_directly_inside_builder_lambda() {
        let output = analyze_with_stub_classpath(
            "com/example/FileOne.kt",
            r#"
package com.example

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking

fun methodOne(varOne: CoroutineScope) {
    varOne.launch { runBlocking { } }
}
"#,
        );
        let messages = rule_messages(&output);
        assert_eq!(messages.len(), 1, "expected one finding, got {messages:?}");
        assert_eq!(
            messages[0],
            expected_message("com.example.FileOneKt$methodOne$1.invokeSuspend")
        );
    }

    #[test]
    fn ignores_run_blocking_from_plain_entry_point() {
        let output = analyze_with_stub_classpath(
            "com/example/FileOne.kt",
            r#"
package com.example

import kotlinx.coroutines.runBlocking

fun methodOne() {
    runBlocking { methodTwo() }
}

suspend fun methodTwo() { }
"#,
        );
        let messages = rule_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect findings for top-level runBlocking: {messages:?}"
        );
    }

    #[test]
    fn ignores_run_blocking_inside_suspend_function() {
        let output = analyze_with_stub_classpath(
            "com/example/FileOne.kt",
            r#"
package com.example

import kotlinx.coroutines.runBlocking

suspend fun methodOne() {
    methodTwo()
}

suspend fun methodTwo() {
    runBlocking { }
}
"#,
        );
        let messages = rule_messages(&output);
        assert!(
            messages.is_empty(),
            "call sites inside suspend functions belong to the sibling rule: {messages:?}"
        );
    }

    #[test]
    fn ignores_thread_boundary() {
        let output = analyze_with_stub_classpath(
            "com/example/FileOne.kt",
            r#"
package com.example

import kotlinx.coroutines.runBlocking

suspend fun methodOne() {
    Thread { methodTwo() }.start()
}

fun methodTwo() {
    runBlocking { }
}
"#,
        );
        let messages = rule_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect findings across a thread boundary: {messages:?}"
        );
    }

    #[test]
    fn ignores_restricted_suspend_lambda_bodies() {
        let output = analyze_with_stub_classpath(
            "com/example/FileOne.kt",
            r#"
package com.example

import kotlinx.coroutines.runBlocking

fun methodOne(): Sequence<Int> = sequence {
    methodTwo()
    yield(1)
}

fun methodTwo() {
    runBlocking { }
}
"#,
        );
        let messages = rule_messages(&output);
        assert!(
            messages.is_empty(),
            "restricted suspension builders are not coroutine roots: {messages:?}"
        );
    }

    #[test]
    fn reports_single_finding_for_two_roots() {
        let output = analyze_with_stub_classpath(
            "com/example/FileOne.kt",
            r#"
package com.example

import kotlinx.coroutines.runBlocking

suspend fun methodBeta() {
    methodShared()
}

suspend fun methodAlpha() {
    methodShared()
}

fun methodShared() {
    runBlocking { }
}
"#,
        );
        let messages = rule_messages(&output);
        assert_eq!(messages.len(), 1, "expected one finding, got {messages:?}");
        assert_eq!(
            messages[0],
            expected_message(
                "com.example.FileOneKt.methodAlpha -> com.example.FileOneKt.methodShared"
            )
        );
    }

    #[test]
    fn terminates_on_call_cycle() {
        let output = analyze_with_stub_classpath(
            "com/example/FileOne.kt",
            r#"
package com.example

import kotlinx.coroutines.runBlocking

suspend fun methodOne() {
    methodTwo()
}

fun methodTwo() {
    methodThree()
}

fun methodThree() {
    methodTwo()
    runBlocking { }
}
"#,
        );
        let messages = rule_messages(&output);
        assert_eq!(messages.len(), 1, "expected one finding, got {messages:?}");
        assert_eq!(
            messages[0],
            expected_message(
                "com.example.FileOneKt.methodOne -> com.example.FileOneKt.methodTwo -> com.example.FileOneKt.methodThree"
            )
        );
    }

    #[test]
    fn elides_middle_frames_beyond_ten() {
        let mut contents = String::from(
            "package com.example\n\nimport kotlinx.coroutines.runBlocking\n\nsuspend fun methodB01() {\n    methodB02()\n}\n",
        );
        for index in 2..12 {
            contents.push_str(&format!(
                "\nfun methodB{index:02}() {{\n    methodB{next:02}()\n}}\n",
                next = index + 1
            ));
        }
        contents.push_str("\nfun methodB12() {\n    runBlocking { }\n}\n");
        let output = analyze_with_stub_classpath("com/example/FileOne.kt", &contents);
        let messages = rule_messages(&output);
        assert_eq!(messages.len(), 1, "expected one finding, got {messages:?}");
        assert_eq!(
            messages[0],
            expected_message(
                "com.example.FileOneKt.methodB01 -> com.example.FileOneKt.methodB02 -> com.example.FileOneKt.methodB03 -> com.example.FileOneKt.methodB04 -> com.example.FileOneKt.methodB05 -> ... -> com.example.FileOneKt.methodB08 -> com.example.FileOneKt.methodB09 -> com.example.FileOneKt.methodB10 -> com.example.FileOneKt.methodB11 -> com.example.FileOneKt.methodB12"
            )
        );
        assert!(!messages[0].contains("methodB06"));
        assert!(!messages[0].contains("methodB07"));
    }

    #[test]
    fn reports_sink_inherited_through_superclass_chain() {
        let output = analyze_with_stub_classpath(
            "com/example/FileOne.kt",
            r#"
package com.example

import kotlinx.coroutines.runBlocking

open class ClassBase {
    fun methodHelper() {
        runBlocking { }
    }
}

class ClassChild : ClassBase()

suspend fun methodOne(varOne: ClassChild) {
    varOne.methodHelper()
}
"#,
        );
        let messages = rule_messages(&output);
        assert_eq!(messages.len(), 1, "expected one finding, got {messages:?}");
        assert_eq!(
            messages[0],
            expected_message(
                "com.example.FileOneKt.methodOne -> com.example.ClassBase.methodHelper"
            )
        );
    }

    #[test]
    fn ignores_classpath_only_chains() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let mut dependency_sources = coroutines_stub_sources();
        dependency_sources.push(SourceFile {
            path: "com/dependency/FileDep.kt".to_string(),
            contents: r#"
package com.dependency

import kotlinx.coroutines.runBlocking

fun methodDep() {
    runBlocking { }
}
"#
            .to_string(),
        });
        let dependency_output = harness
            .compile(Language::Kotlin, &dependency_sources, &[])
            .expect("compile dependency classes");
        let classpath = vec![dependency_output.classes_dir().to_path_buf()];

        let app_sources = vec![SourceFile {
            path: "com/example/FileApp.kt".to_string(),
            contents: r#"
package com.example

import com.dependency.methodDep

suspend fun methodOne() {
    methodDep()
}
"#
            .to_string(),
        }];
        let output = harness
            .compile_and_analyze(Language::Kotlin, &app_sources, &classpath)
            .expect("run harness analysis");
        let messages = rule_messages(&output);
        assert!(
            messages.is_empty(),
            "chains through classpath-only classes must not report: {messages:?}"
        );
    }

    #[test]
    fn java_only_input_is_silent() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;

public class ClassA {
    public void methodX() {
        System.out.println("varOne");
    }
}
"#
            .to_string(),
        }];
        let output = harness
            .compile_and_analyze(Language::Java, &sources, &[])
            .expect("run harness analysis");
        let messages = rule_messages(&output);
        assert!(
            messages.is_empty(),
            "Java-only input must be silent: {messages:?}"
        );
    }
}
