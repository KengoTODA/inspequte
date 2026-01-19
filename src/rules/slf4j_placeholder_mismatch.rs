use std::collections::BTreeMap;
use std::str::FromStr;

use anyhow::{Context, Result};
use jdescriptor::MethodDescriptor;
use serde_sarif::sarif::Result as SarifResult;

use crate::descriptor::{ReturnKind, method_return_kind};
use crate::engine::AnalysisContext;
use crate::ir::{CallKind, InstructionKind, Method};
use crate::opcodes;
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

/// Rule that detects SLF4J placeholder and argument count mismatches.
pub(crate) struct Slf4jPlaceholderMismatchRule;

impl Rule for Slf4jPlaceholderMismatchRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "SLF4J_PLACEHOLDER_MISMATCH",
            name: "SLF4J placeholder mismatch",
            description: "SLF4J placeholder count does not match arguments",
        }
    }

    fn run(&self, context: &AnalysisContext) -> Result<Vec<SarifResult>> {
        let mut results = Vec::new();
        for class in &context.classes {
            if !context.is_analysis_target_class(class) {
                continue;
            }
            for method in &class.methods {
                if method.bytecode.is_empty() {
                    continue;
                }
                let artifact_uri = context.class_artifact_uri(class);
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

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum ValueKind {
    Unknown,
    FormatString { placeholders: usize },
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
                if let Some(value) = const_strings.get(&(offset as u32)) {
                    stack.push(ValueKind::FormatString {
                        placeholders: count_placeholders(value),
                    });
                } else {
                    stack.push(ValueKind::Unknown);
                }
            }
            opcodes::DUP => {
                if let Some(value) = stack.last().copied() {
                    stack.push(value);
                }
            }
            opcodes::NEWARRAY | opcodes::ANEWARRAY => {
                let count = stack.pop().unwrap_or(ValueKind::Unknown);
                let len = match count {
                    ValueKind::IntConst { value } => Some(value),
                    _ => None,
                };
                stack.push(ValueKind::Array { len });
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
                        if let Some(mismatch) = placeholder_mismatch(&param_types, &args) {
                            let message = result_message(format!(
                                "SLF4J placeholder mismatch: expected {} argument(s) but found {}",
                                mismatch.expected, mismatch.found
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

/// Placeholder/argument count mismatch details.
struct PlaceholderMismatch {
    expected: usize,
    found: usize,
}

fn placeholder_mismatch(
    param_types: &[jdescriptor::TypeDescriptor],
    args: &[ValueKind],
) -> Option<PlaceholderMismatch> {
    if param_types.is_empty() || args.is_empty() {
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
    let format = match args
        .get(format_index)
        .copied()
        .unwrap_or(ValueKind::Unknown)
    {
        ValueKind::FormatString { placeholders } => placeholders,
        ValueKind::Unknown | ValueKind::IntConst { .. } | ValueKind::Array { .. } => return None,
    };

    if param_types.len() == format_index + 2 {
        if let jdescriptor::TypeDescriptor::Array(inner, _) = &param_types[format_index + 1] {
            if matches!(inner.as_ref(), jdescriptor::TypeDescriptor::Object(class) if class.as_str() == "java/lang/Object")
            {
                let arg_count = match args
                    .get(format_index + 1)
                    .copied()
                    .unwrap_or(ValueKind::Unknown)
                {
                    ValueKind::Array { len: Some(len) } => len,
                    _ => return None,
                };
                return if format == arg_count {
                    None
                } else {
                    Some(PlaceholderMismatch {
                        expected: format,
                        found: arg_count,
                    })
                };
            }
        }
    }

    let mut arg_count = param_types.len().saturating_sub(format_index + 1);
    if let Some(last_param) = param_types.last() {
        if matches!(last_param, jdescriptor::TypeDescriptor::Object(class) if class.as_str() == "java/lang/Throwable")
        {
            arg_count = arg_count.saturating_sub(1);
        }
    }

    if format == arg_count {
        None
    } else {
        Some(PlaceholderMismatch {
            expected: format,
            found: arg_count,
        })
    }
}

fn count_placeholders(text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut index = 0usize;
    let mut count = 0usize;
    while index + 1 < bytes.len() {
        if bytes[index] == b'{' && bytes[index + 1] == b'}' {
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
                count += 1;
            }
            index += 2;
        } else {
            index += 1;
        }
    }
    count
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
            .filter(|result| result.rule_id.as_deref() == Some("SLF4J_PLACEHOLDER_MISMATCH"))
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
    void debug(Marker marker, String format, Object arg1, Object arg2);
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
    fn slf4j_placeholder_mismatch_reports_missing_args() {
        let sources = slf4j_sources(
            r#"
package com.example;
import org.slf4j.Logger;
public class Runner {
    private final Logger logger;
    public Runner(Logger logger) {
        this.logger = logger;
    }
    public void run() {
        logger.info("Hello {} {}", "one");
    }
}
"#,
        );

        let messages = analyze_sources(sources);

        assert!(messages.iter().any(|msg| msg.contains("expected 2")));
    }

    #[test]
    fn slf4j_placeholder_mismatch_allows_matched_and_escaped() {
        let sources = slf4j_sources(
            r#"
package com.example;
import org.slf4j.Logger;
public class Runner {
    private final Logger logger;
    public Runner(Logger logger) {
        this.logger = logger;
    }
    public void run() {
        logger.info("Hello {}", "one");
        logger.info("Escaped \\{} text");
    }
}
"#,
        );

        let messages = analyze_sources(sources);

        assert!(messages.is_empty());
    }

    #[test]
    fn slf4j_placeholder_mismatch_handles_varargs() {
        let sources = slf4j_sources(
            r#"
package com.example;
import org.slf4j.Logger;
public class Runner {
    private final Logger logger;
    public Runner(Logger logger) {
        this.logger = logger;
    }
    public void run() {
        logger.info("Varargs {} {} {}", "one", "two", "three");
        logger.info("Mismatch {} {}", "one", "two", "three");
    }
}
"#,
        );

        let messages = analyze_sources(sources);

        assert_eq!(messages.len(), 1);
        assert!(messages.iter().any(|msg| msg.contains("expected 2")));
    }

    #[test]
    fn slf4j_placeholder_mismatch_handles_marker_varargs() {
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
    public void run() {
        logger.debug(marker, "Marker {} {}", "one", "two");
        logger.debug(marker, "Marker {} {}", "one", "two", "three");
    }
}
"#,
        );

        let messages = analyze_sources(sources);

        assert_eq!(messages.len(), 1);
        assert!(messages.iter().any(|msg| msg.contains("expected 2")));
    }

    #[test]
    fn slf4j_placeholder_mismatch_allows_marker_matched_and_throwable() {
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
    public void run() {
        logger.debug(marker, "Marker {} {}", "one", "two");
        logger.debug(marker, "Marker {}", "one", new RuntimeException("boom"));
    }
}
"#,
        );

        let messages = analyze_sources(sources);

        assert!(messages.is_empty());
    }
}
