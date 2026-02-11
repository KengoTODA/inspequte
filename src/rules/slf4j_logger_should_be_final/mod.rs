use anyhow::Result;
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::engine::AnalysisContext;
use crate::rules::{Rule, RuleMetadata, class_location, result_message};

/// Rule that ensures SLF4J logger fields are final.
#[derive(Default)]
pub(crate) struct Slf4jLoggerShouldBeFinalRule;

crate::register_rule!(Slf4jLoggerShouldBeFinalRule);

impl Rule for Slf4jLoggerShouldBeFinalRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "SLF4J_LOGGER_SHOULD_BE_FINAL",
            name: "SLF4J logger should be final",
            description: "SLF4J Logger fields should be final",
        }
    }

    fn run(&self, context: &AnalysisContext) -> Result<Vec<SarifResult>> {
        if !context.has_slf4j() {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();
        for class in &context.classes {
            if !context.is_analysis_target_class(class) {
                continue;
            }
            let mut attributes = vec![KeyValue::new("inspequte.class", class.name.clone())];
            if let Some(uri) = context.class_artifact_uri(class) {
                attributes.push(KeyValue::new("inspequte.artifact_uri", uri));
            }
            let class_results =
                context.with_span("class", &attributes, || -> Result<Vec<SarifResult>> {
                    let mut class_results = Vec::new();
                    for field in &class.fields {
                        if field.descriptor != "Lorg/slf4j/Logger;" {
                            continue;
                        }
                        if field.access.is_final {
                            continue;
                        }
                        let message = result_message(format!(
                            "Logger field should be final: {}.{}",
                            class.name, field.name
                        ));
                        let artifact_uri = context.class_artifact_uri(class);
                        let location = class_location(&class.name, artifact_uri.as_deref());
                        class_results.push(
                            SarifResult::builder()
                                .message(message)
                                .locations(vec![location])
                                .build(),
                        );
                    }
                    Ok(class_results)
                })?;
            results.extend(class_results);
        }
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use crate::test_harness::{JvmTestHarness, Language, SourceFile};

    fn analyze_sources(sources: Vec<SourceFile>) -> Vec<String> {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let output = harness
            .compile_and_analyze(Language::Java, &sources, &[])
            .expect("run harness analysis");
        output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("SLF4J_LOGGER_SHOULD_BE_FINAL"))
            .filter_map(|result| result.message.text.clone())
            .collect()
    }

    fn slf4j_sources(contents: &str) -> Vec<SourceFile> {
        vec![
            SourceFile {
                path: "org/slf4j/Logger.java".to_string(),
                contents: r#"
package org.slf4j;
public interface Logger {}
"#
                .to_string(),
            },
            SourceFile {
                path: "com/example/ClassA.java".to_string(),
                contents: contents.to_string(),
            },
        ]
    }

    #[test]
    fn slf4j_logger_should_be_final_reports_non_final() {
        let sources = slf4j_sources(
            r#"
package com.example;
import org.slf4j.Logger;
public class ClassA {
    public Logger fieldA;
    private Logger fieldB;
    Logger fieldC;
}
"#,
        );

        let messages = analyze_sources(sources);

        assert_eq!(messages.len(), 3);
        assert!(messages.iter().any(|msg| msg.contains("ClassA.fieldA")));
        assert!(messages.iter().any(|msg| msg.contains("ClassA.fieldB")));
        assert!(messages.iter().any(|msg| msg.contains("ClassA.fieldC")));
    }

    #[test]
    fn slf4j_logger_should_be_final_allows_final() {
        let sources = slf4j_sources(
            r#"
package com.example;
import org.slf4j.Logger;
public class ClassA {
    private final Logger fieldA = null;
}
"#,
        );

        let messages = analyze_sources(sources);

        assert!(messages.is_empty());
    }
}
