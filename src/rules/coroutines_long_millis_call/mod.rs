use std::collections::BTreeMap;

use anyhow::Result;
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::engine::AnalysisContext;
use crate::ir::Class;
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

/// JVM `(owner, name, descriptor)` triple for a kotlinx.coroutines function that takes raw Long milliseconds.
struct TargetCall {
    owner: &'static str,
    function: &'static str,
    descriptor: &'static str,
}

const TARGET_CALLS: &[TargetCall] = &[
    TargetCall {
        owner: "kotlinx/coroutines/DelayKt",
        function: "delay",
        descriptor: "(JLkotlin/coroutines/Continuation;)Ljava/lang/Object;",
    },
    TargetCall {
        owner: "kotlinx/coroutines/TimeoutKt",
        function: "withTimeout",
        descriptor: "(JLkotlin/jvm/functions/Function2;Lkotlin/coroutines/Continuation;)Ljava/lang/Object;",
    },
    TargetCall {
        owner: "kotlinx/coroutines/TimeoutKt",
        function: "withTimeoutOrNull",
        descriptor: "(JLkotlin/jvm/functions/Function2;Lkotlin/coroutines/Continuation;)Ljava/lang/Object;",
    },
    TargetCall {
        owner: "kotlinx/coroutines/flow/FlowKt",
        function: "debounce",
        descriptor: "(Lkotlinx/coroutines/flow/Flow;J)Lkotlinx/coroutines/flow/Flow;",
    },
    TargetCall {
        owner: "kotlinx/coroutines/flow/FlowKt",
        function: "sample",
        descriptor: "(Lkotlinx/coroutines/flow/Flow;J)Lkotlinx/coroutines/flow/Flow;",
    },
];

/// Rule that detects kotlinx.coroutines time-based calls passing a raw Long milliseconds value when a kotlin.time.Duration overload is available on the classpath.
#[derive(Default)]
pub(crate) struct CoroutinesLongMillisCallRule;

crate::register_rule!(CoroutinesLongMillisCallRule);

impl Rule for CoroutinesLongMillisCallRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "COROUTINES_LONG_MILLIS_CALL",
            name: "Coroutines call with raw Long milliseconds",
            description: "kotlinx.coroutines time-based calls that pass a raw Long milliseconds value should use the kotlin.time.Duration overload when it is available on the classpath",
        }
    }

    fn run(&self, context: &AnalysisContext) -> Result<Vec<SarifResult>> {
        // Index only the classes that can own a target call. The first matching
        // class wins, so analysis target classes and earlier classpath artifacts
        // take priority over later duplicates, mirroring classpath resolution.
        let mut owner_classes: BTreeMap<&str, &Class> = BTreeMap::new();
        for class in context.all_classes() {
            let is_owner = TARGET_CALLS
                .iter()
                .any(|target| target.owner == class.name.as_str());
            if is_owner {
                owner_classes.entry(class.name.as_str()).or_insert(class);
            }
        }
        let available_targets: Vec<&TargetCall> = TARGET_CALLS
            .iter()
            .filter(|target| duration_counterpart_available(target, &owner_classes))
            .collect();
        if available_targets.is_empty() {
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
                        for call in &method.calls {
                            let Some(target) = available_targets.iter().find(|target| {
                                call.owner == target.owner
                                    && call.name == target.function
                                    && call.descriptor == target.descriptor
                            }) else {
                                continue;
                            };
                            class_findings.push(RuleFinding {
                                class_name: class.name.clone(),
                                method_name: method.name.clone(),
                                method_descriptor: method.descriptor.clone(),
                                artifact_uri: artifact_uri.clone(),
                                line: method.line_for_offset(call.offset),
                                offset: call.offset,
                                function_name: target.function,
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
                    "Call to {} in {}.{}{} passes a raw Long milliseconds value. Use the kotlin.time.Duration overload available on the classpath, for example {}(500.milliseconds).",
                    finding.function_name,
                    finding.class_name,
                    finding.method_name,
                    finding.method_descriptor,
                    finding.function_name,
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

/// Matched call site pending conversion into a SARIF result.
#[derive(Clone, Debug, Eq, PartialEq)]
struct RuleFinding {
    class_name: String,
    method_name: String,
    method_descriptor: String,
    artifact_uri: Option<String>,
    line: Option<u32>,
    offset: u32,
    function_name: &'static str,
}

fn duration_counterpart_available(
    target: &TargetCall,
    owner_classes: &BTreeMap<&str, &Class>,
) -> bool {
    let Some(owner) = owner_classes.get(target.owner) else {
        return false;
    };
    let mangled_prefix = format!("{}-", target.function);
    owner.methods.iter().any(|method| {
        method.name.starts_with(&mangled_prefix) && method.descriptor == target.descriptor
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::engine::EngineOutput;
    use crate::test_harness::{CompileOutput, JvmTestHarness, Language, SourceFile};

    fn rule_messages(output: &EngineOutput) -> Vec<String> {
        output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("COROUTINES_LONG_MILLIS_CALL"))
            .filter_map(|result| result.message.text.clone())
            .collect()
    }

    fn rule_start_lines(output: &EngineOutput) -> Vec<Option<i64>> {
        output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("COROUTINES_LONG_MILLIS_CALL"))
            .map(|result| {
                result
                    .locations
                    .as_ref()
                    .and_then(|locations| locations.first())
                    .and_then(|location| location.physical_location.as_ref())
                    .and_then(|physical| physical.region.as_ref())
                    .and_then(|region| region.start_line)
            })
            .collect()
    }

    fn compile_and_analyze(
        harness: &JvmTestHarness,
        language: Language,
        sources: &[SourceFile],
        classpath: &[PathBuf],
    ) -> EngineOutput {
        harness
            .compile_and_analyze(language, sources, classpath)
            .expect("run harness analysis")
    }

    fn coroutines_stub_sources() -> Vec<SourceFile> {
        vec![
            SourceFile {
                path: "kotlinx/coroutines/CoroutineScope.kt".to_string(),
                contents: r#"
package kotlinx.coroutines

interface CoroutineScope
"#
                .to_string(),
            },
            SourceFile {
                path: "kotlinx/coroutines/Delay.kt".to_string(),
                contents: r#"
@file:JvmName("DelayKt")

package kotlinx.coroutines

import kotlin.time.Duration

suspend fun delay(timeMillis: Long) {}

suspend fun delay(duration: Duration) {}
"#
                .to_string(),
            },
            SourceFile {
                path: "kotlinx/coroutines/Timeout.kt".to_string(),
                contents: r#"
@file:JvmName("TimeoutKt")

package kotlinx.coroutines

import kotlin.time.Duration

suspend fun <T> withTimeout(timeMillis: Long, block: suspend CoroutineScope.() -> T): T =
    TODO()

suspend fun <T> withTimeout(timeout: Duration, block: suspend CoroutineScope.() -> T): T =
    TODO()

suspend fun <T> withTimeoutOrNull(timeMillis: Long, block: suspend CoroutineScope.() -> T): T? =
    null

suspend fun <T> withTimeoutOrNull(timeout: Duration, block: suspend CoroutineScope.() -> T): T? =
    null
"#
                .to_string(),
            },
            SourceFile {
                path: "kotlinx/coroutines/flow/Flow.kt".to_string(),
                contents: r#"
package kotlinx.coroutines.flow

interface Flow<out T>
"#
                .to_string(),
            },
            SourceFile {
                path: "kotlinx/coroutines/flow/Operators.kt".to_string(),
                contents: r#"
@file:JvmName("FlowKt")

package kotlinx.coroutines.flow

import kotlin.time.Duration

fun <T> Flow<T>.debounce(timeoutMillis: Long): Flow<T> = this

fun <T> Flow<T>.debounce(timeout: Duration): Flow<T> = this

fun <T> Flow<T>.sample(periodMillis: Long): Flow<T> = this

fun <T> Flow<T>.sample(period: Duration): Flow<T> = this
"#
                .to_string(),
            },
        ]
    }

    fn coroutines_stub_sources_without_duration_overloads() -> Vec<SourceFile> {
        vec![
            SourceFile {
                path: "kotlinx/coroutines/CoroutineScope.kt".to_string(),
                contents: r#"
package kotlinx.coroutines

interface CoroutineScope
"#
                .to_string(),
            },
            SourceFile {
                path: "kotlinx/coroutines/Delay.kt".to_string(),
                contents: r#"
@file:JvmName("DelayKt")

package kotlinx.coroutines

suspend fun delay(timeMillis: Long) {}
"#
                .to_string(),
            },
            SourceFile {
                path: "kotlinx/coroutines/Timeout.kt".to_string(),
                contents: r#"
@file:JvmName("TimeoutKt")

package kotlinx.coroutines

suspend fun <T> withTimeout(timeMillis: Long, block: suspend CoroutineScope.() -> T): T =
    TODO()

suspend fun <T> withTimeoutOrNull(timeMillis: Long, block: suspend CoroutineScope.() -> T): T? =
    null
"#
                .to_string(),
            },
            SourceFile {
                path: "kotlinx/coroutines/flow/Flow.kt".to_string(),
                contents: r#"
package kotlinx.coroutines.flow

interface Flow<out T>
"#
                .to_string(),
            },
            SourceFile {
                path: "kotlinx/coroutines/flow/Operators.kt".to_string(),
                contents: r#"
@file:JvmName("FlowKt")

package kotlinx.coroutines.flow

fun <T> Flow<T>.debounce(timeoutMillis: Long): Flow<T> = this

fun <T> Flow<T>.sample(periodMillis: Long): Flow<T> = this
"#
                .to_string(),
            },
        ]
    }

    fn compile_stubs(harness: &JvmTestHarness, sources: Vec<SourceFile>) -> CompileOutput {
        harness
            .compile(Language::Kotlin, &sources, &[])
            .expect("compile kotlinx.coroutines stub classes")
    }

    #[test]
    fn coroutines_long_millis_call_reports_delay_with_long() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let mut sources = coroutines_stub_sources();
        sources.push(SourceFile {
            path: "com/example/FileOne.kt".to_string(),
            contents: r#"
package com.example

import kotlinx.coroutines.delay

suspend fun functionOne() {
    delay(500)
}
"#
            .to_string(),
        });

        let output = compile_and_analyze(&harness, Language::Kotlin, &sources, &[]);
        let messages = rule_messages(&output);
        assert_eq!(messages.len(), 1, "expected one finding, got {messages:?}");
        assert!(
            messages[0].contains("Call to delay in com/example/FileOneKt.functionOne"),
            "message must name delay and the enclosing method: {messages:?}"
        );
        assert!(
            messages[0].contains("delay(500.milliseconds)"),
            "message must suggest the Duration overload: {messages:?}"
        );
    }

    #[test]
    fn coroutines_long_millis_call_reports_timeout_functions_with_long() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let mut sources = coroutines_stub_sources();
        sources.push(SourceFile {
            path: "com/example/FileTwo.kt".to_string(),
            contents: r#"
package com.example

import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.withTimeoutOrNull

suspend fun functionOne(): String {
    val varOne = withTimeout(1_000) { "a" }
    val varTwo = withTimeoutOrNull(1_000) { "b" } ?: ""
    return varOne + varTwo
}
"#
            .to_string(),
        });

        let output = compile_and_analyze(&harness, Language::Kotlin, &sources, &[]);
        let messages = rule_messages(&output);
        assert_eq!(messages.len(), 2, "expected two findings, got {messages:?}");
        assert_eq!(
            messages
                .iter()
                .filter(|message| message.contains("Call to withTimeout in"))
                .count(),
            1,
            "expected one withTimeout finding: {messages:?}"
        );
        assert_eq!(
            messages
                .iter()
                .filter(|message| message.contains("Call to withTimeoutOrNull in"))
                .count(),
            1,
            "expected one withTimeoutOrNull finding: {messages:?}"
        );
    }

    #[test]
    fn coroutines_long_millis_call_reports_flow_operators_with_long() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let mut sources = coroutines_stub_sources();
        sources.push(SourceFile {
            path: "com/example/FileThree.kt".to_string(),
            contents: r#"
package com.example

import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.sample

fun functionOne(varOne: Flow<Int>): Flow<Int> = varOne.debounce(200)

fun functionTwo(varOne: Flow<Int>): Flow<Int> = varOne.sample(200)
"#
            .to_string(),
        });

        let output = compile_and_analyze(&harness, Language::Kotlin, &sources, &[]);
        let messages = rule_messages(&output);
        assert_eq!(messages.len(), 2, "expected two findings, got {messages:?}");
        assert!(
            messages
                .iter()
                .any(|message| message.contains("Call to debounce in")),
            "expected a debounce finding: {messages:?}"
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("Call to sample in")),
            "expected a sample finding: {messages:?}"
        );
    }

    #[test]
    fn coroutines_long_millis_call_ignores_duration_overload_call() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let mut sources = coroutines_stub_sources();
        sources.push(SourceFile {
            path: "com/example/FileFour.kt".to_string(),
            contents: r#"
package com.example

import kotlin.time.Duration.Companion.milliseconds
import kotlinx.coroutines.delay

suspend fun functionOne() {
    delay(500.milliseconds)
}
"#
            .to_string(),
        });

        let output = compile_and_analyze(&harness, Language::Kotlin, &sources, &[]);
        let messages = rule_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect findings for the Duration overload call: {messages:?}"
        );
    }

    #[test]
    fn coroutines_long_millis_call_ignores_calls_when_owner_is_unresolvable() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let stub_classes = compile_stubs(&harness, coroutines_stub_sources());
        let app_sources = vec![SourceFile {
            path: "com/example/FileFive.kt".to_string(),
            contents: r#"
package com.example

import kotlinx.coroutines.delay

suspend fun functionOne() {
    delay(500)
}
"#
            .to_string(),
        }];
        let app_output = harness
            .compile(
                Language::Kotlin,
                &app_sources,
                &[stub_classes.classes_dir().to_path_buf()],
            )
            .expect("compile app classes");

        let output = harness
            .analyze(app_output.classes_dir(), &[])
            .expect("run harness analysis");
        let messages = rule_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect findings when the owner class is unresolvable: {messages:?}"
        );
    }

    #[test]
    fn coroutines_long_millis_call_ignores_calls_when_library_has_no_duration_overloads() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let mut sources = coroutines_stub_sources_without_duration_overloads();
        sources.push(SourceFile {
            path: "com/example/FileSix.kt".to_string(),
            contents: r#"
package com.example

import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.sample
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.withTimeoutOrNull

suspend fun functionOne(): String {
    delay(500)
    val varOne = withTimeout(1_000) { "a" }
    val varTwo = withTimeoutOrNull(1_000) { "b" } ?: ""
    return varOne + varTwo
}

fun functionTwo(varOne: Flow<Int>): Flow<Int> = varOne.debounce(200).sample(200)
"#
            .to_string(),
        });

        let output = compile_and_analyze(&harness, Language::Kotlin, &sources, &[]);
        let messages = rule_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect findings without Duration counterparts: {messages:?}"
        );
    }

    #[test]
    fn coroutines_long_millis_call_ignores_mangled_method_with_different_descriptor() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![
            SourceFile {
                path: "kotlinx/coroutines/Delay.kt".to_string(),
                contents: r#"
@file:JvmName("DelayKt")

package kotlinx.coroutines

suspend fun delay(timeMillis: Long) {}

fun `delay-abc`(timeMillis: Long) {}
"#
                .to_string(),
            },
            SourceFile {
                path: "com/example/FileTen.kt".to_string(),
                contents: r#"
package com.example

import kotlinx.coroutines.delay

suspend fun functionOne() {
    delay(500)
}
"#
                .to_string(),
            },
        ];

        let output = compile_and_analyze(&harness, Language::Kotlin, &sources, &[]);
        let messages = rule_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect findings when the dash-named method has a different descriptor: {messages:?}"
        );
    }

    #[test]
    fn coroutines_long_millis_call_ignores_user_defined_function_with_matching_name() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let mut sources = coroutines_stub_sources();
        sources.push(SourceFile {
            path: "com/example/FileSeven.kt".to_string(),
            contents: r#"
package com.example

fun delay(timeMillis: Long) {}

fun functionOne() {
    delay(500)
}
"#
            .to_string(),
        });

        let output = compile_and_analyze(&harness, Language::Kotlin, &sources, &[]);
        let messages = rule_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect findings for a user-defined delay: {messages:?}"
        );
    }

    #[test]
    fn coroutines_long_millis_call_reports_each_call_site_in_one_method() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let mut sources = coroutines_stub_sources();
        sources.push(SourceFile {
            path: "com/example/FileEight.kt".to_string(),
            contents: r#"
package com.example

import kotlinx.coroutines.delay

suspend fun functionOne() {
    delay(100)
    delay(200)
}
"#
            .to_string(),
        });

        let output = compile_and_analyze(&harness, Language::Kotlin, &sources, &[]);
        let messages = rule_messages(&output);
        assert_eq!(messages.len(), 2, "expected two findings, got {messages:?}");
        assert!(
            messages
                .iter()
                .all(|message| message.contains("Call to delay in")),
            "both findings must name delay: {messages:?}"
        );
        assert_eq!(
            rule_start_lines(&output),
            vec![Some(7), Some(8)],
            "findings must carry the call-site lines in call-site order"
        );
    }

    #[test]
    fn coroutines_long_millis_call_ignores_classpath_only_call_sites() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let mut dependency_sources = coroutines_stub_sources();
        dependency_sources.push(SourceFile {
            path: "com/dependency/FileNine.kt".to_string(),
            contents: r#"
package com.dependency

import kotlinx.coroutines.delay

suspend fun functionOne() {
    delay(500)
}
"#
            .to_string(),
        });
        let dependency_classes = compile_stubs(&harness, dependency_sources);

        let app_sources = vec![SourceFile {
            path: "com/example/ClassA.kt".to_string(),
            contents: r#"
package com.example

class ClassA
"#
            .to_string(),
        }];

        let output = compile_and_analyze(
            &harness,
            Language::Kotlin,
            &app_sources,
            &[dependency_classes.classes_dir().to_path_buf()],
        );
        let messages = rule_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect findings from classpath-only call sites: {messages:?}"
        );
    }

    #[test]
    fn coroutines_long_millis_call_reports_java_call_site() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let stub_classes = compile_stubs(&harness, coroutines_stub_sources());
        let app_sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;

