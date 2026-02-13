use std::collections::{BTreeMap, HashMap};
use std::str::FromStr;

use anyhow::{Context, Result};
use jdescriptor::{MethodDescriptor, TypeDescriptor};
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::dataflow::opcode_semantics::{
    ApplyOutcome, SemanticsCoverage, SemanticsDebugConfig, SemanticsHooks, ValueDomain,
    apply_semantics, opcode_semantics_debug_enabled,
};
use crate::dataflow::stack_machine::StackMachine;
use crate::engine::AnalysisContext;
use crate::ir::{CallSite, Method};
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
                    let artifact_uri = context.class_artifact_uri(class);
                    for method in &class.methods {
                        if method.bytecode.is_empty() {
                            continue;
                        }
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

/// Rule-specific hook that keeps array-aware semantics in one place.
#[derive(Default)]
struct ArraySemanticsHook {
    reference_equality_offsets: Vec<u32>,
}

impl ArraySemanticsHook {
    fn take_reference_equality_offsets(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.reference_equality_offsets)
    }
}

impl SemanticsHooks<ValueKind> for ArraySemanticsHook {
    fn pre_apply(
        &mut self,
        machine: &mut StackMachine<ValueKind>,
        method: &Method,
        offset: usize,
        opcode: u8,
    ) -> ApplyOutcome {
        match opcode {
            opcodes::NEWARRAY | opcodes::ANEWARRAY => {
                machine.pop();
                machine.push(ValueKind::Array(1));
                ApplyOutcome::Applied
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
                ApplyOutcome::Applied
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
                ApplyOutcome::Applied
            }
            opcodes::ARRAYLENGTH => {
                machine.pop();
                machine.push(ValueKind::NonArray);
                ApplyOutcome::Applied
            }
            opcodes::IF_ACMPEQ | opcodes::IF_ACMPNE => {
                let right = machine.pop();
                let left = machine.pop();
                if matches!(left, ValueKind::Array(_)) && matches!(right, ValueKind::Array(_)) {
                    self.reference_equality_offsets.push(offset as u32);
                }
                ApplyOutcome::Applied
            }
            _ => ApplyOutcome::NotHandled,
        }
    }
}

/// Cached descriptor summary used to avoid repeated parse work in invoke handling.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct CallDescriptorSummary {
    arg_count: usize,
    return_kind: Option<ValueKind>,
}

fn analyze_method(
    class_name: &str,
    method: &Method,
    artifact_uri: Option<&str>,
) -> Result<Vec<SarifResult>> {
    let mut results = Vec::new();
    let mut next_call_index = 0usize;
    let mut descriptor_cache = HashMap::new();

    let mut machine = StackMachine::new(ValueKind::Unknown);
    let domain = ArrayValueDomain;
    let mut hooks = ArraySemanticsHook::default();
    let mut coverage = SemanticsCoverage::default();
    let debug = SemanticsDebugConfig {
        enabled: opcode_semantics_debug_enabled(),
        rule_id: "ARRAY_EQUALS",
    };
    for (index, value) in initial_locals(method)? {
        machine.store_local(index, value);
    }
    let mut offset = 0usize;
    while offset < method.bytecode.len() {
        let opcode = method.bytecode[offset];
        if apply_semantics(
            &mut machine,
            method,
            offset,
            opcode,
            &domain,
            &mut hooks,
            &mut coverage,
            debug,
        ) == ApplyOutcome::Applied
        {
            for finding_offset in hooks.take_reference_equality_offsets() {
                let message = result_message(format!(
                    "Array comparison uses reference equality: {}.{}{}",
                    class_name, method.name, method.descriptor
                ));
                let line = method.line_for_offset(finding_offset);
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
            let length = crate::scan::opcode_length(&method.bytecode, offset)?;
            offset += length;
            continue;
        }
        match opcode {
            opcodes::INVOKEVIRTUAL
            | opcodes::INVOKESPECIAL
            | opcodes::INVOKEINTERFACE
            | opcodes::INVOKESTATIC => {
                if let Some(call) = callsite_for_offset(method, &mut next_call_index, offset as u32)
                {
                    let descriptor =
                        call_descriptor_summary(&mut descriptor_cache, &call.descriptor)?;
                    for _ in 0..descriptor.arg_count {
                        machine.pop();
                    }

                    if opcode != opcodes::INVOKESTATIC {
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
                    }

                    if let Some(return_kind) = descriptor.return_kind {
                        machine.push(return_kind);
                    }
                }
            }
            _ => {}
        }
        let length = crate::scan::opcode_length(&method.bytecode, offset)?;
        offset += length;
    }

    if debug.enabled && coverage.fallback_not_handled > 0 {
        let invoke_fallbacks = coverage.fallback_count(opcodes::INVOKEVIRTUAL)
            + coverage.fallback_count(opcodes::INVOKESPECIAL)
            + coverage.fallback_count(opcodes::INVOKESTATIC)
            + coverage.fallback_count(opcodes::INVOKEINTERFACE);
        eprintln!(
            "opcode_semantics debug: rule=ARRAY_EQUALS method={}{} default={} hook={} fallback={} if_acmp_hook={} invoke_fallback={}",
            method.name,
            method.descriptor,
            coverage.applied_by_default,
            coverage.applied_by_hook,
            coverage.fallback_not_handled,
            coverage.hook_override_count(opcodes::IF_ACMPEQ)
                + coverage.hook_override_count(opcodes::IF_ACMPNE),
            invoke_fallbacks
        );
    }

    Ok(results)
}

fn callsite_for_offset<'a>(
    method: &'a Method,
    next_call_index: &mut usize,
    offset: u32,
) -> Option<&'a CallSite> {
    while *next_call_index < method.calls.len() {
        let call = &method.calls[*next_call_index];
        if call.offset < offset {
            *next_call_index += 1;
            continue;
        }
        if call.offset == offset {
            *next_call_index += 1;
            return Some(call);
        }
        break;
    }
    None
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

fn call_descriptor_summary<'a>(
    cache: &mut HashMap<&'a str, CallDescriptorSummary>,
    descriptor: &'a str,
) -> Result<CallDescriptorSummary> {
    if let Some(summary) = cache.get(descriptor) {
        return Ok(*summary);
    }

    let parsed = MethodDescriptor::from_str(descriptor).context("parse call descriptor")?;
    let summary = CallDescriptorSummary {
        arg_count: parsed.parameter_types().len(),
        return_kind: match parsed.return_type() {
            TypeDescriptor::Array(_, dims) => Some(ValueKind::Array(*dims)),
            TypeDescriptor::Object(_) => Some(ValueKind::NonArray),
            _ => None,
        },
    };
    cache.insert(descriptor, summary);
    Ok(summary)
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
