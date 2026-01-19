use std::collections::BTreeMap;
use std::str::FromStr;

use anyhow::{Context, Result};
use jdescriptor::{MethodDescriptor, TypeDescriptor};
use serde_sarif::sarif::Result as SarifResult;

use crate::descriptor::method_param_count;
use crate::engine::AnalysisContext;
use crate::ir::Method;
use crate::opcodes;
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

/// Rule that detects non-constant SLF4J format strings.
pub(crate) struct Slf4jFormatShouldBeConstRule;

impl Rule for Slf4jFormatShouldBeConstRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "SLF4J_FORMAT_SHOULD_BE_CONST",
            name: "SLF4J format should be constant",
            description: "SLF4J format string should be a constant literal",
        }
    }

    fn run(&self, context: &AnalysisContext) -> Result<Vec<SarifResult>> {
        let mut results = Vec::new();
        for class in &context.classes {
            if !context.is_analysis_target_class(class) {
                continue;
            }
            let artifact_uri = context.class_artifact_uri(class);
            for method in &class.methods {
                if method.bytecode.is_empty() {
                    continue;
                }
                results.extend(analyze_method(
                    &class.name,
                    method,
                    artifact_uri.as_deref(),
                )?);
            }
        }
        Ok(results)
    }
}

#[derive(Clone, Debug)]
enum ValueKind {
    Unknown,
    StringLiteral,
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
        for instruction in &block.instructions {
            if let crate::ir::InstructionKind::ConstString(value) = &instruction.kind {
                const_strings.insert(instruction.offset, value.clone());
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
            opcodes::NEW => stack.push(ValueKind::Unknown),
            opcodes::LDC | opcodes::LDC_W | opcodes::LDC2_W => {
                if let Some(value) = const_strings.get(&(offset as u32)) {
                    let _ = value;
                    stack.push(ValueKind::StringLiteral);
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
                    let arg_count = method_param_count(&call.descriptor)?;
                    let mut args_rev = Vec::new();
                    for _ in 0..arg_count {
                        args_rev.push(stack.pop().unwrap_or(ValueKind::Unknown));
                    }
                    let args: Vec<ValueKind> = args_rev.into_iter().rev().collect();
                    if opcode != opcodes::INVOKESTATIC {
                        stack.pop();
                    }
                    if is_slf4j_logger_method(call.owner.as_str(), call.name.as_str()) {
                        if let Some(result) = check_format_const(
                            class_name,
                            method,
                            artifact_uri,
                            call.descriptor.as_str(),
                            &args,
                            offset as u32,
                        )? {
                            results.push(result);
                        }
                    }
                    if returns_reference(&call.descriptor)? {
                        stack.push(ValueKind::Unknown);
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

fn check_format_const(
    class_name: &str,
    method: &Method,
    artifact_uri: Option<&str>,
    descriptor: &str,
    args: &[ValueKind],
    offset: u32,
) -> Result<Option<SarifResult>> {
    let descriptor = MethodDescriptor::from_str(descriptor).context("parse descriptor")?;
    let params = descriptor.parameter_types();
    if params.len() < 2 {
        return Ok(None);
    }
    if !matches!(params[0], TypeDescriptor::Object(ref name) if name == "java/lang/String") {
        return Ok(None);
    }
    if is_throwable_only_overload(&params) {
        return Ok(None);
    }
    if matches!(args.first(), Some(ValueKind::StringLiteral)) {
        return Ok(None);
    }
    let message = result_message(format!(
        "SLF4J format string should be a constant literal: {}.{}{}",
        class_name, method.name, method.descriptor
    ));
    let line = method.line_for_offset(offset);
    let location = method_location_with_line(
        class_name,
        &method.name,
        &method.descriptor,
        artifact_uri,
        line,
    );
    Ok(Some(
        SarifResult::builder()
            .message(message)
            .locations(vec![location])
            .build(),
    ))
}

fn is_slf4j_logger_method(owner: &str, name: &str) -> bool {
    owner == "org/slf4j/Logger" && matches!(name, "trace" | "debug" | "info" | "warn" | "error")
}

fn is_throwable_only_overload(params: &[TypeDescriptor]) -> bool {
    params.len() == 2
        && matches!(
            params[1],
            TypeDescriptor::Object(ref name) if name == "java/lang/Throwable"
        )
}

fn returns_reference(descriptor: &str) -> Result<bool> {
    let descriptor = MethodDescriptor::from_str(descriptor).context("parse descriptor")?;
    Ok(matches!(
        descriptor.return_type(),
        TypeDescriptor::Object(_) | TypeDescriptor::Array(_, _)
    ))
}

fn initial_locals(method: &Method) -> Result<Vec<ValueKind>> {
    let mut locals = Vec::new();
    if !method.access.is_static {
        locals.push(ValueKind::Unknown);
    }
    let descriptor =
        MethodDescriptor::from_str(&method.descriptor).context("parse method descriptor")?;
    for param in descriptor.parameter_types() {
        locals.push(ValueKind::Unknown);
        if matches!(param, TypeDescriptor::Long | TypeDescriptor::Double) {
            locals.push(ValueKind::Unknown);
        }
    }
    Ok(locals)
}

fn ensure_local(locals: &mut Vec<ValueKind>, index: usize) {
    if locals.len() <= index {
        locals.resize(index + 1, ValueKind::Unknown);
    }
}

#[cfg(test)]
mod tests {
    use crate::test_harness::{JvmTestHarness, Language, SourceFile};

    #[test]
    fn slf4j_format_should_be_const_reports_non_const_format() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![
            SourceFile {
                path: "org/slf4j/Logger.java".to_string(),
                contents: r#"
package org.slf4j;

public interface Logger {
    void info(String msg);
    void info(String format, Object arg);
    void info(String format, Object arg1, Object arg2);
    void info(String msg, Throwable t);
}
"#
                .to_string(),
            },
            SourceFile {
                path: "example/Sample.java".to_string(),
                contents: r#"
package example;

import org.slf4j.Logger;

public class Sample {
    private final Logger logger;

    public Sample(Logger logger) {
        this.logger = logger;
    }

    public void nonConst() {
        String format = System.getProperty("fmt");
        logger.info(format, "one");
    }

    public void constFormat() {
        logger.info("Hello {}", "one");
    }

    public void throwableOnly() {
        logger.info("Boom", new RuntimeException("boom"));
    }
}
"#
                .to_string(),
            },
        ];

        let analysis = harness
            .compile_and_analyze(Language::Java, &sources, &[])
            .expect("compile and analyze");

        let messages: Vec<_> = analysis
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("SLF4J_FORMAT_SHOULD_BE_CONST"))
            .filter_map(|result| result.message.text.as_deref())
            .collect();

        assert_eq!(1, messages.len());
        assert!(messages[0].contains("format string should be a constant literal"));
    }
}
