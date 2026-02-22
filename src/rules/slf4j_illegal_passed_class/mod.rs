use std::collections::BTreeMap;
use std::str::FromStr;

use anyhow::{Context, Result};
use jdescriptor::MethodDescriptor;
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::descriptor::{ReturnKind, method_return_kind};
use crate::engine::AnalysisContext;
use crate::ir::{CallKind, InstructionKind, Method};
use crate::opcodes;
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

/// Rule that checks illegal classes passed to LoggerFactory.getLogger(Class).
#[derive(Default)]
pub(crate) struct Slf4jIllegalPassedClassRule;

crate::register_rule!(Slf4jIllegalPassedClassRule);

impl Rule for Slf4jIllegalPassedClassRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "SLF4J_ILLEGAL_PASSED_CLASS",
            name: "SLF4J illegal passed class",
            description: "LoggerFactory.getLogger should be called with the caller class",
        }
    }

    fn run(&self, context: &AnalysisContext) -> Result<Vec<SarifResult>> {
        if !context.has_slf4j() {
            return Ok(Vec::new());
        }

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
                        if method.bytecode.is_empty() {
                            continue;
                        }
                        let artifact_uri = context.class_artifact_uri(class);
                        class_results.extend(analyze_method(
                            &class.name,
                            method,
                            artifact_uri.as_deref(),
                        )?);
                    }
                    Ok(class_results)
                })?;
            results.extend(class_results);
        }
        Ok(results)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ValueKind {
    Unknown,
    ClassLiteral(String),
    GetClass,
}

