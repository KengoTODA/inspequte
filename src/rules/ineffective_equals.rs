use anyhow::Result;
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::engine::AnalysisContext;
use crate::rules::{Rule, RuleMetadata, class_location, result_message};

/// Rule that flags classes overriding equals or hashCode alone.
#[derive(Default)]
pub(crate) struct IneffectiveEqualsRule;

crate::register_rule!(IneffectiveEqualsRule);

impl Rule for IneffectiveEqualsRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "INEFFECTIVE_EQUALS_HASHCODE",
            name: "Ineffective equals/hashCode",
            description: "Classes with equals without hashCode or vice versa",
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
                    let mut has_equals = false;
                    let mut has_hashcode = false;
                    for method in &class.methods {
                        if method.name == "equals" && method.descriptor == "(Ljava/lang/Object;)Z" {
                            has_equals = true;
                        }
                        if method.name == "hashCode" && method.descriptor == "()I" {
                            has_hashcode = true;
                        }
                    }
                    if has_equals ^ has_hashcode {
                        let message = if has_equals {
                            result_message(format!(
                                "Class {} overrides equals without hashCode",
                                class.name
                            ))
                        } else {
                            result_message(format!(
                                "Class {} overrides hashCode without equals",
                                class.name
                            ))
                        };
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
    use super::*;
    use crate::descriptor::method_param_count;
    use crate::engine::build_context;
    use crate::ir::{Class, ControlFlowGraph, Method, MethodAccess, MethodNullness};
    use crate::test_harness::{JvmTestHarness, Language, SourceFile};

    fn empty_cfg() -> ControlFlowGraph {
        ControlFlowGraph {
            blocks: Vec::new(),
            edges: Vec::new(),
        }
    }

    fn method_with(name: &str, descriptor: &str) -> Method {
        Method {
            name: name.to_string(),
            descriptor: descriptor.to_string(),
            signature: None,
            access: MethodAccess {
                is_public: true,
                is_static: false,
                is_abstract: false,
            },
            nullness: MethodNullness::unknown(method_param_count(descriptor).expect("param count")),
            type_use: None,
            bytecode: vec![0],
            line_numbers: Vec::new(),
            cfg: empty_cfg(),
            calls: Vec::new(),
            string_literals: Vec::new(),
            exception_handlers: Vec::new(),
            local_variable_types: Vec::new(),
        }
    }

    fn class_with_methods(name: &str, methods: Vec<Method>) -> Class {
        Class {
            name: name.to_string(),
            super_name: None,
            interfaces: Vec::new(),
            type_parameters: Vec::new(),
            referenced_classes: Vec::new(),
            fields: Vec::new(),
            methods,
            artifact_index: 0,
            is_record: false,
        }
    }

    fn context_for(classes: Vec<Class>) -> crate::engine::AnalysisContext {
        build_context(classes, &[])
    }

    #[test]
    fn ineffective_equals_rule_reports_missing_pair() {
        let equals = method_with("equals", "(Ljava/lang/Object;)Z");
        let classes = vec![class_with_methods("com/example/Value", vec![equals])];
        let context = context_for(classes);

        let results = IneffectiveEqualsRule
            .run(&context)
            .expect("ineffective equals rule run");

        assert_eq!(1, results.len());
        let message = results[0].message.text.as_deref().unwrap_or("");
        assert!(message.contains("overrides equals without hashCode"));
    }

    #[test]
    fn ineffective_equals_rule_ignores_complete_pairs() {
        let equals = method_with("equals", "(Ljava/lang/Object;)Z");
        let hashcode = method_with("hashCode", "()I");
        let classes = vec![class_with_methods(
            "com/example/Value",
            vec![equals, hashcode],
        )];
        let context = context_for(classes);

        let results = IneffectiveEqualsRule
            .run(&context)
            .expect("ineffective equals rule run");

        assert!(results.is_empty());
    }

    #[test]
    fn ineffective_equals_rule_ignores_mismatched_signatures() {
        let equals = method_with("equals", "()Z");
        let classes = vec![class_with_methods("com/example/Value", vec![equals])];
        let context = context_for(classes);

        let results = IneffectiveEqualsRule
            .run(&context)
            .expect("ineffective equals rule run");

        assert!(results.is_empty());
    }

    #[test]
    fn ineffective_equals_rule_reports_override_from_harness() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
public class ClassA {
    @Override
    public boolean equals(Object varOne) {
        return varOne instanceof ClassA;
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
            .filter(|result| result.rule_id.as_deref() == Some("INEFFECTIVE_EQUALS_HASHCODE"))
            .filter_map(|result| result.message.text.clone())
            .collect();

        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("overrides equals without hashCode"))
        );
    }
}
