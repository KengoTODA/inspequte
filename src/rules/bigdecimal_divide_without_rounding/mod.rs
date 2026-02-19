use anyhow::Result;
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::engine::AnalysisContext;
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

/// Rule that detects `BigDecimal.divide(BigDecimal)` calls without rounding config.
#[derive(Default)]
pub(crate) struct BigDecimalDivideWithoutRoundingRule;

crate::register_rule!(BigDecimalDivideWithoutRoundingRule);

impl Rule for BigDecimalDivideWithoutRoundingRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "BIGDECIMAL_DIVIDE_WITHOUT_ROUNDING",
            name: "BigDecimal divide without rounding",
            description: "BigDecimal.divide(BigDecimal) can throw on non-terminating decimals",
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
                            if is_unrounded_bigdecimal_divide(
                                &call.owner,
                                &call.name,
                                &call.descriptor,
                            ) {
                                let message = result_message(format!(
                                    "Avoid BigDecimal.divide(...) without rounding in {}.{}{}; specify RoundingMode or MathContext.",
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

fn is_unrounded_bigdecimal_divide(owner: &str, name: &str, descriptor: &str) -> bool {
    owner == "java/math/BigDecimal"
        && name == "divide"
        && descriptor == "(Ljava/math/BigDecimal;)Ljava/math/BigDecimal;"
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::test_harness::{JvmTestHarness, Language, SourceFile};

    fn divide_messages(output: &crate::engine::EngineOutput) -> Vec<String> {
        output
            .results
            .iter()
            .filter(|result| {
                result.rule_id.as_deref() == Some("BIGDECIMAL_DIVIDE_WITHOUT_ROUNDING")
            })
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
    fn bigdecimal_divide_without_rounding_reports_one_arg_divide() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
import java.math.BigDecimal;
public class ClassA {
    public BigDecimal methodX(BigDecimal varOne, BigDecimal varTwo) {
        return varOne.divide(varTwo);
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, Language::Java, &sources, &[]);
        let messages = divide_messages(&output);
        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("Avoid BigDecimal.divide(...) without rounding")),
            "expected BIGDECIMAL_DIVIDE_WITHOUT_ROUNDING finding, got {messages:?}"
        );
    }

    #[test]
    fn bigdecimal_divide_without_rounding_ignores_divide_with_rounding_mode() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassB.java".to_string(),
            contents: r#"
package com.example;
import java.math.BigDecimal;
import java.math.RoundingMode;
public class ClassB {
    public BigDecimal methodY(BigDecimal varOne, BigDecimal varTwo) {
        return varOne.divide(varTwo, RoundingMode.HALF_UP);
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, Language::Java, &sources, &[]);
        let messages = divide_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect BIGDECIMAL_DIVIDE_WITHOUT_ROUNDING finding for divide with RoundingMode: {messages:?}"
        );
    }

    #[test]
    fn bigdecimal_divide_without_rounding_ignores_divide_with_math_context() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassC.java".to_string(),
            contents: r#"
package com.example;
import java.math.BigDecimal;
import java.math.MathContext;
public class ClassC {
    public BigDecimal methodZ(BigDecimal varOne, BigDecimal varTwo) {
        return varOne.divide(varTwo, MathContext.DECIMAL64);
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, Language::Java, &sources, &[]);
        let messages = divide_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect BIGDECIMAL_DIVIDE_WITHOUT_ROUNDING finding for divide with MathContext: {messages:?}"
        );
    }

    #[test]
    fn bigdecimal_divide_without_rounding_ignores_kotlin_operator_div() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/file_a.kt".to_string(),
            contents: r#"
package com.example

import java.math.BigDecimal

fun methodY(varOne: BigDecimal, varTwo: BigDecimal): BigDecimal {
    return varOne / varTwo
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, Language::Kotlin, &sources, &[]);
        let messages = divide_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect BIGDECIMAL_DIVIDE_WITHOUT_ROUNDING finding for Kotlin operator /: {messages:?}"
        );
    }

    #[test]
    fn bigdecimal_divide_without_rounding_ignores_classpath_calls() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");

        let dependency_sources = vec![SourceFile {
            path: "com/example/ClassB.java".to_string(),
            contents: r#"
package com.example;
import java.math.BigDecimal;
public class ClassB {
    public BigDecimal methodY(BigDecimal varOne, BigDecimal varTwo) {
        return varOne.divide(varTwo);
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
        let messages = divide_messages(&analysis);
        assert!(
            messages.is_empty(),
            "classpath classes must be out of scope for BIGDECIMAL_DIVIDE_WITHOUT_ROUNDING: {messages:?}"
        );
    }
}