import kotlinx.coroutines.flow.Flow;
import kotlinx.coroutines.flow.FlowKt;

public class ClassA {
    public Flow<Integer> methodX(Flow<Integer> varOne) {
        return FlowKt.debounce(varOne, 200L);
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(
            &harness,
            Language::Java,
            &app_sources,
            &[stub_classes.classes_dir().to_path_buf()],
        );
        let messages = rule_messages(&output);
        assert_eq!(messages.len(), 1, "expected one finding, got {messages:?}");
        assert!(
            messages[0].contains("Call to debounce in com/example/ClassA.methodX"),
            "message must name debounce and the Java method: {messages:?}"
        );
    }

    #[test]
    fn coroutines_long_millis_call_ignores_selector_overload_on_the_same_owner() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        // Kept out of the shared stub so the Java call-site test does not need
        // kotlin.jvm.functions.Function1 on its javac classpath.
        let sources = vec![
            SourceFile {
                path: "kotlinx/coroutines/flow/Flow.kt".to_string(),
                contents: r#"
package kotlinx.coroutines.flow

interface Flow<out T>
"#
                .to_string(),
            },
            SourceFile {
                path: "kotlinx/coroutines/flow/Operators.kt".to_string(),
                contents: r#"
@file:JvmName("FlowKt")

package kotlinx.coroutines.flow

import kotlin.time.Duration

fun <T> Flow<T>.debounce(timeoutMillis: Long): Flow<T> = this

fun <T> Flow<T>.debounce(timeout: Duration): Flow<T> = this

fun <T> Flow<T>.debounce(timeoutMillis: (T) -> Long): Flow<T> = this
"#
                .to_string(),
            },
            SourceFile {
                path: "com/example/FileEleven.kt".to_string(),
                contents: r#"
package com.example

import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.debounce

fun functionOne(varOne: Flow<Int>): Flow<Int> = varOne.debounce { 200L }
"#
                .to_string(),
            },
        ];

