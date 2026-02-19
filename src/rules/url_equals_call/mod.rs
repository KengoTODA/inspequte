use anyhow::Result;
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::engine::AnalysisContext;
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

/// Rule that detects direct `URL.equals(Object)` calls.
#[derive(Default)]
pub(crate) struct UrlEqualsCallRule;

crate::register_rule!(UrlEqualsCallRule);

impl Rule for UrlEqualsCallRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "URL_EQUALS_CALL",
            name: "URL equals call",
            description: "URL.equals may trigger host resolution and surprising equality semantics",
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
                            if is_url_equals_call(&call.owner, &call.name, &call.descriptor) {
                                let message = result_message(format!(
                                    "Avoid URL.equals() in {}.{}{}; compare normalized URI values or explicit URL components instead.",
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

fn is_url_equals_call(owner: &str, name: &str, descriptor: &str) -> bool {
    owner == "java/net/URL" && name == "equals" && descriptor == "(Ljava/lang/Object;)Z"
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::test_harness::{JvmTestHarness, Language, SourceFile};

    fn url_equals_messages(output: &crate::engine::EngineOutput) -> Vec<String> {
        output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("URL_EQUALS_CALL"))
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
    fn url_equals_call_reports_equals_usage() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
import java.net.URL;
public class ClassA {
    public boolean methodX(URL varOne, URL varTwo) {
        return varOne.equals(varTwo);
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = url_equals_messages(&output);
        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("Avoid URL.equals()")),
            "expected URL_EQUALS_CALL finding, got {messages:?}"
        );
    }

    #[test]
    fn url_equals_call_ignores_uri_equals_usage() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassB.java".to_string(),
            contents: r#"
package com.example;
import java.net.URL;
import java.net.URISyntaxException;
public class ClassB {
    public boolean methodY(URL varOne, URL varTwo) throws URISyntaxException {
        return varOne.toURI().equals(varTwo.toURI());
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = url_equals_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect URL_EQUALS_CALL finding for URI.equals(): {messages:?}"
        );
    }

    #[test]
    fn url_equals_call_ignores_classpath_calls() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");

        let dependency_sources = vec![SourceFile {
            path: "com/example/ClassB.java".to_string(),
            contents: r#"
package com.example;
import java.net.URL;
public class ClassB {
    public boolean methodY(URL varOne, URL varTwo) {
        return varOne.equals(varTwo);
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
        let messages = url_equals_messages(&analysis);
        assert!(
            messages.is_empty(),
            "classpath classes must be out of scope for URL_EQUALS_CALL: {messages:?}"
        );
    }
}
