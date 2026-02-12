use std::collections::BTreeMap;
use std::str::FromStr;

use anyhow::{Context, Result};
use jdescriptor::{MethodDescriptor, TypeDescriptor};
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::dataflow::opcode_semantics::{ApplyOutcome, ValueDomain, apply_default_semantics};
use crate::dataflow::stack_machine::StackMachine;
use crate::descriptor::method_param_count;
use crate::engine::AnalysisContext;
use crate::ir::Method;
use crate::opcodes;
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

/// Rule that flags array comparisons using == or equals().
#[derive(Default)]
pub(crate) struct ArrayEqualsRule;

crate::register_rule!(ArrayEqualsRule);

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
    Array(u8),
    NonArray,
}

/// Value-domain adapter used by shared default opcode semantics.
struct ArrayValueDomain;

impl ValueDomain<ValueKind> for ArrayValueDomain {
    fn unknown_value(&self) -> ValueKind {
        ValueKind::Unknown
    }

    fn scalar_value(&self) -> ValueKind {
        ValueKind::NonArray
    }
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

    let mut machine = StackMachine::new(ValueKind::Unknown);
    let domain = ArrayValueDomain;
    for (index, value) in initial_locals(method)? {
        machine.store_local(index, value);
    }
    let mut offset = 0usize;
    while offset < method.bytecode.len() {
        let opcode = method.bytecode[offset];
        if apply_default_semantics(&mut machine, method, offset, opcode, &domain)
            == ApplyOutcome::Applied
        {
            let length = crate::scan::opcode_length(&method.bytecode, offset)?;
            offset += length;
            continue;
        }
        match opcode {
            opcodes::NEWARRAY | opcodes::ANEWARRAY => {
                machine.pop();
                machine.push(ValueKind::Array(1));
            }
            opcodes::MULTIANEWARRAY => {
                let dims = method.bytecode.get(offset + 3).copied().unwrap_or(0);
                for _ in 0..dims {
                    machine.pop();
                }
                if dims > 0 {
                    machine.push(ValueKind::Array(dims));
                } else {
                    machine.push(ValueKind::Unknown);
                }
            }
            opcodes::AALOAD => {
                machine.pop();
                let array = machine.pop();
                let value = match array {
                    ValueKind::Array(dims) if dims > 1 => ValueKind::Array(dims - 1),
                    ValueKind::Array(_) => ValueKind::NonArray,
                    _ => ValueKind::Unknown,
                };
                machine.push(value);
            }
            opcodes::ARRAYLENGTH => {
                machine.pop();
                machine.push(ValueKind::NonArray);
            }
            opcodes::IF_ACMPEQ | opcodes::IF_ACMPNE => {
                let right = machine.pop();
                let left = machine.pop();
                if matches!(left, ValueKind::Array(_)) && matches!(right, ValueKind::Array(_)) {
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
                        machine.pop();
                    }
                    let receiver = machine.pop();
                    if call.name == "equals"
                        && call.descriptor == "(Ljava/lang/Object;)Z"
                        && matches!(receiver, ValueKind::Array(_))
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
                        machine.push(return_kind(&call.descriptor)?);
                    }
                }
            }
            opcodes::INVOKESTATIC => {
                if let Some(call) = callsites.get(&(offset as u32)) {
                    let arg_count = method_param_count(&call.descriptor)?;
                    for _ in 0..arg_count {
                        machine.pop();
                    }
                    if is_reference_return(&call.descriptor)? {
                        machine.push(return_kind(&call.descriptor)?);
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

fn initial_locals(method: &Method) -> Result<BTreeMap<usize, ValueKind>> {
    let mut locals = BTreeMap::new();
    let mut index = 0usize;
    if !method.access.is_static {
        locals.insert(index, ValueKind::NonArray);
        index += 1;
    }
    let descriptor =
        MethodDescriptor::from_str(&method.descriptor).context("parse method descriptor")?;
    for param in descriptor.parameter_types() {
        let value = match param {
            TypeDescriptor::Array(_, dims) => ValueKind::Array(*dims),
            _ => ValueKind::NonArray,
        };
        locals.insert(index, value);
        index += 1;
        if matches!(param, TypeDescriptor::Long | TypeDescriptor::Double) {
            locals.insert(index, ValueKind::NonArray);
            index += 1;
        }
    }
    Ok(locals)
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
        TypeDescriptor::Array(_, dims) => ValueKind::Array(*dims),
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
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
public class ClassA {
    public boolean methodOne(String[] varOne, String[] varTwo) {
        return varOne == varTwo;
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
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
public class ClassA {
    public boolean methodOne(String[] varOne, String[] varTwo) {
        return varOne.equals(varTwo);
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
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
import java.util.Arrays;
public class ClassA {
    public boolean methodOne(String[] varOne, String[] varTwo) {
        return Arrays.equals(varOne, varTwo);
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
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
public class ClassA {
    public boolean methodOne(Object varOne, Object varTwo) {
        return varOne == varTwo;
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
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
public class ClassA {
    public boolean methodOne(String[] varOne) {
        return varOne == null;
    }
}
"#
            .to_string(),
        }];
        let messages = analyze_sources(sources);
        assert!(messages.is_empty());
    }

    #[test]
    fn array_equals_ignores_varargs_iteration_equals() {
        let sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
class ClassB {}
public class ClassA {
    public boolean methodOne(ClassB varOne, ClassB... varTwo) {
        for (ClassB varThree : varTwo) {
            if (varOne.equals(varThree)) {
                return true;
            }
        }
        return false;
    }
}
"#
            .to_string(),
        }];
        let messages = analyze_sources(sources);
        assert!(messages.is_empty());
    }

    #[test]
    fn array_equals_ignores_array_element_equals_loop() {
        let sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
class ClassB {}
class ClassC {
    public String methodOne() { return ""; }
}
class ClassD {
    static ClassB[] methodTwo(String varOne) { return new ClassB[0]; }
}
public class ClassA {
    public ClassC methodThree(ClassC varOne, ClassB[] varTwo) {
        ClassB[] varThree = ClassD.methodTwo(varOne.methodOne());
        if (varTwo.length == varThree.length) {
            outer:
            for (int varFour = 0; varFour < varTwo.length; varFour++) {
                if (!varThree[varFour].equals(varTwo[varFour])) {
                    continue outer;
                }
            }
            return varOne;
        }
        return null;
    }
}
"#
            .to_string(),
        }];
        let messages = analyze_sources(sources);
        assert!(messages.is_empty());
    }
}
