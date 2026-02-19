use anyhow::Result;
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::engine::AnalysisContext;
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

/// Rule that detects `BigDecimal.setScale(int)` calls without explicit rounding.
#[derive(Default)]
pub(crate) struct BigDecimalSetScaleWithoutRoundingRule;

crate::register_rule!(BigDecimalSetScaleWithoutRoundingRule);

impl Rule for BigDecimalSetScaleWithoutRoundingRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "BIGDECIMAL_SET_SCALE_WITHOUT_ROUNDING",
            name: "BigDecimal setScale without rounding",
            description: "BigDecimal.setScale(int) can throw when rounding is required",
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
                            if is_unrounded_set_scale(&call.owner, &call.name, &call.descriptor) {
                                let message = result_message(format!(
                                    "Avoid BigDecimal.setScale(...) without rounding in {}.{}{}; specify RoundingMode.",
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

fn is_unrounded_set_scale(owner: &str, name: &str, descriptor: &str) -> bool {
    owner == "java/math/BigDecimal"
        && name == "setScale"
        && descriptor == "(I)Ljava/math/BigDecimal;"
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::test_harness::{JvmTestHarness, Language, SourceFile};

    fn set_scale_messages(output: &crate::engine::EngineOutput) -> Vec<String> {
        output
            .results
            .iter()
            .filter(|result| {
                result.rule_id.as_deref() == Some("BIGDECIMAL_SET_SCALE_WITHOUT_ROUNDING")
            })
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
    fn bigdecimal_setscale_without_rounding_reports_one_arg_call() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
import java.math.BigDecimal;
public class ClassA {
    public BigDecimal methodX(BigDecimal varOne) {
        return varOne.setScale(2);
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = set_scale_messages(&output);
        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("Avoid BigDecimal.setScale(...) without rounding")),
            "expected BIGDECIMAL_SET_SCALE_WITHOUT_ROUNDING finding, got {messages:?}"
        );
    }

    #[test]
    fn bigdecimal_setscale_without_rounding_ignores_rounding_mode_overload() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassB.java".to_string(),
            contents: r#"
package com.example;
import java.math.BigDecimal;
import java.math.RoundingMode;
public class ClassB {
    public BigDecimal methodY(BigDecimal varOne) {
        return varOne.setScale(2, RoundingMode.HALF_UP);
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = set_scale_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect BIGDECIMAL_SET_SCALE_WITHOUT_ROUNDING finding for setScale with RoundingMode: {messages:?}"
        );
    }

    #[test]
    fn bigdecimal_setscale_without_rounding_ignores_classpath_calls() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");

        let dependency_sources = vec![SourceFile {
            path: "com/example/ClassB.java".to_string(),
            contents: r#"
package com.example;
import java.math.BigDecimal;
public class ClassB {
    public BigDecimal methodY(BigDecimal varOne) {
        return varOne.setScale(2);
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
        let messages = set_scale_messages(&analysis);
        assert!(
            messages.is_empty(),
            "classpath classes must be out of scope for BIGDECIMAL_SET_SCALE_WITHOUT_ROUNDING: {messages:?}"
        );
    }
}
