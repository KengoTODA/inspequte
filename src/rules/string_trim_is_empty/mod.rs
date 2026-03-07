use anyhow::Result;
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::engine::AnalysisContext;
use crate::ir::{CallSite, Method};
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

/// Rule that detects direct `String.trim().isEmpty()` call chains.
#[derive(Default)]
pub(crate) struct StringTrimIsEmptyRule;

crate::register_rule!(StringTrimIsEmptyRule);

impl Rule for StringTrimIsEmptyRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "STRING_TRIM_IS_EMPTY",
            name: "String trim followed by isEmpty",
            description: "String.trim().isEmpty() can be ambiguous; prefer String.isBlank()",
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
                        for offset in direct_trim_is_empty_offsets(method) {
                            let message = result_message(format!(
                                "String blank check in {}.{}{} uses trim().isEmpty(); replace with isBlank() (Java 11+) for clearer Unicode-aware whitespace handling.",
                                class.name, method.name, method.descriptor
                            ));
                            let line = method.line_for_offset(offset);
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
                    Ok(class_results)
                })?;
            results.extend(class_results);
        }
        Ok(results)
    }
}

fn direct_trim_is_empty_offsets(method: &Method) -> Vec<u32> {
    method
        .calls
        .windows(2)
        .filter_map(|pair| {
            let [first, second] = pair else {
                return None;
            };
            if !is_string_trim_call(first) || !is_string_is_empty_call(second) {
                return None;
            }
            let length = crate::scan::opcode_length(&method.bytecode, first.offset as usize).ok()?;
            if first.offset + length as u32 == second.offset {
                return Some(second.offset);
            }
            None
        })
        .collect()
}

fn is_string_trim_call(call: &CallSite) -> bool {
    call.owner == "java/lang/String"
        && call.name == "trim"
        && call.descriptor == "()Ljava/lang/String;"
}

fn is_string_is_empty_call(call: &CallSite) -> bool {
    call.owner == "java/lang/String" && call.name == "isEmpty" && call.descriptor == "()Z"
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::test_harness::{JvmTestHarness, Language, SourceFile};

    fn trim_is_empty_messages(output: &crate::engine::EngineOutput) -> Vec<String> {
        output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("STRING_TRIM_IS_EMPTY"))
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
    fn string_trim_is_empty_reports_direct_chain() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
public class ClassA {
    public boolean methodX(String varOne) {
        return varOne.trim().isEmpty();
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = trim_is_empty_messages(&output);
        assert_eq!(messages.len(), 1, "expected one finding, got: {messages:?}");
        assert!(messages[0].contains("replace with isBlank()"));
    }

    #[test]
    fn string_trim_is_empty_ignores_is_blank_and_plain_is_empty() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassB.java".to_string(),
            contents: r#"
package com.example;
public class ClassB {
    public boolean methodY(String varOne) {
        boolean varTwo = varOne.isBlank();
        boolean varThree = varOne.isEmpty();
        return varTwo || varThree;
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = trim_is_empty_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect STRING_TRIM_IS_EMPTY finding, got: {messages:?}"
        );
    }

    #[test]
    fn string_trim_is_empty_reports_only_direct_chain_in_mixed_case() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassC.java".to_string(),
            contents: r#"
package com.example;
public class ClassC {
    public boolean methodZ(String varOne, String varTwo) {
        String varThree = varTwo.trim();
        boolean varFour = varOne.trim().isEmpty();
        boolean varFive = varThree.isEmpty();
        return varFour || varFive;
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = trim_is_empty_messages(&output);
        assert_eq!(messages.len(), 1, "expected one finding, got: {messages:?}");
    }

    #[test]
    fn string_trim_is_empty_ignores_classpath_calls() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");

        let dependency_sources = vec![SourceFile {
            path: "com/example/ClassB.java".to_string(),
            contents: r#"
package com.example;
public class ClassB {
    public boolean methodY(String varOne) {
        return varOne.trim().isEmpty();
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
        let messages = trim_is_empty_messages(&analysis);
        assert!(
            messages.is_empty(),
            "classpath classes must be out of scope for STRING_TRIM_IS_EMPTY: {messages:?}"
        );
    }
}
