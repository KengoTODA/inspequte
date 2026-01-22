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

/// Rule that detects SLF4J log messages assembled manually instead of placeholders.
pub(crate) struct Slf4jManuallyProvidedMessageRule;

impl Rule for Slf4jManuallyProvidedMessageRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "SLF4J_MANUALLY_PROVIDED_MESSAGE",
            name: "SLF4J preformatted message",
            description: "SLF4J messages should use placeholders instead of manual formatting",
        }
    }

    fn run(&self, context: &AnalysisContext) -> Result<Vec<SarifResult>> {
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
    ManualMessage,
    IntConst { value: usize },
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
                stack.pop();
                stack.push(ValueKind::Unknown);
            }
            opcodes::AASTORE => {
                stack.pop();
                stack.pop();
                stack.pop();
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

                    if is_slf4j_logger_call(call) {
                        if let Some(format_index) = slf4j_format_index(&param_types) {
                            let format_arg = args
                                .get(format_index)
                                .copied()
                                .unwrap_or(ValueKind::Unknown);
                            if format_arg == ValueKind::ManualMessage {
                                let message = result_message(
                                    "SLF4J message is manually formatted; use placeholders",
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
                        ReturnKind::Primitive => stack.push(ValueKind::Unknown),
                        ReturnKind::Reference => {
                            if is_manual_message_call(call) {
                                stack.push(ValueKind::ManualMessage);
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

fn is_manual_message_call(call: &crate::ir::CallSite) -> bool {
    if call.owner == "java/lang/String" && call.name == "format" {
        return call.descriptor.ends_with(")Ljava/lang/String;");
    }
    if matches!(
        call.owner.as_str(),
        "java/lang/StringBuilder" | "java/lang/StringBuffer"
    ) && call.name == "toString"
        && call.descriptor == "()Ljava/lang/String;"
    {
        return true;
    }
    false
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
            .filter(|result| result.rule_id.as_deref() == Some("SLF4J_MANUALLY_PROVIDED_MESSAGE"))
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
    void debug(Marker marker, String format, Object... args);
    void debug(Marker marker, String msg, Throwable t);
}
"#
                .to_string(),
            },
            SourceFile {
                path: "com/example/Runner.java".to_string(),
                contents: contents.to_string(),
            },
        ]
    }

    #[test]
    fn slf4j_manually_provided_message_reports_manual_formatting() {
        let sources = slf4j_sources(
            r#"
package com.example;
import org.slf4j.Logger;
import org.slf4j.Marker;
public class Runner {
    private final Logger logger;
    private final Marker marker;
    public Runner(Logger logger, Marker marker) {
        this.logger = logger;
        this.marker = marker;
    }
    public void run(int value) {
        logger.info(new StringBuilder().append("value=").append(value).toString());
        logger.info(String.format("value=%s", value));
        logger.debug(marker, new StringBuilder().append("value=").append(value).toString());
    }
}
"#,
        );

        let messages = analyze_sources(sources);

        assert_eq!(messages.len(), 3);
    }

    #[test]
    fn slf4j_manually_provided_message_allows_placeholders_and_unknown() {
        let sources = slf4j_sources(
            r#"
package com.example;
import org.slf4j.Logger;
public class Runner {
    private final Logger logger;
    public Runner(Logger logger) {
        this.logger = logger;
    }
    public void run(String message, int value) {
        logger.info("value={}", value);
        logger.info(message);
    }
}
"#,
        );

        let messages = analyze_sources(sources);

        assert!(messages.is_empty());
    }
}
