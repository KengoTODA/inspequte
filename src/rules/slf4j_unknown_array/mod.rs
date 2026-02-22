use std::collections::BTreeMap;
use std::str::FromStr;

use anyhow::{Context, Result};
use jdescriptor::MethodDescriptor;
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::descriptor::{ReturnKind, method_return_kind};
use crate::engine::AnalysisContext;
use crate::ir::{CallKind, Method};
use crate::opcodes;
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

/// Rule that reports SLF4J varargs calls with unknown argument array length.
#[derive(Default)]
pub(crate) struct Slf4jUnknownArrayRule;

crate::register_rule!(Slf4jUnknownArrayRule);

impl Rule for Slf4jUnknownArrayRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "SLF4J_UNKNOWN_ARRAY",
            name: "SLF4J unknown array",
            description: "SLF4J varargs calls with unknown argument arrays",
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

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum ValueKind {
    Unknown,
    IntConst { value: usize },
    Array { len: Option<usize> },
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

    let mut locals = initial_locals(method)?;
    let mut stack: Vec<ValueKind> = Vec::new();
    let mut offset = 0usize;
    while offset < method.bytecode.len() {
        let opcode = method.bytecode[offset];
        match opcode {
            opcodes::ACONST_NULL => stack.push(ValueKind::Unknown),
            opcodes::ICONST_M1 => stack.push(ValueKind::Unknown),
            opcodes::ICONST_0
            | opcodes::ICONST_1
            | opcodes::ICONST_2
            | opcodes::ICONST_3
            | opcodes::ICONST_4
            | opcodes::ICONST_5 => {
                let value = (opcode - opcodes::ICONST_0) as usize;
                stack.push(ValueKind::IntConst { value });
            }
            opcodes::BIPUSH => {
                let value = method.bytecode.get(offset + 1).copied().unwrap_or(0) as i8 as i32;
                if value >= 0 {
                    stack.push(ValueKind::IntConst {
                        value: value as usize,
                    });
                } else {
                    stack.push(ValueKind::Unknown);
                }
            }
            opcodes::SIPUSH => {
                let high = method.bytecode.get(offset + 1).copied().unwrap_or(0);
                let low = method.bytecode.get(offset + 2).copied().unwrap_or(0);
                let value = i16::from_be_bytes([high, low]) as i32;
                if value >= 0 {
                    stack.push(ValueKind::IntConst {
                        value: value as usize,
                    });
                } else {
                    stack.push(ValueKind::Unknown);
                }
            }
            opcodes::ALOAD => {
                let index = method.bytecode.get(offset + 1).copied().unwrap_or(0) as usize;
                ensure_local(&mut locals, index);
                stack.push(locals[index]);
            }
            opcodes::ALOAD_0 | opcodes::ALOAD_1 | opcodes::ALOAD_2 | opcodes::ALOAD_3 => {
                let index = (opcode - opcodes::ALOAD_0) as usize;
                ensure_local(&mut locals, index);
                stack.push(locals[index]);
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
                stack.push(ValueKind::Unknown);
            }
            opcodes::NEWARRAY | opcodes::ANEWARRAY => {
                let count = stack.pop().unwrap_or(ValueKind::Unknown);
                let len = match count {
                    ValueKind::IntConst { value } => Some(value),
                    _ => None,
                };
                stack.push(ValueKind::Array { len });
            }
            opcodes::DUP => {
                if let Some(value) = stack.last().copied() {
                    stack.push(value);
                }
            }
            opcodes::AASTORE => {
                stack.pop();
                stack.pop();
                stack.pop();
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

                    if is_slf4j_logger_call(call) {
                        if let Some(array_index) = slf4j_varargs_array_index(param_types) {
                            let array_arg =
                                args.get(array_index).copied().unwrap_or(ValueKind::Unknown);
                            let is_known = matches!(array_arg, ValueKind::Array { len: Some(_) });
                            if !is_known {
                                let message = result_message(
                                    "SLF4J varargs argument array length is unknown",
                                );
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
                        ReturnKind::Primitive | ReturnKind::Reference => {
                            stack.push(ValueKind::Unknown);
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

fn is_slf4j_logger_call(call: &crate::ir::CallSite) -> bool {
    if call.owner != "org/slf4j/Logger" {
        return false;
    }
    matches!(
        call.name.as_str(),
        "trace" | "debug" | "info" | "warn" | "error"
    )
}

fn slf4j_varargs_array_index(param_types: &[jdescriptor::TypeDescriptor]) -> Option<usize> {
    if param_types.len() < 2 {
        return None;
    }
    let mut format_index = 0usize;
    if let Some(first_param) = param_types.first() {
        if matches!(first_param, jdescriptor::TypeDescriptor::Object(class) if class.as_str() == "org/slf4j/Marker")
        {
            format_index = 1;
        }
    }
    let format_param = param_types.get(format_index)?;
    let is_string = matches!(format_param, jdescriptor::TypeDescriptor::Object(class) if class.as_str() == "java/lang/String");
    if !is_string {
        return None;
    }
    if param_types.len() != format_index + 2 {
        return None;
    }
    let array_param = param_types.get(format_index + 1)?;
    if let jdescriptor::TypeDescriptor::Array(inner, _) = array_param {
        if matches!(inner.as_ref(), jdescriptor::TypeDescriptor::Object(class) if class.as_str() == "java/lang/Object")
        {
            return Some(format_index + 1);
        }
    }
    None
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
            .filter(|result| result.rule_id.as_deref() == Some("SLF4J_UNKNOWN_ARRAY"))
            .filter_map(|result| result.message.text.clone())
            .collect()
    }

    fn slf4j_sources(contents: &str) -> Vec<SourceFile> {
        vec![
            SourceFile {
                path: "org/slf4j/Marker.java".to_string(),
                contents: r#"
package org.slf4j;
public interface Marker {}
"#
                .to_string(),
            },
            SourceFile {
                path: "org/slf4j/Logger.java".to_string(),
                contents: r#"
package org.slf4j;
public interface Logger {
    void info(String format, Object... args);
    void debug(Marker marker, String format, Object... args);
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
    fn slf4j_unknown_array_reports_unknown_arrays() {
        let sources = slf4j_sources(
            r#"
package com.example;
import org.slf4j.Logger;
import org.slf4j.Marker;
public class ClassA {
    private final Logger fieldA;
    private final Marker fieldB;
    public ClassA(Logger varOne, Marker varTwo) {
        this.fieldA = varOne;
        this.fieldB = varTwo;
    }
    public void methodOne(Object[] varThree) {
        fieldA.info("{} {}", varThree);
        fieldA.debug(fieldB, "{}", varThree);
    }
}
"#,
        );

        let messages = analyze_sources(sources);

        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn slf4j_unknown_array_allows_known_arrays() {
        let sources = slf4j_sources(
            r#"
package com.example;
import org.slf4j.Logger;
public class ClassA {
    private final Logger fieldA;
    public ClassA(Logger varOne) {
        this.fieldA = varOne;
    }
    public void methodOne(Object varTwo) {
        fieldA.info("{} {}", new Object[]{varTwo, varTwo});
        fieldA.info("{} {}", "a", "b");
    }
}
"#,
        );

        let messages = analyze_sources(sources);

        assert!(messages.is_empty());
    }
}
