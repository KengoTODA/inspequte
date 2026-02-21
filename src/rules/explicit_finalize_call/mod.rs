use anyhow::Result;
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::engine::AnalysisContext;
use crate::ir::CallKind;
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

/// Rule that detects explicit virtual calls to `finalize()` on object instances.
#[derive(Default)]
pub(crate) struct ExplicitFinalizeCallRule;

crate::register_rule!(ExplicitFinalizeCallRule);

impl Rule for ExplicitFinalizeCallRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "EXPLICIT_FINALIZE_CALL",
            name: "Explicit finalize call",
            description: "Direct virtual calls to finalize() bypass GC lifecycle and indicate broken resource cleanup",
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
                            if is_explicit_finalize_call(&call.name, &call.descriptor, call.kind) {
                                let message = result_message(format!(
                                    "Explicit call to finalize() in {}.{}{}; use AutoCloseable with try-with-resources or java.lang.ref.Cleaner for deterministic resource cleanup.",
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

fn is_explicit_finalize_call(name: &str, descriptor: &str, kind: CallKind) -> bool {
    name == "finalize" && descriptor == "()V" && kind == CallKind::Virtual
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::engine::EngineOutput;
    use crate::test_harness::{JvmTestHarness, Language, SourceFile};

    fn finalize_messages(output: &EngineOutput) -> Vec<String> {
        output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("EXPLICIT_FINALIZE_CALL"))
            .filter_map(|result| result.message.text.clone())
            .collect()
    }

    fn compile_and_analyze(
        harness: &JvmTestHarness,
        sources: &[SourceFile],
        classpath: &[PathBuf],
    ) -> EngineOutput {
        harness
            .compile_and_analyze(Language::Java, sources, classpath)
            .expect("run harness analysis")
    }

    #[test]
    fn explicit_finalize_call_reports_call_on_local_variable() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        // Use the class's own type for the local variable so protected access compiles.
        let sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
@SuppressWarnings("deprecation")
public class ClassA {
    public void methodOne() throws Throwable {
        ClassA varOne = new ClassA();
        varOne.finalize();
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = finalize_messages(&output);
        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("Explicit call to finalize()")),
            "expected EXPLICIT_FINALIZE_CALL finding for explicit finalize call, got {messages:?}"
        );
    }

    #[test]
    fn explicit_finalize_call_reports_call_on_this() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassB.java".to_string(),
            contents: r#"
package com.example;
@SuppressWarnings("deprecation")
public class ClassB {
    public void methodTwo() throws Throwable {
        this.finalize();
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = finalize_messages(&output);
        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("Explicit call to finalize()")),
            "expected EXPLICIT_FINALIZE_CALL finding for this.finalize(), got {messages:?}"
        );
    }

    #[test]
    fn explicit_finalize_call_ignores_super_finalize_in_override() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassC.java".to_string(),
            contents: r#"
package com.example;
@SuppressWarnings("deprecation")
public class ClassC {
    @Override
    protected void finalize() throws Throwable {
        super.finalize();
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = finalize_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect EXPLICIT_FINALIZE_CALL finding for super.finalize(): {messages:?}"
        );
    }

    #[test]
    fn explicit_finalize_call_ignores_finalize_override_declaration() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassD.java".to_string(),
            contents: r#"
package com.example;
@SuppressWarnings("deprecation")
public class ClassD {
    @Override
    protected void finalize() throws Throwable {
        // cleanup
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = finalize_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect EXPLICIT_FINALIZE_CALL finding for finalize() override: {messages:?}"
        );
    }

    #[test]
    fn explicit_finalize_call_ignores_unrelated_void_methods() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassE.java".to_string(),
            contents: r#"
package com.example;
public class ClassE {
    public void methodThree() {
        System.gc();
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = finalize_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect EXPLICIT_FINALIZE_CALL finding for System.gc(): {messages:?}"
        );
    }

    #[test]
    fn explicit_finalize_call_ignores_classpath_classes() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");

        // Use ClassF's own type for the local variable so protected access compiles.
        let dependency_sources = vec![SourceFile {
            path: "com/example/ClassF.java".to_string(),
            contents: r#"
package com.example;
@SuppressWarnings("deprecation")
public class ClassF {
    public void methodFour() throws Throwable {
        ClassF varOne = new ClassF();
        varOne.finalize();
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
    public void methodOne() {}
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
        let messages = finalize_messages(&analysis);
        assert!(
            messages.is_empty(),
            "classpath classes must be out of scope for EXPLICIT_FINALIZE_CALL: {messages:?}"
        );
    }
}
