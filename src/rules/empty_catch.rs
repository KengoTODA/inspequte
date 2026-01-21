use anyhow::Result;
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::engine::AnalysisContext;
use crate::ir::Instruction;
use crate::opcodes;
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

/// Rule that detects empty catch blocks.
pub(crate) struct EmptyCatchRule;

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
        for class in &context.classes {
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
    use crate::classpath::resolve_classpath;
    use crate::descriptor::method_param_count;
    use crate::engine::build_context;
    use crate::ir::{
        BasicBlock, Class, ControlFlowGraph, ExceptionHandler, Instruction, InstructionKind,
        Method, MethodAccess, MethodNullness,
    };
    use crate::test_harness::{JvmTestHarness, Language, SourceFile};

    fn default_access() -> MethodAccess {
        MethodAccess {
            is_public: true,
            is_static: false,
            is_abstract: false,
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
            access: default_access(),
            nullness: MethodNullness::unknown(method_param_count(descriptor).expect("param count")),
            bytecode: vec![0],
            line_numbers: Vec::new(),
            cfg,
            calls: Vec::new(),
            string_literals: Vec::new(),
            exception_handlers: handlers,
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
    fn empty_catch_from_compiled_java() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "example/EmptyCatchSample.java".to_string(),
            contents: r#"
package example;

public class EmptyCatchSample {
    public void run() {
        try {
            throw new RuntimeException("boom");
        } catch (RuntimeException ex) {
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
}
