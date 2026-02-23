use anyhow::Result;
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::engine::AnalysisContext;
use crate::ir::Instruction;
use crate::opcodes;
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

/// Rule that detects empty catch blocks.
#[derive(Default)]
pub(crate) struct EmptyCatchRule;

crate::register_rule!(EmptyCatchRule);

impl Rule for EmptyCatchRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "EMPTY_CATCH",
            name: "Empty catch block",
            description: "Catch blocks with no meaningful instructions",
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
                context.with_span("class", &attributes, || -> Result<Vec<SarifResult>> {
                    let mut class_results = Vec::new();
                    for method in &class.methods {
                        for handler in &method.exception_handlers {
                            let Some(block) = method
                                .cfg
                                .blocks
                                .iter()
                                .find(|block| block.start_offset == handler.handler_pc)
                            else {
                                continue;
                            };
                            if is_kotlin_enum_when_mapping_handler(
                                &class.name,
                                &method.name,
                                handler.catch_type.as_deref(),
                            ) {
                                continue;
                            }
                            if is_empty_handler(block.instructions.as_slice()) {
                                let message = result_message(format!(
                                    "Empty catch block in {}.{}{}",
                                    class.name, method.name, method.descriptor
                                ));
                                let line = method.line_for_offset(handler.handler_pc);
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

fn is_empty_handler(instructions: &[Instruction]) -> bool {
    if instructions.is_empty() {
        return true;
    }
    instructions
        .iter()
        .all(|inst| is_trivial_opcode(inst.opcode))
}

fn is_kotlin_enum_when_mapping_handler(
    class_name: &str,
    method_name: &str,
    catch_type: Option<&str>,
) -> bool {
    method_name == "<clinit>"
        && class_name.contains("$WhenMappings")
        && catch_type == Some("java/lang/NoSuchFieldError")
}

fn is_trivial_opcode(opcode: u8) -> bool {
    matches!(
        opcode,
        opcodes::NOP
            | opcodes::ASTORE
            | opcodes::ASTORE_0
            | opcodes::ASTORE_1
            | opcodes::ASTORE_2
            | opcodes::ASTORE_3
            | opcodes::POP
            | opcodes::GOTO
            | opcodes::JSR
            | opcodes::IRETURN
            | opcodes::LRETURN
            | opcodes::FRETURN
            | opcodes::DRETURN
            | opcodes::ARETURN
            | opcodes::RETURN
            | opcodes::ATHROW
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::descriptor::method_param_count;
    use crate::engine::build_context;
    use crate::ir::{
        BasicBlock, Class, ControlFlowGraph, ExceptionHandler, Instruction, InstructionKind,
        Method, MethodAccess, MethodNullness,
    };
    use crate::test_harness::{JvmTestHarness, Language, SourceFile};
    use serde_sarif::sarif::{Artifact, ArtifactLocation, ArtifactRoles};

    fn default_access() -> MethodAccess {
        MethodAccess {
            is_public: true,
            is_static: false,
            is_abstract: false,
            is_synthetic: false,
            is_bridge: false,
        }
    }

    fn method_with(
        name: &str,
        descriptor: &str,
        cfg: ControlFlowGraph,
        handlers: Vec<ExceptionHandler>,
    ) -> Method {
        Method {
            name: name.to_string(),
            descriptor: descriptor.to_string(),
            signature: None,
            access: default_access(),
            nullness: MethodNullness::unknown(method_param_count(descriptor).expect("param count")),
            type_use: None,
            bytecode: vec![0],
            line_numbers: Vec::new(),
            cfg,
            calls: Vec::new(),
            string_literals: Vec::new(),
            exception_handlers: handlers,
            local_variable_types: Vec::new(),
        }
    }

    fn class_with_methods(name: &str, methods: Vec<Method>) -> Class {
        Class {
            name: name.to_string(),
            source_file: None,
            super_name: None,
            interfaces: Vec::new(),
            type_parameters: Vec::new(),
            referenced_classes: Vec::new(),
            fields: Vec::new(),
            methods,
            annotation_defaults: Vec::new(),
            artifact_index: 0,
            is_record: false,
        }
    }

    fn class_with_methods_and_artifact(
        name: &str,
        artifact_index: i64,
        methods: Vec<Method>,
    ) -> Class {
        Class {
            name: name.to_string(),
            source_file: None,
            super_name: None,
            interfaces: Vec::new(),
            type_parameters: Vec::new(),
            referenced_classes: Vec::new(),
            fields: Vec::new(),
            methods,
            annotation_defaults: Vec::new(),
            artifact_index,
            is_record: false,
        }
    }

    fn context_for(classes: Vec<Class>) -> crate::engine::AnalysisContext {
        build_context(classes, &[])
    }

    fn context_for_with_artifacts(
        classes: Vec<Class>,
        artifacts: Vec<Artifact>,
    ) -> crate::engine::AnalysisContext {
        build_context(classes, &artifacts)
    }

    #[test]
    fn empty_catch_from_compiled_java() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "example/ClassA.java".to_string(),
            contents: r#"
package example;

public class ClassA {
    public void methodOne() {
        try {
            throw new RuntimeException("boom");
        } catch (RuntimeException varOne) {
        }
    }
}
"#
            .to_string(),
        }];

        let analysis = harness
            .compile_and_analyze(Language::Java, &sources, &[])
            .expect("compile and analyze");

        let has_empty_catch = analysis
            .results
            .iter()
            .any(|result| result.rule_id.as_deref() == Some("EMPTY_CATCH"));
        if !has_empty_catch {
            let messages = analysis
                .results
                .iter()
                .filter_map(|result| result.message.text.as_deref())
                .collect::<Vec<_>>()
                .join("\n");
            panic!("expected EMPTY_CATCH result, got:\n{messages}");
        }
    }

    #[test]
    fn empty_catch_ignores_kotlin_enum_pattern_match() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "example/ClassA.kt".to_string(),
            contents: r#"
package example

enum class ClassA {
    ONE,
    TWO,
}

fun methodOne(varOne: ClassA): Int = when (varOne) {
    ClassA.ONE -> 1
    ClassA.TWO -> 2
}
"#
            .to_string(),
        }];

        let analysis = harness
            .compile_and_analyze(Language::Kotlin, &sources, &[])
            .expect("compile and analyze");

        let empty_catch_messages: Vec<String> = analysis
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("EMPTY_CATCH"))
            .filter_map(|result| result.message.text.clone())
            .collect();

        assert!(
            empty_catch_messages.is_empty(),
            "expected no EMPTY_CATCH result, got: {empty_catch_messages:?}"
        );
    }

    #[test]
    fn empty_catch_rule_reports_trivial_handler() {
        let block = BasicBlock {
            start_offset: 0,
            end_offset: 1,
            instructions: vec![Instruction {
                offset: 0,
                opcode: opcodes::NOP,
                kind: InstructionKind::Other(opcodes::NOP),
            }],
        };
        let cfg = ControlFlowGraph {
            blocks: vec![block],
            edges: Vec::new(),
        };
        let handlers = vec![ExceptionHandler {
            start_pc: 0,
            end_pc: 1,
            handler_pc: 0,
            catch_type: None,
        }];
        let method = method_with("handle", "()V", cfg, handlers);
        let classes = vec![class_with_methods("com/example/App", vec![method])];
        let context = context_for(classes);

        let results = EmptyCatchRule.run(&context).expect("empty catch rule run");

        assert_eq!(1, results.len());
    }

    #[test]
    fn empty_catch_rule_ignores_non_trivial_handler() {
        let block = BasicBlock {
            start_offset: 0,
            end_offset: 1,
            instructions: vec![Instruction {
                offset: 0,
                opcode: opcodes::INVOKESTATIC,
                kind: InstructionKind::Other(opcodes::INVOKESTATIC),
            }],
        };
        let cfg = ControlFlowGraph {
            blocks: vec![block],
            edges: Vec::new(),
        };
        let handlers = vec![ExceptionHandler {
            start_pc: 0,
            end_pc: 1,
            handler_pc: 0,
            catch_type: None,
        }];
        let method = method_with("handle", "()V", cfg, handlers);
        let classes = vec![class_with_methods("com/example/App", vec![method])];
        let context = context_for(classes);

        let results = EmptyCatchRule.run(&context).expect("empty catch rule run");

        assert!(results.is_empty());
    }

    #[test]
    fn empty_catch_rule_skips_non_target_classes() {
        let block = BasicBlock {
            start_offset: 0,
            end_offset: 1,
            instructions: vec![Instruction {
                offset: 0,
                opcode: opcodes::NOP,
                kind: InstructionKind::Other(opcodes::NOP),
            }],
        };
        let cfg = ControlFlowGraph {
            blocks: vec![block],
            edges: Vec::new(),
        };
        let handlers = vec![ExceptionHandler {
            start_pc: 0,
            end_pc: 1,
            handler_pc: 0,
            catch_type: None,
        }];
        let target_method = method_with("entry", "()V", cfg.clone(), handlers.clone());
        let dependency_method = method_with("helper", "()V", cfg, handlers);
        let classes = vec![
            class_with_methods_and_artifact("com/example/App", 0, vec![target_method]),
            class_with_methods_and_artifact("org/example/Lib", 1, vec![dependency_method]),
        ];
        let artifacts = vec![
            Artifact::builder()
                .location(
                    ArtifactLocation::builder()
                        .uri("file:///target/app.jar".to_string())
                        .build(),
                )
                .roles(vec![
                    serde_json::to_value(ArtifactRoles::AnalysisTarget)
                        .expect("serialize artifact role"),
                ])
                .build(),
            Artifact::builder()
                .location(
                    ArtifactLocation::builder()
                        .uri("file:///deps/lib.jar".to_string())
                        .build(),
                )
                .build(),
        ];
        let context = context_for_with_artifacts(classes, artifacts);

        let results = EmptyCatchRule.run(&context).expect("empty catch rule run");

        assert_eq!(results.len(), 1);
        let message = results[0].message.text.as_deref().unwrap_or("");
        assert!(message.contains("com/example/App.entry()V"));
    }
}
