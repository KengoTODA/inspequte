use anyhow::Result;
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::engine::AnalysisContext;
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

/// Rule that detects locale-dependent string formatting calls.
#[derive(Default)]
pub(crate) struct StringFormatLocaleMissingRule;

crate::register_rule!(StringFormatLocaleMissingRule);

impl Rule for StringFormatLocaleMissingRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "STRING_FORMAT_LOCALE_MISSING",
            name: "String/Formatter formatting without explicit locale",
            description: "String.format(...) and Formatter usage without Locale can vary by runtime locale",
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
                            if is_locale_missing_format_call(call) {
                                let message = result_message(format!(
                                    "Formatting in {}.{}{} depends on the default locale; pass Locale.ROOT (or another explicit Locale).",
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

fn is_locale_missing_format_call(call: &crate::ir::CallSite) -> bool {
    is_string_format_without_locale(call) || is_formatter_without_locale(call)
}

fn is_string_format_without_locale(call: &crate::ir::CallSite) -> bool {
    call.owner == "java/lang/String"
        && call.name == "format"
        && call.descriptor == "(Ljava/lang/String;[Ljava/lang/Object;)Ljava/lang/String;"
}

fn is_formatter_without_locale(call: &crate::ir::CallSite) -> bool {
    if call.owner != "java/util/Formatter" {
        return false;
    }

    if call.name == "format" {
        return call.descriptor == "(Ljava/lang/String;[Ljava/lang/Object;)Ljava/util/Formatter;";
    }

    if call.name != "<init>" {
        return false;
    }

    matches!(
        call.descriptor.as_str(),
        "()V"
            | "(Ljava/lang/Appendable;)V"
            | "(Ljava/lang/String;)V"
            | "(Ljava/lang/String;Ljava/lang/String;)V"
            | "(Ljava/io/File;)V"
            | "(Ljava/io/File;Ljava/lang/String;)V"
            | "(Ljava/io/PrintStream;)V"
            | "(Ljava/io/OutputStream;)V"
            | "(Ljava/io/OutputStream;Ljava/lang/String;)V"
    )
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
            .filter(|result| result.rule_id.as_deref() == Some("STRING_FORMAT_LOCALE_MISSING"))
            .filter_map(|result| result.message.text.clone())
            .collect()
    }

    #[test]
    fn reports_string_format_without_locale() {
        let sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;

class ClassA {
    String methodX(int varOne) {
        return String.format("value=%d", varOne);
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_sources(sources);
        assert_eq!(messages.len(), 1, "expected one finding, got: {messages:?}");
    }

    #[test]
    fn reports_formatter_without_locale() {
        let sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;

import java.util.Formatter;

class ClassA {
    String methodX(int varOne) {
        Formatter varTwo = new Formatter();
        varTwo.format("value=%d", varOne);
        return varTwo.toString();
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_sources(sources);
        assert_eq!(messages.len(), 2, "expected two findings, got: {messages:?}");
    }

    #[test]
    fn reports_each_supported_formatter_constructor_without_locale() {
        let sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;

import java.io.ByteArrayOutputStream;
import java.io.File;
import java.util.Formatter;

class ClassA {
    void methodX() throws Exception {
        File varOne = File.createTempFile("aa", "bb");
        ByteArrayOutputStream varTwo = new ByteArrayOutputStream();
        new Formatter();
        new Formatter(new StringBuilder());
        new Formatter("var-three.txt");
        new Formatter("var-four.txt", "UTF-8");
        new Formatter(varOne);
        new Formatter(varOne, "UTF-8");
        new Formatter(System.out);
        new Formatter(varTwo);
        new Formatter(varTwo, "UTF-8");
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_sources(sources);
        assert_eq!(messages.len(), 9, "expected nine findings, got: {messages:?}");
    }

    #[test]
    fn does_not_report_locale_aware_calls() {
        let sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;

import java.util.Formatter;
import java.util.Locale;

class ClassA {
    String methodX(int varOne) {
        Formatter varTwo = new Formatter(Locale.ROOT);
        varTwo.format(Locale.ROOT, "value=%d", varOne);
        return String.format(Locale.ROOT, "value=%d", varOne);
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_sources(sources);
        assert!(
            messages.is_empty(),
            "expected no findings for locale-aware calls, got: {messages:?}"
        );
    }

    #[test]
    fn does_not_report_locale_aware_formatter_constructors() {
        let sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;

import java.io.ByteArrayOutputStream;
import java.io.File;
import java.nio.charset.StandardCharsets;
import java.util.Formatter;
import java.util.Locale;

class ClassA {
    void methodX() throws Exception {
        File varOne = File.createTempFile("aa", "bb");
        ByteArrayOutputStream varTwo = new ByteArrayOutputStream();
        new Formatter(Locale.ROOT);
        new Formatter(new StringBuilder(), Locale.ROOT);
        new Formatter("var-three.txt", "UTF-8", Locale.ROOT);
        new Formatter("var-four.txt", StandardCharsets.UTF_8, Locale.ROOT);
        new Formatter(varOne, "UTF-8", Locale.ROOT);
        new Formatter(varOne, StandardCharsets.UTF_8, Locale.ROOT);
        new Formatter(varTwo, "UTF-8", Locale.ROOT);
        new Formatter(varTwo, StandardCharsets.UTF_8, Locale.ROOT);
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_sources(sources);
        assert!(
            messages.is_empty(),
            "expected no findings for locale-aware constructors, got: {messages:?}"
        );
    }
}