        let output = compile_and_analyze(&harness, Language::Kotlin, &sources, &[]);
        let messages = rule_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect findings for the selector overload, which shares the owner and name but not the descriptor: {messages:?}"
        );
    }

    #[test]
    fn coroutines_long_millis_call_does_not_borrow_availability_from_a_longer_function_name() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![
            SourceFile {
                path: "kotlinx/coroutines/CoroutineScope.kt".to_string(),
                contents: r#"
package kotlinx.coroutines

interface CoroutineScope
"#
                .to_string(),
            },
            // Only withTimeoutOrNull has a Duration counterpart. Because both
            // functions share one descriptor, a prefix check without the dash
            // would treat withTimeout as available too.
            SourceFile {
                path: "kotlinx/coroutines/Timeout.kt".to_string(),
                contents: r#"
@file:JvmName("TimeoutKt")

package kotlinx.coroutines

import kotlin.time.Duration

suspend fun <T> withTimeout(timeMillis: Long, block: suspend CoroutineScope.() -> T): T =
    TODO()

suspend fun <T> withTimeoutOrNull(timeMillis: Long, block: suspend CoroutineScope.() -> T): T? =
    null

suspend fun <T> withTimeoutOrNull(timeout: Duration, block: suspend CoroutineScope.() -> T): T? =
    null
"#
                .to_string(),
            },
            SourceFile {
                path: "com/example/FileTwelve.kt".to_string(),
                contents: r#"
package com.example

import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.withTimeoutOrNull

suspend fun functionOne(): String {
    val varOne = withTimeout(1_000) { "a" }
    val varTwo = withTimeoutOrNull(1_000) { "b" } ?: ""
    return varOne + varTwo
}
"#
                .to_string(),
            },
        ];

        let output = compile_and_analyze(&harness, Language::Kotlin, &sources, &[]);
        let messages = rule_messages(&output);
        assert_eq!(messages.len(), 1, "expected one finding, got {messages:?}");
        assert!(
            messages[0].contains("Call to withTimeoutOrNull in"),
            "expected the only finding to name withTimeoutOrNull: {messages:?}"
        );
        assert!(
            !messages[0].contains("Call to withTimeout in"),
            "withTimeout has no Duration counterpart and must not be reported: {messages:?}"
        );
    }

    #[test]
    fn coroutines_long_millis_call_gates_availability_per_owner() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![
            // DelayKt ships a Duration counterpart, FlowKt does not.
            SourceFile {
                path: "kotlinx/coroutines/Delay.kt".to_string(),
                contents: r#"
@file:JvmName("DelayKt")

package kotlinx.coroutines

import kotlin.time.Duration

suspend fun delay(timeMillis: Long) {}

suspend fun delay(duration: Duration) {}
"#
                .to_string(),
            },
            SourceFile {
                path: "kotlinx/coroutines/flow/Flow.kt".to_string(),
                contents: r#"
package kotlinx.coroutines.flow

interface Flow<out T>
"#
                .to_string(),
            },
            SourceFile {
                path: "kotlinx/coroutines/flow/Operators.kt".to_string(),
                contents: r#"
@file:JvmName("FlowKt")

package kotlinx.coroutines.flow

fun <T> Flow<T>.debounce(timeoutMillis: Long): Flow<T> = this

fun <T> Flow<T>.sample(periodMillis: Long): Flow<T> = this
"#
                .to_string(),
            },
            SourceFile {
                path: "com/example/FileThirteen.kt".to_string(),
                contents: r#"
package com.example

import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.sample

suspend fun functionOne() {
    delay(500)
}

fun functionTwo(varOne: Flow<Int>): Flow<Int> = varOne.debounce(200).sample(200)
"#
                .to_string(),
            },
        ];

        let output = compile_and_analyze(&harness, Language::Kotlin, &sources, &[]);
        let messages = rule_messages(&output);
        assert_eq!(messages.len(), 1, "expected one finding, got {messages:?}");
        assert!(
            messages[0].contains("Call to delay in"),
            "expected the only finding to name delay: {messages:?}"
        );
    }
}
