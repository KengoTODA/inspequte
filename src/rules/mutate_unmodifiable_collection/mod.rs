use std::collections::BTreeMap;
use std::str::FromStr;

use anyhow::{Context, Result};
use jdescriptor::MethodDescriptor;
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::descriptor::{ReturnKind, method_return_kind};
use crate::engine::AnalysisContext;
use crate::ir::{CallKind, CallSite, Method};
use crate::opcodes;
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

/// Rule that detects mutations applied to known-unmodifiable collections.
#[derive(Default)]
pub(crate) struct MutateUnmodifiableCollectionRule;

crate::register_rule!(MutateUnmodifiableCollectionRule);

impl Rule for MutateUnmodifiableCollectionRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "MUTATE_UNMODIFIABLE_COLLECTION",
            name: "Mutation on unmodifiable collection",
            description: "Mutation calls on known JDK unmodifiable collection values",
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
                context.with_span("rule.class", &attributes, || -> Result<Vec<SarifResult>> {
                    let mut class_results = Vec::new();
                    for method in &class.methods {
                        if method.bytecode.is_empty() {
                            continue;
                        }
                        class_results.extend(analyze_method(
                            &class.name,
                            method,
                            context.class_artifact_uri(class).as_deref(),
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
    KnownUnmodifiable,
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
    let mut stack = Vec::new();
    let mut offset = 0usize;
    while offset < method.bytecode.len() {
        let opcode = method.bytecode[offset];
        match opcode {
            opcodes::ACONST_NULL | opcodes::LDC | opcodes::LDC_W | opcodes::LDC2_W => {
                stack.push(ValueKind::Unknown);
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
                locals[index] = stack.pop().unwrap_or(ValueKind::Unknown);
            }
            opcodes::ASTORE_0 | opcodes::ASTORE_1 | opcodes::ASTORE_2 | opcodes::ASTORE_3 => {
                let index = (opcode - opcodes::ASTORE_0) as usize;
                ensure_local(&mut locals, index);
                locals[index] = stack.pop().unwrap_or(ValueKind::Unknown);
            }
            opcodes::NEW | opcodes::ANEWARRAY | opcodes::NEWARRAY | opcodes::MULTIANEWARRAY => {
                stack.push(ValueKind::Unknown);
            }
            opcodes::DUP => {
                if let Some(value) = stack.last().copied() {
                    stack.push(value);
                }
            }
            opcodes::POP => {
                stack.pop();
            }
            opcodes::POP2 => {
                stack.pop();
                stack.pop();
            }
            opcodes::AASTORE => {
                stack.pop();
                stack.pop();
                stack.pop();
            }
            opcodes::AALOAD => {
                stack.pop();
                stack.pop();
                stack.push(ValueKind::Unknown);
            }
            opcodes::INVOKEVIRTUAL
            | opcodes::INVOKEINTERFACE
            | opcodes::INVOKESPECIAL
            | opcodes::INVOKESTATIC => {
                if let Some(call) = callsites.get(&(offset as u32)) {
                    handle_call(
                        class_name,
                        method,
                        artifact_uri,
                        offset as u32,
                        call,
                        &mut stack,
                        &mut results,
                    )?;
                }
            }
            _ => {}
        }
        let length = crate::scan::opcode_length(&method.bytecode, offset)?;
        offset += length;
    }

    Ok(results)
}

fn handle_call(
    class_name: &str,
    method: &Method,
    artifact_uri: Option<&str>,
    offset: u32,
    call: &CallSite,
    stack: &mut Vec<ValueKind>,
    results: &mut Vec<SarifResult>,
) -> Result<()> {
    let descriptor =
        MethodDescriptor::from_str(&call.descriptor).context("parse call descriptor")?;
    let param_count = descriptor.parameter_types().len();
    for _ in 0..param_count {
        stack.pop();
    }

    let receiver = if call.kind != CallKind::Static {
        stack.pop().unwrap_or(ValueKind::Unknown)
    } else {
        ValueKind::Unknown
    };

    if receiver == ValueKind::KnownUnmodifiable && is_mutator_call(call) {
        let message = result_message(format!(
            "Unmodifiable collection is mutated in {}.{}{}; create a mutable copy before calling {}().",
            class_name, method.name, method.descriptor, call.name
        ));
        let line = method.line_for_offset(offset);
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

    let return_value = if is_unmodifiable_factory_call(call) {
        Some(ValueKind::KnownUnmodifiable)
    } else {
        match method_return_kind(&call.descriptor)? {
            ReturnKind::Void => None,
            ReturnKind::Primitive | ReturnKind::Reference => Some(ValueKind::Unknown),
        }
    };
    if let Some(value) = return_value {
        stack.push(value);
    }

    Ok(())
}

fn is_unmodifiable_factory_call(call: &CallSite) -> bool {
    if call.owner == "java/util/stream/Stream"
        && call.name == "toList"
        && call.descriptor == "()Ljava/util/List;"
    {
        return true;
    }
    if call.kind != CallKind::Static {
        return false;
    }
    match (call.owner.as_str(), call.name.as_str()) {
        ("java/util/List", "of" | "copyOf")
        | ("java/util/Set", "of" | "copyOf")
        | ("java/util/Map", "of" | "ofEntries" | "copyOf") => true,
        ("java/util/Collections", name) => {
            name.starts_with("unmodifiable")
                || name.starts_with("empty")
                || name.starts_with("singleton")
        }
        _ => false,
    }
}

fn is_mutator_call(call: &CallSite) -> bool {
    if !call.owner.starts_with("java/util/") {
        return false;
    }
    matches!(
        call.name.as_str(),
        "add"
            | "addAll"
            | "clear"
            | "compute"
            | "computeIfAbsent"
            | "computeIfPresent"
            | "merge"
            | "put"
            | "putAll"
            | "putIfAbsent"
            | "remove"
            | "removeAll"
            | "removeIf"
            | "replace"
            | "replaceAll"
            | "retainAll"
            | "set"
            | "sort"
    )
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
            .filter(|result| result.rule_id.as_deref() == Some("MUTATE_UNMODIFIABLE_COLLECTION"))
            .filter_map(|result| result.message.text.clone())
            .collect()
    }

    #[test]
    fn mutate_unmodifiable_collection_reports_list_of_mutation() {
        let sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;

import java.util.List;

public class ClassA {
    public void methodX() {
        List<String> varOne = List.of("tmpValue");
        varOne.add("varTwo");
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_sources(sources);
        assert_eq!(messages.len(), 1);
        assert!(messages[0].contains("mutable copy"));
    }

    #[test]
    fn mutate_unmodifiable_collection_reports_collections_wrapper_mutation() {
        let sources = vec![SourceFile {
            path: "com/example/ClassB.java".to_string(),
            contents: r#"
package com.example;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

public class ClassB {
    public void methodY() {
        List<String> varOne = Collections.unmodifiableList(new ArrayList<>());
        varOne.add("varTwo");
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_sources(sources);
        assert_eq!(messages.len(), 1);
        assert!(messages[0].contains("methodY"));
    }

    #[test]
    fn mutate_unmodifiable_collection_ignores_mutable_copy() {
        let sources = vec![SourceFile {
            path: "com/example/ClassC.java".to_string(),
            contents: r#"
package com.example;

import java.util.ArrayList;
import java.util.List;

public class ClassC {
    public void methodZ() {
        List<String> varOne = new ArrayList<>(List.of("tmpValue"));
        varOne.add("varTwo");
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_sources(sources);
        assert!(messages.is_empty());
    }

    #[test]
    fn mutate_unmodifiable_collection_reports_only_unmodifiable_receiver() {
        let sources = vec![SourceFile {
            path: "com/example/ClassD.java".to_string(),
            contents: r#"
package com.example;

import java.util.ArrayList;
import java.util.List;
import java.util.Set;

public class ClassD {
    public void methodW() {
        Set<String> varOne = Set.of("tmpValue");
        varOne.remove("tmpValue");

        List<String> varTwo = new ArrayList<>();
        varTwo.add("varThree");
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_sources(sources);
        assert_eq!(messages.len(), 1);
        assert!(messages[0].contains("methodW"));
    }
}
