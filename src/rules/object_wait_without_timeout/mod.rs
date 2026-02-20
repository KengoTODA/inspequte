use anyhow::Result;
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::engine::AnalysisContext;
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

/// Rule that detects timeout-free Object.wait calls.
#[derive(Default)]
pub(crate) struct ObjectWaitWithoutTimeoutRule;

crate::register_rule!(ObjectWaitWithoutTimeoutRule);

impl Rule for ObjectWaitWithoutTimeoutRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "OBJECT_WAIT_WITHOUT_TIMEOUT",
            name: "Object.wait without timeout",
            description: "Timeout-free Object.wait calls can block indefinitely",
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
                            if is_timeout_free_wait(&call.owner, &call.name, &call.descriptor) {
                                let message = result_message(format!(
                                    "Avoid timeout-free Object.wait() in {}.{}{}; use a timed wait and explicit condition checks.",
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

fn is_timeout_free_wait(owner: &str, name: &str, descriptor: &str) -> bool {
    owner == "java/lang/Object" && name == "wait" && descriptor == "()V"
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::test_harness::{JvmTestHarness, Language, SourceFile};

    fn wait_messages(output: &crate::engine::EngineOutput) -> Vec<String> {
        output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("OBJECT_WAIT_WITHOUT_TIMEOUT"))
            .filter_map(|result| result.message.text.clone())
            .collect()
    }

    fn compile_and_analyze(
        harness: &JvmTestHarness,
        sources: &[SourceFile],
        classpath: &[PathBuf],
    ) -> crate::engine::EngineOutput {
        harness
            .compile_and_analyze(Language::Java, sources, classpath)
            .expect("run harness analysis")
    }

    #[test]
    fn object_wait_without_timeout_reports_wait_call() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
public class ClassA {
    public void methodX(Object varOne) throws Exception {
        synchronized (varOne) {
            varOne.wait();
        }
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = wait_messages(&output);
        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("Avoid timeout-free Object.wait()")),
            "expected OBJECT_WAIT_WITHOUT_TIMEOUT finding for wait(), got {messages:?}"
        );
    }

    #[test]
    fn object_wait_without_timeout_ignores_wait_with_long_timeout() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassB.java".to_string(),
            contents: r#"
package com.example;
public class ClassB {
    public void methodY(Object varOne) throws Exception {
        synchronized (varOne) {
            varOne.wait(1000L);
        }
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = wait_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect OBJECT_WAIT_WITHOUT_TIMEOUT finding for wait(long): {messages:?}"
        );
    }

    #[test]
    fn object_wait_without_timeout_ignores_wait_with_nanos_timeout() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassC.java".to_string(),
            contents: r#"
package com.example;
public class ClassC {
    public void methodZ(Object varOne) throws Exception {
        synchronized (varOne) {
            varOne.wait(1000L, 1);
        }
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = wait_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect OBJECT_WAIT_WITHOUT_TIMEOUT finding for wait(long,int): {messages:?}"
        );
    }

    #[test]
    fn object_wait_without_timeout_ignores_classpath_calls() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");

        let dependency_sources = vec![SourceFile {
            path: "com/example/ClassB.java".to_string(),
            contents: r#"
package com.example;
public class ClassB {
    public void methodY(Object varOne) throws Exception {
        synchronized (varOne) {
            varOne.wait();
        }
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
        let messages = wait_messages(&analysis);
        assert!(
            messages.is_empty(),
            "classpath classes must be out of scope for OBJECT_WAIT_WITHOUT_TIMEOUT: {messages:?}"
        );
    }
}
