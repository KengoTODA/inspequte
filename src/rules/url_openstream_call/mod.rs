use anyhow::Result;
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::engine::AnalysisContext;
use crate::ir::Method;
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

/// Rule that detects direct URL.openStream calls.
#[derive(Default)]
pub(crate) struct UrlOpenstreamCallRule;

crate::register_rule!(UrlOpenstreamCallRule);

impl Rule for UrlOpenstreamCallRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "URL_OPENSTREAM_CALL",
            name: "URL.openStream call",
            description: "URL.openStream can hide timeout and connection configuration",
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
                        for (call_index, call) in method.calls.iter().enumerate() {
                            if is_url_openstream_call(&call.owner, &call.name, &call.descriptor) {
                                if is_classpath_resource_openstream(method, call_index) {
                                    continue;
                                }
                                let message = result_message(format!(
                                    "Avoid URL.openStream() in {}.{}{}; use openConnection() with explicit timeouts and structured resource handling.",
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

fn is_url_openstream_call(owner: &str, name: &str, descriptor: &str) -> bool {
    owner == "java/net/URL" && name == "openStream" && descriptor == "()Ljava/io/InputStream;"
}

fn is_classpath_resource_openstream(method: &Method, openstream_index: usize) -> bool {
    if openstream_index == 0 {
        return false;
    }
    let previous = &method.calls[openstream_index - 1];
    is_resource_lookup_call(&previous.owner, &previous.name, &previous.descriptor)
}

fn is_resource_lookup_call(owner: &str, name: &str, descriptor: &str) -> bool {
    matches!(
        (owner, name, descriptor),
        (
            "java/lang/Class",
            "getResource",
            "(Ljava/lang/String;)Ljava/net/URL;"
        ) | (
            "java/lang/ClassLoader",
            "getResource",
            "(Ljava/lang/String;)Ljava/net/URL;"
        )
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::test_harness::{JvmTestHarness, Language, SourceFile};

    fn openstream_messages(output: &crate::engine::EngineOutput) -> Vec<String> {
        output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("URL_OPENSTREAM_CALL"))
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
    fn url_openstream_call_reports_usage() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
import java.io.InputStream;
import java.net.URL;
public class ClassA {
    public InputStream methodX(URL varOne) throws Exception {
        return varOne.openStream();
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = openstream_messages(&output);
        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("Avoid URL.openStream()")),
            "expected URL_OPENSTREAM_CALL finding, got {messages:?}"
        );
    }

    #[test]
    fn url_openstream_call_ignores_open_connection_usage() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassB.java".to_string(),
            contents: r#"
package com.example;
import java.net.URL;
import java.net.URLConnection;
public class ClassB {
    public URLConnection methodY(URL varOne) throws Exception {
        return varOne.openConnection();
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = openstream_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect URL_OPENSTREAM_CALL finding for openConnection(): {messages:?}"
        );
    }

    #[test]
    fn url_openstream_call_ignores_class_get_resource_chain() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassC.java".to_string(),
            contents: r#"
package com.example;
import java.io.InputStream;
public class ClassC {
    public InputStream methodZ() throws Exception {
        return ClassC.class.getResource("/tmp.txt").openStream();
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = openstream_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect URL_OPENSTREAM_CALL for Class.getResource(...).openStream(): {messages:?}"
        );
    }

    #[test]
    fn url_openstream_call_ignores_classloader_get_resource_chain() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassD.java".to_string(),
            contents: r#"
package com.example;
import java.io.InputStream;
public class ClassD {
    public InputStream methodW() throws Exception {
        ClassLoader varOne = ClassD.class.getClassLoader();
        return varOne.getResource("tmp.txt").openStream();
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = openstream_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect URL_OPENSTREAM_CALL for ClassLoader.getResource(...).openStream(): {messages:?}"
        );
    }

    #[test]
    fn url_openstream_call_ignores_classpath_calls() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");

        let dependency_sources = vec![SourceFile {
            path: "com/example/ClassB.java".to_string(),
            contents: r#"
package com.example;
import java.io.InputStream;
import java.net.URL;
public class ClassB {
    public InputStream methodY(URL varOne) throws Exception {
        return varOne.openStream();
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
        let messages = openstream_messages(&analysis);
        assert!(
            messages.is_empty(),
            "classpath classes must be out of scope for URL_OPENSTREAM_CALL: {messages:?}"
        );
    }
}
