use anyhow::Result;
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::engine::AnalysisContext;
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

/// Rule that detects timeout-free blocking Future.get calls.
#[derive(Default)]
pub(crate) struct FutureGetWithoutTimeoutRule;

crate::register_rule!(FutureGetWithoutTimeoutRule);

impl Rule for FutureGetWithoutTimeoutRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "FUTURE_GET_WITHOUT_TIMEOUT",
            name: "Future.get without timeout",
            description: "Timeout-free Future.get calls can block indefinitely",
        }
    }

    fn run(&self, context: &AnalysisContext) -> Result<Vec<SarifResult>> {
        let mut results = Vec::new();
        for class in context.analysis_target_classes() {
            let mut attributes = vec![KeyValue::new("inspequte.class", class.name.clone())];
            if let Some(uri) = context.class_artifact_uri(class) {
                attributes.push(KeyValue::new("inspequte.artifact_uri", uri));
            }
            let class_results =
                context.with_span("scan.class", &attributes, || -> Result<Vec<SarifResult>> {
                    let mut class_results = Vec::new();
                    let artifact_uri = context.class_artifact_uri(class);
                    for method in &class.methods {
                        for call in &method.calls {
                            if is_timeout_free_future_get(&call.owner, &call.name, &call.descriptor)
                            {
                                let message = result_message(format!(
                                    "Avoid timeout-free Future.get() in {}.{}{}; prefer get(timeout, unit) or non-blocking composition.",
                                    class.name, method.name, method.descriptor
                                ));
                                let line = method.line_for_offset(call.offset);
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

fn is_timeout_free_future_get(owner: &str, name: &str, descriptor: &str) -> bool {
    if name != "get" || !descriptor.starts_with("()") {
        return false;
    }

    matches!(
        owner,
        "java/util/concurrent/Future"
            | "java/util/concurrent/CompletableFuture"
            | "java/util/concurrent/FutureTask"
            | "java/util/concurrent/ForkJoinTask"
    ) || (owner.starts_with("java/util/concurrent/") && owner.ends_with("Future"))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::test_harness::{JvmTestHarness, Language, SourceFile};

    fn future_get_messages(output: &crate::engine::EngineOutput) -> Vec<String> {
        output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("FUTURE_GET_WITHOUT_TIMEOUT"))
            .filter_map(|result| result.message.text.clone())
            .collect()
    }

    fn compile_and_analyze(
        harness: &JvmTestHarness,
        language: Language,
        sources: &[SourceFile],
        classpath: &[PathBuf],
    ) -> crate::engine::EngineOutput {
        harness
            .compile_and_analyze(language, sources, classpath)
            .expect("run harness analysis")
    }

    #[test]
    fn future_get_without_timeout_reports_future_get_call() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
import java.util.concurrent.Future;
public class ClassA {
    public Object methodX(Future<Object> varOne) throws Exception {
        return varOne.get();
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, Language::Java, &sources, &[]);
        let messages = future_get_messages(&output);
        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("Avoid timeout-free Future.get()")),
            "expected FUTURE_GET_WITHOUT_TIMEOUT finding for Future.get(), got {messages:?}"
        );
    }

    #[test]
    fn future_get_without_timeout_reports_completable_future_get_call() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassB.java".to_string(),
            contents: r#"
package com.example;
import java.util.concurrent.CompletableFuture;
public class ClassB {
    public Object methodY(CompletableFuture<Object> varOne) throws Exception {
        return varOne.get();
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, Language::Java, &sources, &[]);
        let messages = future_get_messages(&output);
        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("Avoid timeout-free Future.get()")),
            "expected FUTURE_GET_WITHOUT_TIMEOUT finding for CompletableFuture.get(), got {messages:?}"
        );
    }

    #[test]
    fn future_get_without_timeout_ignores_timed_get_overload() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassC.java".to_string(),
            contents: r#"
package com.example;
import java.util.concurrent.Future;
import java.util.concurrent.TimeUnit;
public class ClassC {
    public Object methodZ(Future<Object> varOne) throws Exception {
        return varOne.get(1L, TimeUnit.SECONDS);
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, Language::Java, &sources, &[]);
        let messages = future_get_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect FUTURE_GET_WITHOUT_TIMEOUT finding for timed get(): {messages:?}"
        );
    }

    #[test]
    fn future_get_without_timeout_ignores_get_now_api() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassD.java".to_string(),
            contents: r#"
package com.example;
import java.util.concurrent.CompletableFuture;
public class ClassD {
    public Object methodW(CompletableFuture<Object> varOne, Object varTwo) {
        return varOne.getNow(varTwo);
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, Language::Java, &sources, &[]);
        let messages = future_get_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect FUTURE_GET_WITHOUT_TIMEOUT finding for getNow(): {messages:?}"
        );
    }

    #[test]
    fn future_get_without_timeout_ignores_classpath_calls() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");

        let dependency_sources = vec![SourceFile {
            path: "com/example/ClassB.java".to_string(),
            contents: r#"
package com.example;
import java.util.concurrent.Future;
public class ClassB {
    public Object methodY(Future<Object> varOne) throws Exception {
        return varOne.get();
    }
}
"#
            .to_string(),
        }];
        let dependency_output = harness
            .compile(Language::Java, &dependency_sources, &[])
            .expect("compile dependency classes");

        let app_sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
public class ClassA {
    public void methodX() {}
}
"#
            .to_string(),
        }];
        let app_output = harness
            .compile(
                Language::Java,
                &app_sources,
                &[dependency_output.classes_dir().to_path_buf()],
            )
            .expect("compile app classes");

        let analysis = harness
            .analyze(
                app_output.classes_dir(),
                &[dependency_output.classes_dir().to_path_buf()],
            )
            .expect("run harness analysis");
        let messages = future_get_messages(&analysis);
        assert!(
            messages.is_empty(),
            "classpath classes must be out of scope for FUTURE_GET_WITHOUT_TIMEOUT: {messages:?}"
        );
    }
}