fn analyze_method(
    class_name: &str,
    method: &Method,
    artifact_uri: Option<&str>,
) -> Result<Vec<SarifResult>> {
    let mut results = Vec::new();
    let mut callsites = BTreeMap::new();
    for call in &method.calls {
        callsites.insert(call.offset, call);
    }

    let mut instructions = Vec::new();
    for block in &method.cfg.blocks {
        for inst in &block.instructions {
            instructions.push(inst);
        }
    }
    instructions.sort_by_key(|inst| inst.offset);
    let mut instruction_index = 0usize;

    let mut locals = initial_locals(method)?;
    let mut stack: Vec<ValueKind> = Vec::new();
    let mut expect_reified_class_literal = false;
    let mut offset = 0usize;
    while offset < method.bytecode.len() {
        let opcode = method.bytecode[offset];
        if expect_reified_class_literal
            && !matches!(opcode, opcodes::LDC | opcodes::LDC_W | opcodes::LDC2_W)
        {
            expect_reified_class_literal = false;
        }
        while instruction_index < instructions.len()
            && instructions[instruction_index].offset < offset as u32
        {
            instruction_index += 1;
        }
        let instruction_kind = instructions
            .get(instruction_index)
            .filter(|inst| inst.offset == offset as u32)
            .map(|inst| &inst.kind);

        match opcode {
            opcodes::ACONST_NULL => stack.push(ValueKind::Unknown),
            opcodes::ALOAD => {
                let index = method.bytecode.get(offset + 1).copied().unwrap_or(0) as usize;
                ensure_local(&mut locals, index);
                stack.push(locals[index].clone());
            }
            opcodes::ALOAD_0 | opcodes::ALOAD_1 | opcodes::ALOAD_2 | opcodes::ALOAD_3 => {
                let index = (opcode - opcodes::ALOAD_0) as usize;
                ensure_local(&mut locals, index);
                stack.push(locals[index].clone());
            }
            opcodes::ASTORE => {
                let index = method.bytecode.get(offset + 1).copied().unwrap_or(0) as usize;
                ensure_local(&mut locals, index);
                let value = stack.pop().unwrap_or(ValueKind::Unknown);
                locals[index] = value;
            }
            opcodes::ASTORE_0 | opcodes::ASTORE_1 | opcodes::ASTORE_2 | opcodes::ASTORE_3 => {
                let index = (opcode - opcodes::ASTORE_0) as usize;
                ensure_local(&mut locals, index);
                let value = stack.pop().unwrap_or(ValueKind::Unknown);
                locals[index] = value;
            }
            opcodes::LDC | opcodes::LDC_W | opcodes::LDC2_W => {
                let value = match instruction_kind {
                    Some(InstructionKind::ConstClass(value))
                        if expect_reified_class_literal && value == "java/lang/Object" =>
                    {
                        ValueKind::Unknown
                    }
                    Some(InstructionKind::ConstClass(value)) => {
                        ValueKind::ClassLiteral(value.clone())
                    }
                    _ => ValueKind::Unknown,
                };
                expect_reified_class_literal = false;
                stack.push(value);
            }
            opcodes::DUP => {
                if let Some(value) = stack.last().cloned() {
                    stack.push(value);
                }
            }
            opcodes::POP => {
                stack.pop();
            }
            opcodes::INVOKEVIRTUAL
            | opcodes::INVOKEINTERFACE
            | opcodes::INVOKESPECIAL
            | opcodes::INVOKESTATIC => {
                if let Some(call) = callsites.get(&(offset as u32)) {
                    let descriptor = MethodDescriptor::from_str(&call.descriptor)
                        .context("parse call descriptor")?;
                    let param_types = descriptor.parameter_types();
                    let mut args = Vec::with_capacity(param_types.len());
                    for _ in 0..param_types.len() {
                        args.push(stack.pop().unwrap_or(ValueKind::Unknown));
                    }
                    args.reverse();
                    if call.kind != CallKind::Static {
                        stack.pop();
                    }
                    expect_reified_class_literal = is_kotlin_reified_operation_marker(call);

                    if is_get_logger_call(call, param_types) {
                        let arg = args.first().cloned().unwrap_or(ValueKind::Unknown);
                        if let ValueKind::ClassLiteral(passed_class) = arg {
                            if !is_acceptable_class(class_name, &passed_class) {
                                let message = result_message(format!(
                                    "Illegal class passed to LoggerFactory.getLogger: {}",
                                    passed_class
                                ));
                                let line = method.line_for_offset(offset as u32);
                                let location = method_location_with_line(
                                    class_name,
                                    &method.name,
                                    &method.descriptor,
                                    artifact_uri,
                                    line,
                                );
                                results.push(
                                    SarifResult::builder()
                                        .message(message)
                                        .locations(vec![location])
                                        .build(),
                                );
                            }
                        }
                    }

                    match method_return_kind(&call.descriptor)? {
                        ReturnKind::Void => {}
                        ReturnKind::Primitive => stack.push(ValueKind::Unknown),
                        ReturnKind::Reference => {
                            if is_get_class_call(call) {
                                stack.push(ValueKind::GetClass);
                            } else {
                                stack.push(ValueKind::Unknown);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        let length = crate::scan::opcode_length(&method.bytecode, offset)?;
        offset += length;
    }

    Ok(results)
}

fn initial_locals(method: &Method) -> Result<Vec<ValueKind>> {
    let mut locals = Vec::new();
    if !method.access.is_static {
        locals.push(ValueKind::Unknown);
    }
    let descriptor =
        MethodDescriptor::from_str(&method.descriptor).context("parse method descriptor")?;
    for _ in descriptor.parameter_types() {
        locals.push(ValueKind::Unknown);
    }
    Ok(locals)
}

fn ensure_local(locals: &mut Vec<ValueKind>, index: usize) {
    if index >= locals.len() {
        locals.resize(index + 1, ValueKind::Unknown);
    }
}

fn is_get_logger_call(
    call: &crate::ir::CallSite,
    param_types: &[jdescriptor::TypeDescriptor],
) -> bool {
    if call.owner != "org/slf4j/LoggerFactory" || call.name != "getLogger" {
        return false;
    }
    if param_types.len() != 1 {
        return false;
    }
    matches!(
        param_types[0],
        jdescriptor::TypeDescriptor::Object(ref class)
            if class.as_str() == "java/lang/Class"
    )
}

fn is_get_class_call(call: &crate::ir::CallSite) -> bool {
    call.owner == "java/lang/Object"
        && call.name == "getClass"
        && call.descriptor == "()Ljava/lang/Class;"
}

fn is_kotlin_reified_operation_marker(call: &crate::ir::CallSite) -> bool {
    call.owner == "kotlin/jvm/internal/Intrinsics"
        && call.name == "reifiedOperationMarker"
        && call.descriptor == "(ILjava/lang/String;)V"
}

/// Returns true when the passed class matches the caller or any outer class.
fn is_acceptable_class(caller_class: &str, passed_class: &str) -> bool {
    let mut current = caller_class;
    loop {
        if current == passed_class {
            return true;
        }
        if let Some(index) = current.rfind('$') {
            current = &current[..index];
        } else {
            break;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use crate::test_harness::{JvmTestHarness, Language, SourceFile};

    fn analyze_sources(language: Language, sources: Vec<SourceFile>) -> Vec<String> {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let output = harness
            .compile_and_analyze(language, &sources, &[])
            .expect("run harness analysis");
        output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("SLF4J_ILLEGAL_PASSED_CLASS"))
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
                path: "org/slf4j/LoggerFactory.java".to_string(),
                contents: r#"
package org.slf4j;
public class LoggerFactory {
    public static Logger getLogger(Class<?> clazz) {
        return null;
    }
}
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
    fn slf4j_illegal_passed_class_reports_illegal_literals() {
        let sources = slf4j_sources(
            r#"
package com.example;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
class ClassB {}
public class ClassA {
    private final Logger fieldA = LoggerFactory.getLogger(ClassB.class);
}
"#,
        );

        let messages = analyze_sources(Language::Java, sources);

        assert_eq!(messages.len(), 1);
        assert!(messages.iter().any(|msg| msg.contains("ClassB")));
    }

    #[test]
    fn slf4j_illegal_passed_class_allows_expected_classes() {
        let sources = slf4j_sources(
            r#"
package com.example;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
public class ClassA {
    private final Logger fieldA = LoggerFactory.getLogger(ClassA.class);
    public Logger methodOne() {
        return LoggerFactory.getLogger(getClass());
    }
    static class ClassB {
        private final Logger fieldA = LoggerFactory.getLogger(ClassB.class);
        private final Logger fieldB = LoggerFactory.getLogger(ClassA.class);
    }
}
"#,
        );

        let messages = analyze_sources(Language::Java, sources);

        assert!(messages.is_empty());
    }

    #[test]
    fn slf4j_illegal_passed_class_ignores_kotlin_reified_extension() {
        let sources = vec![
            SourceFile {
                path: "org/slf4j/Logger.kt".to_string(),
                contents: r#"
package org.slf4j
interface Logger
"#
                .to_string(),
            },
            SourceFile {
                path: "org/slf4j/LoggerFactory.kt".to_string(),
                contents: r#"
package org.slf4j
object LoggerFactory {
    @JvmStatic
    fun getLogger(clazz: Class<*>): Logger = object : Logger {}
}
"#
                .to_string(),
            },
            SourceFile {
                path: "com/example/ClassA.kt".to_string(),
                contents: r#"
package com.example

import org.slf4j.Logger
import org.slf4j.LoggerFactory

inline fun <reified T : Any> T.logger(): Logger = LoggerFactory.getLogger(T::class.java)

class ClassA {
    val varOne: Logger = logger()
}
"#
                .to_string(),
            },
        ];

        let messages = analyze_sources(Language::Kotlin, sources);

        assert!(messages.is_empty());
    }
}
