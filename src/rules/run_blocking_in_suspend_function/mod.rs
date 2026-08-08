use std::str::FromStr;

use anyhow::Result;
use jdescriptor::{MethodDescriptor, TypeDescriptor};
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::engine::AnalysisContext;
use crate::ir::{CallKind, CallSite};
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

/// Rule that detects kotlinx.coroutines runBlocking calls inside compiled Kotlin suspend functions.
#[derive(Default)]
pub(crate) struct RunBlockingInSuspendFunctionRule;

crate::register_rule!(RunBlockingInSuspendFunctionRule);

const RUN_BLOCKING_OWNER: &str = "kotlinx/coroutines/BuildersKt";
const RUN_BLOCKING_DESCRIPTOR: &str =
    "(Lkotlin/coroutines/CoroutineContext;Lkotlin/jvm/functions/Function2;)Ljava/lang/Object;";
const RUN_BLOCKING_DEFAULT_DESCRIPTOR: &str =
    "(Lkotlin/coroutines/CoroutineContext;Lkotlin/jvm/functions/Function2;ILjava/lang/Object;)Ljava/lang/Object;";

impl Rule for RunBlockingInSuspendFunctionRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "RUN_BLOCKING_IN_SUSPEND_FUNCTION",
            name: "runBlocking in suspend function",
            description: "runBlocking inside a suspend function blocks the calling thread and defeats asynchronous execution",
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
            let class_findings =
                context.with_span("scan.class", &attributes, || -> Vec<RuleFinding> {
                    let artifact_uri = context.class_artifact_uri(class);
                    let mut class_findings = Vec::new();
                    for method in &class.methods {
                        if !is_compiled_suspend_shape(&method.descriptor) {
                            continue;
                        }
                        for call in &method.calls {
                            if !is_run_blocking_call(call) {
                                continue;
                            }
                            class_findings.push(RuleFinding {
                                class_name: class.name.clone(),
                                method_name: method.name.clone(),
                                method_descriptor: method.descriptor.clone(),
                                artifact_uri: artifact_uri.clone(),
                                line: method.line_for_offset(call.offset),
                                offset: call.offset,
                            });
                        }
                    }
                    class_findings
                });
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
                let message = result_message(format!(
                    "runBlocking in suspend function {}.{}{} blocks the calling thread and can deadlock. Call the suspending code directly, or use withContext(...) when a specific dispatcher or context is needed.",
                    finding.class_name, finding.method_name, finding.method_descriptor
                ));
                let location = method_location_with_line(
                    &finding.class_name,
                    &finding.method_name,
                    &finding.method_descriptor,
                    finding.artifact_uri.as_deref(),
                    finding.line,
                );
                SarifResult::builder()
                    .message(message)
                    .locations(vec![location])
                    .build()
            })
            .collect())
    }
}

/// Intermediate finding used to sort results deterministically before SARIF conversion.
#[derive(Clone, Debug, Eq, PartialEq)]
struct RuleFinding {
    class_name: String,
    method_name: String,
    method_descriptor: String,
    artifact_uri: Option<String>,
    line: Option<u32>,
    offset: u32,
}

fn is_compiled_suspend_shape(descriptor: &str) -> bool {
    let Ok(parsed) = MethodDescriptor::from_str(descriptor) else {
        return false;
    };
    match parsed.return_type() {
        TypeDescriptor::Object(name) if name == "java/lang/Object" => {}
        _ => return false,
    }
    matches!(
        parsed.parameter_types().last(),
        Some(TypeDescriptor::Object(name)) if name == "kotlin/coroutines/Continuation"
    )
}

