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

/// Rule that ensures Log4j2 format strings are compile-time constants.
#[derive(Default)]
pub(crate) struct Log4j2FormatShouldBeConstRule;

crate::register_rule!(Log4j2FormatShouldBeConstRule);

impl Rule for Log4j2FormatShouldBeConstRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "LOG4J2_FORMAT_SHOULD_BE_CONST",
            name: "Log4j2 format should be const",
            description: "Log4j2 format strings should be compile-time constants",
        }
    }

    fn run(&self, context: &AnalysisContext) -> Result<Vec<SarifResult>> {
        if !context.has_log4j2() {
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

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum ValueKind {
    Unknown,
    ConstString,
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
                if const_strings.get(&(offset as u32)).is_some() {
                    stack.push(ValueKind::ConstString);
                } else {
                    stack.push(ValueKind::Unknown);
                }
            }
            opcodes::DUP => {
                if let Some(value) = stack.last().copied() {
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

                    if is_log4j2_logger_call(call) {
                        if let Some(format_index) = format_index(&param_types) {
                            let format_arg = args
                                .get(format_index)
                                .copied()
                                .unwrap_or(ValueKind::Unknown);
                            if format_arg != ValueKind::ConstString {
                                let message =
                                    result_message("Log4j2 format string should be constant");
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

fn is_log4j2_logger_call(call: &crate::ir::CallSite) -> bool {
    if call.owner != "org/apache/logging/log4j/Logger" {
        return false;
    }
    matches!(
        call.name.as_str(),
        "trace" | "debug" | "info" | "warn" | "error"
    )
}

fn format_index(param_types: &[jdescriptor::TypeDescriptor]) -> Option<usize> {
    if param_types.is_empty() {
        return None;
    }
    let mut index = 0usize;
    if let Some(first_param) = param_types.first() {
        if matches!(first_param, jdescriptor::TypeDescriptor::Object(class) if class.as_str() == "org/apache/logging/log4j/Marker")
        {
            index = 1;
        }
    }
    let format_param = param_types.get(index)?;
    let is_string = matches!(
        format_param,
        jdescriptor::TypeDescriptor::Object(class) if class.as_str() == "java/lang/String"
    );
    if !is_string {
        return None;
    }
    if param_types.len() <= index + 1 {
        return None;
    }
    if param_types.len() == index + 2 {
        if let Some(last_param) = param_types.last() {
            if matches!(last_param, jdescriptor::TypeDescriptor::Object(class) if class.as_str() == "java/lang/Throwable")
            {
                return None;
            }
        }
    }
    Some(index)
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
            .filter(|result| result.rule_id.as_deref() == Some("LOG4J2_FORMAT_SHOULD_BE_CONST"))
            .filter_map(|result| result.message.text.clone())
            .collect()
    }

    fn log4j2_sources(contents: &str) -> Vec<SourceFile> {
        vec![
            SourceFile {
                path: "org/apache/logging/log4j/Marker.java".to_string(),
                contents: r#"
package org.apache.logging.log4j;
public interface Marker {}
"#
                .to_string(),
            },
            SourceFile {
                path: "org/apache/logging/log4j/message/Message.java".to_string(),
                contents: r#"
package org.apache.logging.log4j.message;
public interface Message {}
"#
                .to_string(),
            },
            SourceFile {
                path: "org/apache/logging/log4j/Logger.java".to_string(),
                contents: r#"
package org.apache.logging.log4j;
import org.apache.logging.log4j.message.Message;
public interface Logger {
    void info(CharSequence message);
    void info(Object message);
    void info(String message);
    void info(String format, Object arg);
    void info(String format, Object p0, Object p1);
    void info(String format, Object... args);
    void info(String message, Throwable t);
    void info(java.util.function.Supplier<?> supplier);
    void info(java.util.function.Supplier<?> supplier, Throwable t);
    void info(Message message);
    void debug(Marker marker, String message);
    void debug(Marker marker, String format, Object... args);
    void debug(Marker marker, String message, Throwable t);
    void debug(Marker marker, Message message);
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
    fn log4j2_format_should_be_const_reports_non_const() {
        let sources = log4j2_sources(
            r#"
package com.example;
import org.apache.logging.log4j.Logger;
public class ClassA {
    private final Logger fieldA;
    public ClassA(Logger varOne) {
        this.fieldA = varOne;
    }
    public void methodOne(String varTwo) {
        fieldA.info("Hello {}", varTwo);
        fieldA.info("Hello {} {}", varTwo, varTwo);
        fieldA.info("Hello " + varTwo, varTwo);
        fieldA.info("Hello " + varTwo, varTwo, varTwo);
    }
}
"#,
        );

        let messages = analyze_sources(sources);

        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn log4j2_format_should_be_const_handles_marker_and_message_only() {
        let sources = log4j2_sources(
            r#"
package com.example;
import org.apache.logging.log4j.Logger;
import org.apache.logging.log4j.Marker;
import org.apache.logging.log4j.message.Message;
public class ClassA {
    private final Logger fieldA;
    private final Marker fieldB;
    private final Message fieldC;
    public ClassA(Logger varOne, Marker varTwo, Message varThree) {
        this.fieldA = varOne;
        this.fieldB = varTwo;
        this.fieldC = varThree;
    }
    public void methodOne(String varFour) {
        fieldA.debug(fieldB, "Hello {}", varFour);
        fieldA.debug(fieldB, "Hello " + varFour, varFour);
        fieldA.info(new StringBuilder("prefix ").append(varFour));
        fieldA.info((Object) ("value=" + varFour));
        fieldA.info(() -> "lazy " + varFour);
        fieldA.info(() -> "lazy " + varFour, new RuntimeException("boom"));
        fieldA.info(fieldC);
        fieldA.debug(fieldB, fieldC);
        fieldA.info("prefix " + varFour);
        fieldA.info("oops", new RuntimeException("boom"));
    }
}
"#,
        );

        let messages = analyze_sources(sources);

        // Only format overloads are checked; message-only overloads are ignored here.
        assert_eq!(messages.len(), 1);
    }
}
