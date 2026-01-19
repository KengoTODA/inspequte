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

/// Rule that detects SLF4J placeholder mismatches.
pub(crate) struct Slf4jPlaceHolderMismatchRule;

impl Rule for Slf4jPlaceHolderMismatchRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "SLF4J_PLACE_HOLDER_MISMATCH",
            name: "SLF4J placeholder mismatch",
            description: "SLF4J placeholder count mismatches argument count",
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
    StringLiteral(String),
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
                    stack.push(ValueKind::StringLiteral(value.clone()));
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
                        if let Some(result) = check_mismatch(
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

fn check_mismatch(
    class_name: &str,
    method: &Method,
    artifact_uri: Option<&str>,
    descriptor: &str,
    args: &[ValueKind],
    offset: u32,
) -> Result<Option<SarifResult>> {
    let descriptor = MethodDescriptor::from_str(descriptor).context("parse descriptor")?;
    let params = descriptor.parameter_types();
    if params.is_empty() {
        return Ok(None);
    }
    if !matches!(params[0], TypeDescriptor::Object(ref name) if name == "java/lang/String") {
        return Ok(None);
    }
    if matches_object_array(&params.get(1)) {
        return Ok(None);
    }
    let format = match args.first() {
        Some(ValueKind::StringLiteral(value)) => value,
        _ => return Ok(None),
    };
    let placeholder_count = count_placeholders(format);
    let arg_count = formatting_arg_count(&params)?;
    if placeholder_count == arg_count {
        return Ok(None);
    }
    let message = result_message(format!(
        "SLF4J format string expects {} placeholders but {} arguments are provided: {}.{}{}",
        placeholder_count, arg_count, class_name, method.name, method.descriptor
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

fn matches_object_array(param: &Option<&TypeDescriptor>) -> bool {
    let Some(param) = param else {
        return false;
    };
    matches!(
        param,
        TypeDescriptor::Array(inner, 1)
            if matches!(**inner, TypeDescriptor::Object(ref name) if name == "java/lang/Object")
    )
}

fn formatting_arg_count(params: &[TypeDescriptor]) -> Result<usize> {
    if params.len() <= 1 {
        return Ok(0);
    }
    let last_is_throwable = matches!(
        params.last(),
        Some(TypeDescriptor::Object(name)) if name == "java/lang/Throwable"
    );
    if last_is_throwable {
        return Ok(params.len() - 2);
    }
    Ok(params.len() - 1)
}

fn returns_reference(descriptor: &str) -> Result<bool> {
    let descriptor = MethodDescriptor::from_str(descriptor).context("parse descriptor")?;
    Ok(matches!(
        descriptor.return_type(),
        TypeDescriptor::Object(_) | TypeDescriptor::Array(_, _)
    ))
}

fn count_placeholders(message: &str) -> usize {
    let bytes = message.as_bytes();
    let mut count = 0;
    let mut index = 0;
    while index + 1 < bytes.len() {
        if bytes[index] == b'{' && bytes[index + 1] == b'}' {
            let mut backslashes = 0;
            let mut cursor = index;
            while cursor > 0 && bytes[cursor - 1] == b'\\' {
                backslashes += 1;
                cursor -= 1;
            }
            if backslashes % 2 == 0 {
                count += 1;
            }
            index += 2;
            continue;
        }
        index += 1;
    }
    count
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
    fn slf4j_placeholder_mismatch_detects_missing_args() {
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

    public void mismatch() {
        logger.info("Hello {} {}", "one");
    }

    public void match() {
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
            .filter(|result| result.rule_id.as_deref() == Some("SLF4J_PLACE_HOLDER_MISMATCH"))
            .filter_map(|result| result.message.text.as_deref())
            .collect();

        assert_eq!(1, messages.len());
        assert!(messages[0].contains("expects 2 placeholders"));
    }
}
