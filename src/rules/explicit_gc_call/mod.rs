use anyhow::Result;
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::engine::AnalysisContext;
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

/// Rule that detects explicit garbage collection API calls.
#[derive(Default)]
pub(crate) struct ExplicitGcCallRule;

crate::register_rule!(ExplicitGcCallRule);

impl Rule for ExplicitGcCallRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "EXPLICIT_GC_CALL",
            name: "Explicit GC call",
            description: "Direct calls to explicit GC APIs should be avoided",
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
                            if is_explicit_gc_call(&call.owner, &call.name, &call.descriptor) {
                                let message = result_message(format!(
                                    "Avoid explicit GC call in {}.{}{}; let the JVM manage garbage collection.",
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

fn is_explicit_gc_call(owner: &str, name: &str, descriptor: &str) -> bool {
    matches!(
        (owner, name, descriptor),
        ("java/lang/System", "gc", "()V") | ("java/lang/Runtime", "gc", "()V")
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::test_harness::{JvmTestHarness, Language, SourceFile};

    fn explicit_gc_messages(output: &crate::engine::EngineOutput) -> Vec<String> {
        output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("EXPLICIT_GC_CALL"))
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
    fn explicit_gc_reports_system_gc() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
public class ClassA {
    public void methodX(boolean varOne) {
        if (varOne) {
            System.gc();
        }
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = explicit_gc_messages(&output);

        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("Avoid explicit GC call")),
            "expected EXPLICIT_GC_CALL finding for System.gc, got {messages:?}"
        );
    }

    #[test]
    fn explicit_gc_reports_runtime_gc() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassB.java".to_string(),
            contents: r#"
package com.example;
public class ClassB {
    public void methodY(boolean varOne) {
        if (varOne) {
            Runtime varTwo = Runtime.getRuntime();
            varTwo.gc();
        }
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = explicit_gc_messages(&output);

        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("Avoid explicit GC call")),
            "expected EXPLICIT_GC_CALL finding for Runtime.gc, got {messages:?}"
        );
    }

    #[test]
    fn explicit_gc_ignores_non_gc_system_calls() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassC.java".to_string(),
            contents: r#"
package com.example;
public class ClassC {
    public String methodZ() {
        return System.lineSeparator();
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = explicit_gc_messages(&output);

        assert!(
            messages.is_empty(),
            "did not expect EXPLICIT_GC_CALL finding for non-GC System call: {messages:?}"
        );
    }

    #[test]
    fn explicit_gc_ignores_classpath_calls() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");

        let dependency_sources = vec![SourceFile {
            path: "com/example/ClassB.java".to_string(),
            contents: r#"
package com.example;
public class ClassB {
    public void methodY() {
        System.gc();
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
        let messages = explicit_gc_messages(&analysis);

        assert!(
            messages.is_empty(),
            "classpath classes must be out of scope for EXPLICIT_GC_CALL: {messages:?}"
        );
    }
}
