use anyhow::Result;
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::engine::AnalysisContext;
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

/// Rule that detects insecure API usage.
pub(crate) struct InsecureApiRule;

impl Rule for InsecureApiRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "INSECURE_API",
            name: "Insecure API usage",
            description: "Calls to insecure process or reflection APIs",
        }
    }

    fn run(&self, context: &AnalysisContext) -> Result<Vec<SarifResult>> {
        let mut results = Vec::new();
        for class in &context.classes {
            let mut attributes = vec![KeyValue::new("inspequte.class", class.name.clone())];
            if let Some(uri) = context.class_artifact_uri(class) {
                attributes.push(KeyValue::new("inspequte.artifact_uri", uri));
            }
            let class_results =
                context.with_span("class", &attributes, || -> Result<Vec<SarifResult>> {
                    let mut class_results = Vec::new();
                    for method in &class.methods {
                        for call in &method.calls {
                            if is_insecure_call(&call.owner, &call.name) {
                                let message = result_message(format!(
                                    "Insecure API usage: {}.{}",
                                    call.owner, call.name
                                ));
                                let line = method.line_for_offset(call.offset);
                                let artifact_uri = context.class_artifact_uri(class);
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

fn is_insecure_call(owner: &str, name: &str) -> bool {
    matches!(
        (owner, name),
        ("java/lang/Runtime", "exec")
            | ("java/lang/ProcessBuilder", "<init>")
            | ("java/lang/ProcessBuilder", "start")
            | ("java/lang/reflect/Method", "invoke")
            | ("java/lang/reflect/Constructor", "newInstance")
            | ("java/lang/Class", "forName")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classpath::resolve_classpath;
    use crate::descriptor::method_param_count;
    use crate::engine::build_context;
    use crate::ir::{
        CallKind, CallSite, Class, ControlFlowGraph, Method, MethodAccess, MethodNullness,
    };
    use crate::test_harness::{JvmTestHarness, Language, SourceFile};

    fn empty_cfg() -> ControlFlowGraph {
        ControlFlowGraph {
            blocks: Vec::new(),
            edges: Vec::new(),
        }
    }

    fn method_with(name: &str, calls: Vec<CallSite>) -> Method {
        Method {
            name: name.to_string(),
            descriptor: "()V".to_string(),
            access: MethodAccess {
                is_public: true,
                is_static: false,
                is_abstract: false,
            },
            nullness: MethodNullness::unknown(method_param_count("()V").expect("param count")),
            bytecode: vec![0],
            line_numbers: Vec::new(),
            cfg: empty_cfg(),
            calls,
            string_literals: Vec::new(),
            exception_handlers: Vec::new(),
        }
    }

    fn class_with_methods(name: &str, methods: Vec<Method>) -> Class {
        Class {
            name: name.to_string(),
            super_name: None,
            interfaces: Vec::new(),
            referenced_classes: Vec::new(),
            fields: Vec::new(),
            methods,
            artifact_index: 0,
            is_record: false,
        }
    }

    fn context_for(classes: Vec<Class>) -> crate::engine::AnalysisContext {
        let classpath = resolve_classpath(&classes).expect("classpath build");
        build_context(classes, classpath, &[])
    }

    #[test]
    fn insecure_api_rule_reports_matches() {
        let method = method_with(
            "run",
            vec![CallSite {
                owner: "java/lang/Runtime".to_string(),
                name: "exec".to_string(),
                descriptor: "(Ljava/lang/String;)V".to_string(),
                kind: CallKind::Virtual,
                offset: 0,
            }],
        );
        let classes = vec![class_with_methods("com/example/App", vec![method])];
        let context = context_for(classes);

        let results = InsecureApiRule
            .run(&context)
            .expect("insecure api rule run");

        assert_eq!(1, results.len());
        let message = results[0].message.text.as_deref().unwrap_or("");
        assert!(message.contains("Insecure API usage: java/lang/Runtime.exec"));
    }

    #[test]
    fn insecure_api_rule_ignores_safe_calls() {
        let method = method_with(
            "run",
            vec![CallSite {
                owner: "java/lang/String".to_string(),
                name: "length".to_string(),
                descriptor: "()I".to_string(),
                kind: CallKind::Virtual,
                offset: 0,
            }],
        );
        let classes = vec![class_with_methods("com/example/App", vec![method])];
        let context = context_for(classes);

        let results = InsecureApiRule
            .run(&context)
            .expect("insecure api rule run");

        assert!(results.is_empty());
    }

    #[test]
    fn insecure_api_rule_reports_runtime_exec_from_harness() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/Runner.java".to_string(),
            contents: r#"
package com.example;
public class Runner {
    public void run() throws Exception {
        Runtime.getRuntime().exec("echo");
    }
}
"#
            .to_string(),
        }];

        let output = harness
            .compile_and_analyze(Language::Java, &sources, &[])
            .expect("run harness analysis");

        let messages: Vec<String> = output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("INSECURE_API"))
            .filter_map(|result| result.message.text.clone())
            .collect();

        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("java/lang/Runtime.exec"))
        );
    }
}
