use anyhow::Result;
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::engine::AnalysisContext;
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

/// Rule that detects direct Integer.getInteger calls.
#[derive(Default)]
pub(crate) struct IntegerGetintegerCallRule;

crate::register_rule!(IntegerGetintegerCallRule);

impl Rule for IntegerGetintegerCallRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "INTEGER_GETINTEGER_CALL",
            name: "Integer.getInteger call",
            description: "Integer.getInteger reads system properties, not numeric input strings",
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
                            if is_integer_getinteger_call(&call.owner, &call.name, &call.descriptor)
                            {
                                let message = result_message(format!(
                                    "Avoid Integer.getInteger() in {}.{}{}; use Integer.parseInt()/valueOf() for numeric parsing or keep it only for system property reads.",
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

fn is_integer_getinteger_call(owner: &str, name: &str, descriptor: &str) -> bool {
    owner == "java/lang/Integer"
        && name == "getInteger"
        && matches!(
            descriptor,
            "(Ljava/lang/String;)Ljava/lang/Integer;"
                | "(Ljava/lang/String;I)Ljava/lang/Integer;"
                | "(Ljava/lang/String;Ljava/lang/Integer;)Ljava/lang/Integer;"
        )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::test_harness::{JvmTestHarness, Language, SourceFile};

    fn getinteger_messages(output: &crate::engine::EngineOutput) -> Vec<String> {
        output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("INTEGER_GETINTEGER_CALL"))
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
    fn integer_getinteger_call_reports_single_arg_usage() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
public class ClassA {
    public Integer methodX(String varOne) {
        return Integer.getInteger(varOne);
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = getinteger_messages(&output);
        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("Avoid Integer.getInteger()")),
            "expected INTEGER_GETINTEGER_CALL finding, got {messages:?}"
        );
    }

    #[test]
    fn integer_getinteger_call_reports_string_int_overload() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassB.java".to_string(),
            contents: r#"
package com.example;
public class ClassB {
    public Integer methodY(String varOne) {
        return Integer.getInteger(varOne, 10);
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = getinteger_messages(&output);
        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("Avoid Integer.getInteger()")),
            "expected INTEGER_GETINTEGER_CALL finding for (String,int), got {messages:?}"
        );
    }

    #[test]
    fn integer_getinteger_call_ignores_parse_int_usage() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassC.java".to_string(),
            contents: r#"
package com.example;
public class ClassC {
    public int methodZ(String varOne) {
        return Integer.parseInt(varOne);
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = getinteger_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect INTEGER_GETINTEGER_CALL finding for parseInt(): {messages:?}"
        );
    }

    #[test]
    fn integer_getinteger_call_ignores_classpath_calls() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");

        let dependency_sources = vec![SourceFile {
            path: "com/example/ClassB.java".to_string(),
            contents: r#"
package com.example;
public class ClassB {
    public Integer methodY(String varOne) {
        return Integer.getInteger(varOne);
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
        let messages = getinteger_messages(&analysis);
        assert!(
            messages.is_empty(),
            "classpath classes must be out of scope for INTEGER_GETINTEGER_CALL: {messages:?}"
        );
    }
}