fn is_run_blocking_call(call: &CallSite) -> bool {
    call.kind == CallKind::Static
        && call.owner == RUN_BLOCKING_OWNER
        && ((call.name == "runBlocking" && call.descriptor == RUN_BLOCKING_DESCRIPTOR)
            || (call.name == "runBlocking$default"
                && call.descriptor == RUN_BLOCKING_DEFAULT_DESCRIPTOR))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::engine::{EngineOutput, build_context};
    use crate::ir::Class;
    use crate::test_harness::{JvmTestHarness, Language, SourceFile};

    fn rule_messages(output: &EngineOutput) -> Vec<String> {
        output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("RUN_BLOCKING_IN_SUSPEND_FUNCTION"))
            .filter_map(|result| result.message.text.clone())
            .collect()
    }

    fn coroutines_stub_source() -> SourceFile {
        SourceFile {
            path: "kotlinx/coroutines/Builders.kt".to_string(),
            contents: r#"
@file:JvmName("BuildersKt")

package kotlinx.coroutines

import kotlin.coroutines.CoroutineContext
import kotlin.coroutines.EmptyCoroutineContext

interface CoroutineScope

fun <T> runBlocking(context: CoroutineContext = EmptyCoroutineContext, block: suspend CoroutineScope.() -> T): T {
    throw UnsupportedOperationException("stub")
}

suspend fun <T> withContext(context: CoroutineContext, block: suspend CoroutineScope.() -> T): T {
    throw UnsupportedOperationException("stub")
}

fun CoroutineScope.launch(block: suspend CoroutineScope.() -> Unit) {
}
"#
            .to_string(),
        }
    }

    fn compile_and_analyze(
        harness: &JvmTestHarness,
        sources: Vec<SourceFile>,
        classpath: &[PathBuf],
    ) -> EngineOutput {
        harness
            .compile_and_analyze(Language::Kotlin, &sources, classpath)
            .expect("run harness analysis")
    }

    #[test]
    fn is_compiled_suspend_shape_requires_continuation_and_object_return() {
        assert!(is_compiled_suspend_shape(
            "(Lkotlin/coroutines/Continuation;)Ljava/lang/Object;"
        ));
        assert!(is_compiled_suspend_shape(
            "(Lkotlin/coroutines/CoroutineContext;Lkotlin/coroutines/Continuation;)Ljava/lang/Object;"
        ));
        // Continuation parameter with a non-Object return is not a suspend shape.
        assert!(!is_compiled_suspend_shape(
            "(Lkotlin/coroutines/Continuation;)V"
        ));
        assert!(!is_compiled_suspend_shape(
            "(Lkotlin/coroutines/Continuation;)Ljava/lang/String;"
        ));
        // A trailing Continuation array is not a suspend shape.
        assert!(!is_compiled_suspend_shape(
            "([Lkotlin/coroutines/Continuation;)Ljava/lang/Object;"
        ));
        // No parameters means no trailing Continuation.
        assert!(!is_compiled_suspend_shape("()Ljava/lang/Object;"));
        assert!(!is_compiled_suspend_shape("not a descriptor"));
    }

    #[test]
    fn is_run_blocking_call_requires_exact_owner_name_and_descriptor() {
        let call = |owner: &str, name: &str, descriptor: &str, kind: CallKind| CallSite {
            owner: owner.to_string(),
            name: name.to_string(),
            descriptor: descriptor.to_string(),
            kind,
            offset: 0,
        };

        assert!(is_run_blocking_call(&call(
            RUN_BLOCKING_OWNER,
            "runBlocking",
            RUN_BLOCKING_DESCRIPTOR,
            CallKind::Static,
        )));
        assert!(is_run_blocking_call(&call(
            RUN_BLOCKING_OWNER,
            "runBlocking$default",
            RUN_BLOCKING_DEFAULT_DESCRIPTOR,
            CallKind::Static,
        )));
        assert!(!is_run_blocking_call(&call(
            "com/example/ClassBKt",
            "runBlocking",
            RUN_BLOCKING_DESCRIPTOR,
            CallKind::Static,
        )));
        assert!(!is_run_blocking_call(&call(
            RUN_BLOCKING_OWNER,
            "withContext",
            RUN_BLOCKING_DESCRIPTOR,
            CallKind::Static,
        )));
        assert!(!is_run_blocking_call(&call(
            RUN_BLOCKING_OWNER,
            "runBlocking",
            RUN_BLOCKING_DEFAULT_DESCRIPTOR,
            CallKind::Static,
        )));
        assert!(!is_run_blocking_call(&call(
            RUN_BLOCKING_OWNER,
            "runBlocking",
            RUN_BLOCKING_DESCRIPTOR,
            CallKind::Virtual,
        )));
    }

    #[test]
    fn run_blocking_in_suspend_function_skips_when_kotlinx_coroutines_absent() {
        let classes = vec![Class {
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
        }];
        let context = build_context(classes, &[]);

        assert!(!context.has_kotlinx_coroutines());
        let results = RunBlockingInSuspendFunctionRule
            .run(&context)
            .expect("run rule without kotlinx.coroutines");
        assert!(results.is_empty());
    }

    #[test]
    fn run_blocking_in_suspend_function_reports_default_bridge_call() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![
            coroutines_stub_source(),
            SourceFile {
                path: "com/example/ClassA.kt".to_string(),
                contents: r#"
package com.example

import kotlinx.coroutines.runBlocking

fun loadValue(): String = "value"

@Suppress("RunBlockingInSuspendFunction")
suspend fun methodA(): String = runBlocking {
    loadValue()
}
"#
                .to_string(),
            },
        ];

        let output = compile_and_analyze(&harness, sources, &[]);
        let messages = rule_messages(&output);
        assert_eq!(messages.len(), 1, "expected one finding, got {messages:?}");
        assert_eq!(
            messages[0],
            "runBlocking in suspend function com/example/ClassAKt.methodA(Lkotlin/coroutines/Continuation;)Ljava/lang/Object; blocks the calling thread and can deadlock. Call the suspending code directly, or use withContext(...) when a specific dispatcher or context is needed."
        );
        let has_line = output
            .results
            .iter()
            .filter(|result| {
                result.rule_id.as_deref() == Some("RUN_BLOCKING_IN_SUSPEND_FUNCTION")
            })
            .any(|result| {
                result.locations.iter().flatten().any(|location| {
                    location
                        .physical_location
                        .as_ref()
                        .and_then(|physical| physical.region.as_ref())
                        .and_then(|region| region.start_line)
                        .is_some()
                })
            });
        assert!(has_line, "expected a source line on the finding location");
    }

    #[test]
    fn run_blocking_in_suspend_function_reports_explicit_context_call() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![
            coroutines_stub_source(),
            SourceFile {
                path: "com/example/ClassB.kt".to_string(),
                contents: r#"
package com.example

import kotlin.coroutines.CoroutineContext
import kotlinx.coroutines.runBlocking

fun loadValue(): String = "value"

suspend fun methodB(context: CoroutineContext): String = runBlocking(context) {
    loadValue()
}
"#
                .to_string(),
            },
        ];

        let output = compile_and_analyze(&harness, sources, &[]);
        let messages = rule_messages(&output);
        assert_eq!(messages.len(), 1, "expected one finding, got {messages:?}");
        assert!(messages[0].contains("methodB"));
        assert!(messages[0].contains("runBlocking in suspend function"));
    }

    #[test]
    fn run_blocking_in_suspend_function_ignores_non_suspend_function() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![
            coroutines_stub_source(),
            SourceFile {
                path: "com/example/ClassC.kt".to_string(),
                contents: r#"
package com.example

import kotlinx.coroutines.runBlocking

fun loadValue(): String = "value"

fun methodC() {
    runBlocking {
        loadValue()
    }
}
"#
                .to_string(),
            },
        ];

        let output = compile_and_analyze(&harness, sources, &[]);
        let messages = rule_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect findings for non-suspend runBlocking: {messages:?}"
        );
    }

    #[test]
    fn run_blocking_in_suspend_function_ignores_with_context_only() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![
            coroutines_stub_source(),
            SourceFile {
                path: "com/example/ClassD.kt".to_string(),
                contents: r#"
package com.example

import kotlin.coroutines.CoroutineContext
import kotlinx.coroutines.withContext

fun loadValue(): String = "value"

suspend fun methodD(context: CoroutineContext): String = withContext(context) {
    loadValue()
}
"#
                .to_string(),
            },
        ];

        let output = compile_and_analyze(&harness, sources, &[]);
        let messages = rule_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect findings for withContext: {messages:?}"
        );
    }

    #[test]
    fn run_blocking_in_suspend_function_ignores_suspend_lambda_body() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![
            coroutines_stub_source(),
            SourceFile {
                path: "com/example/ClassF.kt".to_string(),
                contents: r#"
package com.example

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking

fun loadValue(): String = "value"

fun methodF(scope: CoroutineScope) {
    scope.launch {
        runBlocking { loadValue() }
    }
}
"#
                .to_string(),
            },
        ];

        let output = compile_and_analyze(&harness, sources, &[]);
        let messages = rule_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect findings inside suspend lambda bodies: {messages:?}"
        );
    }

    #[test]
    fn run_blocking_in_suspend_function_reports_two_calls_in_order() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![
            coroutines_stub_source(),
            SourceFile {
                path: "com/example/ClassE.kt".to_string(),
                contents: r#"
package com.example

import kotlinx.coroutines.runBlocking

fun loadValue(): String = "value"

suspend fun methodE() {
    runBlocking { loadValue() }
    runBlocking { loadValue() }
}
"#
                .to_string(),
            },
        ];

        let output = compile_and_analyze(&harness, sources, &[]);
        let messages = rule_messages(&output);
        assert_eq!(messages.len(), 2, "expected two findings, got {messages:?}");
        assert!(messages.iter().all(|message| message.contains("methodE")));
    }

    #[test]
    fn run_blocking_in_suspend_function_ignores_same_shape_call_on_other_owner() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![
            coroutines_stub_source(),
            SourceFile {
                path: "com/example/ClassG.kt".to_string(),
                contents: r#"
@file:JvmName("ClassGKt")

package com.example

import kotlin.coroutines.CoroutineContext
import kotlin.coroutines.EmptyCoroutineContext
import kotlinx.coroutines.CoroutineScope

fun loadValue(): String = "value"

fun <T> runBlocking(context: CoroutineContext = EmptyCoroutineContext, block: suspend CoroutineScope.() -> T): T {
    throw UnsupportedOperationException("local")
}

suspend fun methodG(): String = runBlocking {
    loadValue()
}
"#
                .to_string(),
            },
        ];

        let output = compile_and_analyze(&harness, sources, &[]);
        let messages = rule_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect findings for a runBlocking-named function on another owner: {messages:?}"
        );
    }

    #[test]
    fn run_blocking_in_suspend_function_ignores_classpath_only_code() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let dependency_sources = vec![
            coroutines_stub_source(),
            SourceFile {
                path: "com/example/ClassDep.kt".to_string(),
                contents: r#"
package com.example

import kotlinx.coroutines.runBlocking

fun loadValue(): String = "value"

suspend fun methodDep(): String = runBlocking {
    loadValue()
}
"#
                .to_string(),
            },
        ];
        let dependency_output = harness
            .compile(Language::Kotlin, &dependency_sources, &[])
            .expect("compile dependency classes");

        let app_sources = vec![SourceFile {
            path: "com/example/ClassApp.kt".to_string(),
            contents: r#"
package com.example

class ClassApp
"#
            .to_string(),
        }];

        let output = compile_and_analyze(
            &harness,
            app_sources,
            &[dependency_output.classes_dir().to_path_buf()],
        );
        let messages = rule_messages(&output);
        assert!(
            messages.is_empty(),
            "classpath classes must be out of scope for RUN_BLOCKING_IN_SUSPEND_FUNCTION: {messages:?}"
        );
    }

    #[test]
    fn run_blocking_in_suspend_function_is_deterministic_across_runs() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![
            coroutines_stub_source(),
            SourceFile {
                path: "com/example/ClassH.kt".to_string(),
                contents: r#"
package com.example

import kotlin.coroutines.CoroutineContext
import kotlinx.coroutines.runBlocking

fun loadValue(): String = "value"

suspend fun methodA(): String = runBlocking {
    loadValue()
}

suspend fun methodB(context: CoroutineContext): String = runBlocking(context) {
    loadValue()
}

suspend fun methodE() {
    runBlocking { loadValue() }
    runBlocking { loadValue() }
}
"#
                .to_string(),
            },
        ];
        let compiled = harness
            .compile(Language::Kotlin, &sources, &[])
            .expect("compile analysis target classes");

        let first = harness
            .analyze(compiled.classes_dir(), &[])
            .expect("run first analysis");
        let second = harness
            .analyze(compiled.classes_dir(), &[])
            .expect("run second analysis");

        let first_messages = rule_messages(&first);
        let second_messages = rule_messages(&second);
        assert_eq!(
            first_messages.len(),
            4,
            "expected four findings, got {first_messages:?}"
        );
        assert_eq!(first_messages, second_messages);
    }
}
