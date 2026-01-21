use std::collections::BTreeMap;
use std::str::FromStr;

use anyhow::{Context, Result};
use jdescriptor::{MethodDescriptor, TypeDescriptor};
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::descriptor::method_param_count;
use crate::engine::AnalysisContext;
use crate::ir::Method;
use crate::opcodes;
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

/// Rule that flags array comparisons using == or equals().
pub(crate) struct ArrayEqualsRule;

impl Rule for ArrayEqualsRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "ARRAY_EQUALS",
            name: "Array equals",
            description: "Array comparisons using == or equals()",
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
    Array,
    NonArray,
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
            opcodes::NEWARRAY | opcodes::ANEWARRAY => {
                stack.pop();
                stack.push(ValueKind::Array);
            }
            opcodes::MULTIANEWARRAY => {
                let dims = method.bytecode.get(offset + 3).copied().unwrap_or(0);
                for _ in 0..dims {
                    stack.pop();
                }
                stack.push(ValueKind::Array);
            }
            opcodes::NEW => {
                stack.push(ValueKind::NonArray);
            }
            opcodes::LDC | opcodes::LDC_W | opcodes::LDC2_W => {
                stack.push(ValueKind::NonArray);
            }
            opcodes::DUP => {
                if let Some(value) = stack.last().copied() {
                    stack.push(value);
                }
            }
            opcodes::POP => {
                stack.pop();
            }
            opcodes::IF_ACMPEQ | opcodes::IF_ACMPNE => {
                let right = stack.pop().unwrap_or(ValueKind::Unknown);
                let left = stack.pop().unwrap_or(ValueKind::Unknown);
                if left == ValueKind::Array && right == ValueKind::Array {
                    let message = result_message(format!(
                        "Array comparison uses reference equality: {}.{}{}",
                        class_name, method.name, method.descriptor
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
            opcodes::INVOKEVIRTUAL | opcodes::INVOKESPECIAL | opcodes::INVOKEINTERFACE => {
                if let Some(call) = callsites.get(&(offset as u32)) {
                    let arg_count = method_param_count(&call.descriptor)?;
                    for _ in 0..arg_count {
                        stack.pop();
                    }
                    let receiver = stack.pop().unwrap_or(ValueKind::Unknown);
                    if call.name == "equals"
                        && call.descriptor == "(Ljava/lang/Object;)Z"
                        && receiver == ValueKind::Array
                    {
                        let message = result_message(format!(
                            "Array comparison uses equals(): {}.{}{}",
                            class_name, method.name, method.descriptor
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
                    if is_reference_return(&call.descriptor)? {
                        stack.push(return_kind(&call.descriptor)?);
                    }
                }
            }
            opcodes::INVOKESTATIC => {
                if let Some(call) = callsites.get(&(offset as u32)) {
                    let arg_count = method_param_count(&call.descriptor)?;
                    for _ in 0..arg_count {
                        stack.pop();
                    }
                    if is_reference_return(&call.descriptor)? {
                        stack.push(return_kind(&call.descriptor)?);
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
        locals.push(ValueKind::NonArray);
    }
    let descriptor =
        MethodDescriptor::from_str(&method.descriptor).context("parse method descriptor")?;
    for param in descriptor.parameter_types() {
        let is_array = matches!(param, TypeDescriptor::Array(_, _));
        locals.push(if is_array {
            ValueKind::Array
        } else {
            ValueKind::NonArray
        });
        if matches!(param, TypeDescriptor::Long | TypeDescriptor::Double) {
            locals.push(ValueKind::NonArray);
        }
    }
    Ok(locals)
}

fn ensure_local(locals: &mut Vec<ValueKind>, index: usize) {
    if index >= locals.len() {
        locals.resize(index + 1, ValueKind::Unknown);
    }
}

fn is_reference_return(descriptor: &str) -> Result<bool> {
    let descriptor = MethodDescriptor::from_str(descriptor).context("parse call descriptor")?;
    Ok(matches!(
        descriptor.return_type(),
        TypeDescriptor::Object(_) | TypeDescriptor::Array(_, _)
    ))
}

fn return_kind(descriptor: &str) -> Result<ValueKind> {
    let descriptor = MethodDescriptor::from_str(descriptor).context("parse call descriptor")?;
    let kind = match descriptor.return_type() {
        TypeDescriptor::Array(_, _) => ValueKind::Array,
        TypeDescriptor::Object(_) => ValueKind::NonArray,
        _ => ValueKind::Unknown,
    };
    Ok(kind)
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
            .filter(|result| result.rule_id.as_deref() == Some("ARRAY_EQUALS"))
            .filter_map(|result| result.message.text.clone())
            .collect()
    }

    #[test]
    fn array_equals_reports_reference_comparison() {
        let sources = vec![SourceFile {
            path: "com/example/Sample.java".to_string(),
            contents: r#"
package com.example;
public class Sample {
    public boolean same(String[] left, String[] right) {
        return left == right;
    }
}
"#
            .to_string(),
        }];
        let messages = analyze_sources(sources);
        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("reference equality"))
        );
    }

    #[test]
    fn array_equals_reports_equals_call() {
        let sources = vec![SourceFile {
            path: "com/example/Sample.java".to_string(),
            contents: r#"
package com.example;
public class Sample {
    public boolean same(String[] left, String[] right) {
        return left.equals(right);
    }
}
"#
            .to_string(),
        }];
        let messages = analyze_sources(sources);
        assert!(messages.iter().any(|msg| msg.contains("equals()")));
    }

    #[test]
    fn array_equals_ignores_arrays_equals_helper() {
        let sources = vec![SourceFile {
            path: "com/example/Sample.java".to_string(),
            contents: r#"
package com.example;
import java.util.Arrays;
public class Sample {
    public boolean same(String[] left, String[] right) {
        return Arrays.equals(left, right);
    }
}
"#
            .to_string(),
        }];
        let messages = analyze_sources(sources);
        assert!(messages.is_empty());
    }

    #[test]
    fn array_equals_ignores_object_reference_comparison() {
        let sources = vec![SourceFile {
            path: "com/example/Sample.java".to_string(),
            contents: r#"
package com.example;
public class Sample {
    public boolean same(Object left, Object right) {
        return left == right;
    }
}
"#
            .to_string(),
        }];
        let messages = analyze_sources(sources);
        assert!(messages.is_empty());
    }

    #[test]
    fn array_equals_ignores_null_comparison() {
        let sources = vec![SourceFile {
            path: "com/example/Sample.java".to_string(),
            contents: r#"
package com.example;
public class Sample {
    public boolean same(String[] left) {
        return left == null;
    }
}
"#
            .to_string(),
        }];
        let messages = analyze_sources(sources);
        assert!(messages.is_empty());
    }
}
