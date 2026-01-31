use std::collections::BTreeMap;
use std::str::FromStr;

use anyhow::{Context, Result};
use jdescriptor::MethodDescriptor;
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::engine::AnalysisContext;
use crate::ir::{CallKind, InstructionKind, Method};
use crate::opcodes;
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

/// Rule that reports SLF4J format strings made only of placeholders or whitespace.
#[derive(Default)]
pub(crate) struct Slf4jSignOnlyFormatRule;

crate::register_rule!(Slf4jSignOnlyFormatRule);

impl Rule for Slf4jSignOnlyFormatRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "SLF4J_SIGN_ONLY_FORMAT",
            name: "SLF4J placeholder-only format",
            description: "SLF4J format strings should include descriptive text",
        }
    }

    fn run(&self, context: &AnalysisContext) -> Result<Vec<SarifResult>> {
        if !context.has_slf4j() {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();
        for class in &context.classes {
            if !context.is_analysis_target_class(class) {
                continue;
            }
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
    FormatString(String),
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

    let mut const_strings = BTreeMap::new();
    for block in &method.cfg.blocks {
        for inst in &block.instructions {
            if let InstructionKind::ConstString(value) = &inst.kind {
                const_strings.insert(inst.offset, value.clone());
            }
        }
    }

    let mut locals = initial_locals(method)?;
    let mut stack: Vec<ValueKind> = Vec::new();
    let mut offset = 0usize;
    while offset < method.bytecode.len() {
        let opcode = method.bytecode[offset];
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
                if let Some(value) = const_strings.get(&(offset as u32)) {
                    stack.push(ValueKind::FormatString(value.clone()));
                } else {
                    stack.push(ValueKind::Unknown);
                }
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

                    if is_slf4j_logger_call(call) {
                        if let Some(format_index) = slf4j_format_index(&param_types) {
                            if is_message_only_overload(&param_types, format_index) {
                                // Message-only overloads are allowed.
                            } else if let Some(ValueKind::FormatString(text)) =
                                args.get(format_index).cloned()
                            {
                                if is_sign_only_format(&text) {
                                    let message = result_message(
                                        "SLF4J format should include text, not only placeholders",
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
                    }

                    stack.push(ValueKind::Unknown);
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

fn slf4j_format_index(param_types: &[jdescriptor::TypeDescriptor]) -> Option<usize> {
    if param_types.is_empty() {
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
    Some(format_index)
}

fn is_message_only_overload(
    param_types: &[jdescriptor::TypeDescriptor],
    format_index: usize,
) -> bool {
    if param_types.len() == format_index + 1 {
        return true;
    }
    if param_types.len() == format_index + 2 {
        if let Some(last_param) = param_types.last() {
            return matches!(last_param, jdescriptor::TypeDescriptor::Object(class) if class.as_str() == "java/lang/Throwable");
        }
    }
    false
}

fn is_sign_only_format(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index].is_ascii_whitespace() {
            index += 1;
            continue;
        }
        if bytes[index] == b'{' && index + 1 < bytes.len() && bytes[index + 1] == b'}' {
            let mut backslashes = 0usize;
            let mut lookback = index;
            while lookback > 0 {
                lookback -= 1;
                if bytes[lookback] == b'\\' {
                    backslashes += 1;
                } else {
                    break;
                }
            }
            if backslashes % 2 == 0 {
                index += 2;
                continue;
            }
        }
        return false;
    }
    true
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
            .filter(|result| result.rule_id.as_deref() == Some("SLF4J_SIGN_ONLY_FORMAT"))
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
    void info(String msg);
    void info(String format, Object arg);
    void info(String format, Object arg1, Object arg2);
    void info(String format, Object... args);
    void info(String msg, Throwable t);
    void debug(Marker marker, String msg);
    void debug(Marker marker, String format, Object arg);
    void debug(Marker marker, String format, Object... args);
    void debug(Marker marker, String msg, Throwable t);
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
    fn slf4j_sign_only_format_reports_placeholders_only() {
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
    public void methodOne(String varThree) {
        fieldA.info("{}", varThree);
        fieldA.info("{} {}", varThree, varThree);
        fieldA.debug(fieldB, "{}", varThree);
    }
}
"#,
        );

        let messages = analyze_sources(sources);

        assert_eq!(messages.len(), 3);
    }

    #[test]
    fn slf4j_sign_only_format_allows_text() {
        let sources = slf4j_sources(
            r#"
package com.example;
import org.slf4j.Logger;
public class ClassA {
    private final Logger fieldA;
    public ClassA(Logger varOne) {
        this.fieldA = varOne;
    }
    public void methodOne(String varTwo) {
        fieldA.info("value={}", varTwo);
        fieldA.info("{} value", varTwo);
        fieldA.info("\\{}", varTwo);
    }
}
"#,
        );

        let messages = analyze_sources(sources);

        assert!(messages.is_empty());
    }
}
