use anyhow::Result;
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::engine::AnalysisContext;
use crate::ir::Method;
use crate::opcodes;
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

/// Rule that detects integer subtraction used as the comparison result in `compareTo` methods,
/// which can produce incorrect ordering for extreme values due to arithmetic overflow.
#[derive(Default)]
pub(crate) struct CompareToOverflowRule;

crate::register_rule!(CompareToOverflowRule);

impl Rule for CompareToOverflowRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "COMPARETO_OVERFLOW",
            name: "compareTo integer subtraction overflow",
            description: "compareTo using integer subtraction can overflow for extreme values",
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
                        if !is_compareto_returning_int(method) {
                            continue;
                        }
                        if calls_safe_integer_compare(method) {
                            continue;
                        }
                        if let Some(isub_offset) = first_isub_offset(method) {
                            let message = result_message(format!(
                                "Avoid integer subtraction in compareTo in {}.{}{}; use Integer.compare() to prevent overflow.",
                                class.name, method.name, method.descriptor
                            ));
                            let line = method.line_for_offset(isub_offset);
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

/// Returns true if the method is named `compareTo` and its descriptor indicates an `int` return.
fn is_compareto_returning_int(method: &Method) -> bool {
    method.name == "compareTo" && method.descriptor.ends_with(")I")
}

/// Returns true if the method contains a call to `Integer.compare` or `Long.compare`,
/// which are overflow-safe alternatives to integer subtraction.
fn calls_safe_integer_compare(method: &Method) -> bool {
    method.calls.iter().any(|call| {
        (call.owner == "java/lang/Integer" || call.owner == "java/lang/Long")
            && call.name == "compare"
    })
}

/// Returns the bytecode offset of the first `isub` instruction found in the method's basic blocks,
/// or `None` if no `isub` is present.
fn first_isub_offset(method: &Method) -> Option<u32> {
    for block in &method.cfg.blocks {
        for instruction in &block.instructions {
            if instruction.opcode == opcodes::ISUB {
                return Some(instruction.offset);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use crate::engine::EngineOutput;
    use crate::test_harness::{JvmTestHarness, Language, SourceFile};

    fn overflow_messages(output: &EngineOutput) -> Vec<String> {
        output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("COMPARETO_OVERFLOW"))
            .filter_map(|result| result.message.text.clone())
            .collect()
    }

    #[test]
    fn compareto_overflow_reports_direct_subtraction() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
public class ClassA implements Comparable<ClassA> {
    int varOne;
    @Override
    public int compareTo(ClassA other) {
        return this.varOne - other.varOne;
    }
}
"#
            .to_string(),
        }];

        let output = harness
            .compile_and_analyze(Language::Java, &sources, &[])
            .expect("run harness analysis");
        let messages = overflow_messages(&output);
        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("Avoid integer subtraction in compareTo")),
            "expected COMPARETO_OVERFLOW finding for direct subtraction, got {messages:?}"
        );
    }

    #[test]
    fn compareto_overflow_reports_multi_field_subtraction() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassB.java".to_string(),
            contents: r#"
package com.example;
public class ClassB implements Comparable<ClassB> {
    int varOne;
    int varTwo;
    @Override
    public int compareTo(ClassB other) {
        int diff = this.varOne - other.varOne;
        if (diff != 0) return diff;
        return this.varTwo - other.varTwo;
    }
}
"#
            .to_string(),
        }];

        let output = harness
            .compile_and_analyze(Language::Java, &sources, &[])
            .expect("run harness analysis");
        let messages = overflow_messages(&output);
        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("Avoid integer subtraction in compareTo")),
            "expected COMPARETO_OVERFLOW finding for multi-field subtraction, got {messages:?}"
        );
    }

    #[test]
    fn compareto_overflow_ignores_integer_compare() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassC.java".to_string(),
            contents: r#"
package com.example;
public class ClassC implements Comparable<ClassC> {
    int varOne;
    @Override
    public int compareTo(ClassC other) {
        return Integer.compare(this.varOne, other.varOne);
    }
}
"#
            .to_string(),
        }];

        let output = harness
            .compile_and_analyze(Language::Java, &sources, &[])
            .expect("run harness analysis");
        let messages = overflow_messages(&output);
        assert!(
            messages.is_empty(),
            "expected no COMPARETO_OVERFLOW for Integer.compare, got {messages:?}"
        );
    }

    #[test]
    fn compareto_overflow_ignores_string_compareto() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassD.java".to_string(),
            contents: r#"
package com.example;
public class ClassD implements Comparable<ClassD> {
    String varOne;
    @Override
    public int compareTo(ClassD other) {
        return this.varOne.compareTo(other.varOne);
    }
}
"#
            .to_string(),
        }];

        let output = harness
            .compile_and_analyze(Language::Java, &sources, &[])
            .expect("run harness analysis");
        let messages = overflow_messages(&output);
        assert!(
            messages.is_empty(),
            "expected no COMPARETO_OVERFLOW for String.compareTo delegation, got {messages:?}"
        );
    }

    #[test]
    fn compareto_overflow_ignores_isub_with_integer_compare_present() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassE.java".to_string(),
            contents: r#"
package com.example;
public class ClassE implements Comparable<ClassE> {
    int varOne;
    int varTwo;
    @Override
    public int compareTo(ClassE other) {
        int adjustment = this.varTwo - 1;
        return Integer.compare(this.varOne + adjustment, other.varOne + adjustment);
    }
}
"#
            .to_string(),
        }];

        let output = harness
            .compile_and_analyze(Language::Java, &sources, &[])
            .expect("run harness analysis");
        let messages = overflow_messages(&output);
        assert!(
            messages.is_empty(),
            "expected no COMPARETO_OVERFLOW when Integer.compare is present, got {messages:?}"
        );
    }

    #[test]
    fn compareto_overflow_ignores_classpath_classes() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");

        let dependency_sources = vec![SourceFile {
            path: "com/example/ClassB.java".to_string(),
            contents: r#"
package com.example;
public class ClassB implements Comparable<ClassB> {
    int varOne;
    @Override
    public int compareTo(ClassB other) {
        return this.varOne - other.varOne;
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
        let messages = overflow_messages(&analysis);
        assert!(
            messages.is_empty(),
            "classpath classes must be out of scope for COMPARETO_OVERFLOW: {messages:?}"
        );
    }
}
