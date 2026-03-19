use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

use anyhow::{Context, Result};
use jdescriptor::{MethodDescriptor, TypeDescriptor};
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::engine::AnalysisContext;
use crate::ir::{CallKind, Class, Instruction, InstructionKind, Method};
use crate::opcodes;
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

/// Rule that detects Koin singleton definitions that create AutoCloseable resources without closing them via onClose.
#[derive(Default)]
pub(crate) struct KoinAutoCloseableNotClosedRule;

crate::register_rule!(KoinAutoCloseableNotClosedRule);

impl Rule for KoinAutoCloseableNotClosedRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "KOIN_AUTOCLOSEABLE_NOT_CLOSED",
            name: "Koin AutoCloseable not closed",
            description: "Koin singleton definitions that construct AutoCloseable resources should close them via onClose",
        }
    }

    fn run(&self, context: &AnalysisContext) -> Result<Vec<SarifResult>> {
        if !context.has_koin() {
            return Ok(Vec::new());
        }

        let class_index: BTreeMap<&str, &Class> = context
            .all_classes()
            .map(|class| (class.name.as_str(), class))
            .collect();
        let mut findings = Vec::new();

        for class in context.analysis_target_classes() {
            let mut attributes = vec![KeyValue::new("inspequte.class", class.name.clone())];
            if let Some(uri) = context.class_artifact_uri(class) {
                attributes.push(KeyValue::new("inspequte.artifact_uri", uri));
            }
            let class_findings = context.with_span(
                "scan.class",
                &attributes,
                || -> Result<Vec<RuleFinding>> {
                    let artifact_uri = context.class_artifact_uri(class);
                    let mut class_findings = Vec::new();
                    for method in &class.methods {
                        class_findings.extend(analyze_method(
                            class,
                            method,
                            artifact_uri.as_deref(),
                            &class_index,
                        )?);
                    }
                    Ok(class_findings)
                },
            )?;
            findings.extend(class_findings);
        }

        findings.sort_by(|left, right| {
            left.class_name
                .cmp(&right.class_name)
                .then(left.method_name.cmp(&right.method_name))
                .then(left.method_descriptor.cmp(&right.method_descriptor))
                .then(left.offset.cmp(&right.offset))
        });

        Ok(findings
            .into_iter()
            .map(|finding| {
                let location = method_location_with_line(
                    &finding.class_name,
                    &finding.method_name,
                    &finding.method_descriptor,
                    finding.artifact_uri.as_deref(),
                    finding.line,
                );
                SarifResult::builder()
                    .message(result_message(finding.message))
                    .locations(vec![location])
                    .build()
            })
            .collect())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RuleFinding {
    class_name: String,
    method_name: String,
    method_descriptor: String,
    artifact_uri: Option<String>,
    line: Option<u32>,
    offset: u32,
    message: String,
}

fn analyze_method(
    class: &Class,
    method: &Method,
    artifact_uri: Option<&str>,
    class_index: &BTreeMap<&str, &Class>,
) -> Result<Vec<RuleFinding>> {
    let instructions = collect_instructions(method);
    let mut findings = Vec::new();

    for (definition_index, definition_call) in instructions.iter().enumerate().filter_map(|(idx, inst)| {
        let InstructionKind::Invoke(call) = &inst.kind else {
            return None;
        };
        if is_koin_single_call(call) {
            Some((idx, call))
        } else {
            None
        }
    }) {
        let Some(definition_lambda_name) = last_lambda_impl_in_range(&instructions, 0, definition_index)
        else {
            continue;
        };
        let Some(definition_lambda) = class
            .methods
            .iter()
            .find(|candidate| is_lambda_impl_method(candidate, &definition_lambda_name))
        else {
            continue;
        };
        let Some(resource_type) =
            created_autocloseable_resource_type(definition_lambda, class_index)?
        else {
            continue;
        };

        let callback_lambda = matching_onclose_callback(class, &instructions, definition_index);
        let callback_closes = callback_lambda
            .map(|lambda| lambda_calls_close(lambda))
            .unwrap_or(false);
        if callback_closes {
            continue;
        }

        findings.push(RuleFinding {
            class_name: class.name.clone(),
            method_name: method.name.clone(),
            method_descriptor: method.descriptor.clone(),
            artifact_uri: artifact_uri.map(ToOwned::to_owned),
            line: method.line_for_offset(definition_call.offset),
            offset: definition_call.offset,
            message: format!(
                "Koin singleton in {}.{}{} creates AutoCloseable resource {} but does not close it in onClose; add onClose {{ it?.close() }} or manage the resource lifecycle outside Koin.",
                class.name,
                method.name,
                method.descriptor,
                resource_type.replace('/', "."),
            ),
        });
    }

    Ok(findings)
}

fn collect_instructions(method: &Method) -> Vec<&Instruction> {
    let mut instructions: Vec<_> = method
        .cfg
        .blocks
        .iter()
        .flat_map(|block| block.instructions.iter())
        .collect();
    instructions.sort_by_key(|instruction| instruction.offset);
    instructions
}

fn last_lambda_impl_in_range(
    instructions: &[&Instruction],
    start_index: usize,
    end_index: usize,
) -> Option<String> {
    if start_index > end_index || end_index >= instructions.len() {
        return None;
    }
    instructions[start_index..=end_index]
        .iter()
        .rev()
        .find_map(|instruction| match &instruction.kind {
            InstructionKind::InvokeDynamic {
                descriptor,
                impl_method: Some(name),
            } if is_lambda_factory_descriptor(descriptor) => Some(name.clone()),
            _ => None,
        })
}

fn is_lambda_factory_descriptor(descriptor: &str) -> bool {
    descriptor.contains("Lkotlin/jvm/functions/Function")
}

fn is_lambda_impl_method(method: &Method, expected_name: &str) -> bool {
    method.name == expected_name
        && method.access.is_static
        && !method.access.is_abstract
        && looks_like_lambda_impl_name(expected_name)
}

fn looks_like_lambda_impl_name(name: &str) -> bool {
    name.contains("$lambda$") || name.starts_with("lambda$")
}

fn is_koin_single_call(call: &crate::ir::CallSite) -> bool {
    call.owner == "org/koin/core/module/Module" && (call.name == "single" || call.name == "single$default")
}

fn is_koin_onclose_call(call: &crate::ir::CallSite) -> bool {
    call.name == "onClose"
        && (call.owner == "org/koin/core/module/dsl/OptionDSLKt"
            || call.owner.ends_with("/BeanDefinition")
            || call.owner.ends_with("/Definition"))
}

fn matching_onclose_callback<'a>(
    class: &'a Class,
    instructions: &[&Instruction],
    definition_index: usize,
) -> Option<&'a Method> {
    let search_start = definition_index.saturating_add(1);
    if search_start >= instructions.len() {
        return None;
    }

    let end_index = instructions[search_start..]
        .iter()
        .position(|instruction| match &instruction.kind {
            InstructionKind::Invoke(call) => is_koin_single_call(call),
            _ => false,
        })
        .map(|relative| search_start + relative)
        .unwrap_or(instructions.len());

    for callback_index in search_start..end_index {
        let InstructionKind::Invoke(call) = &instructions[callback_index].kind else {
            continue;
        };
        if !is_koin_onclose_call(call) {
            continue;
        }
        let Some(callback_lambda_name) =
            last_lambda_impl_in_range(instructions, search_start, callback_index)
        else {
            continue;
        };
        let callback_lambda = class
            .methods
            .iter()
            .find(|candidate| is_lambda_impl_method(candidate, &callback_lambda_name))?;
        return Some(callback_lambda);
    }

    None
}

fn created_autocloseable_resource_type(
    lambda_method: &Method,
    class_index: &BTreeMap<&str, &Class>,
) -> Result<Option<String>> {
    let descriptor =
        MethodDescriptor::from_str(&lambda_method.descriptor).context("parse lambda descriptor")?;
    let return_type = match descriptor.return_type() {
        TypeDescriptor::Object(name) => name.as_str(),
        _ => return Ok(None),
    };
    let instructions = collect_instructions(lambda_method);

    for (instruction_index, call) in instructions.iter().filter_map(|instruction| {
        let InstructionKind::Invoke(call) = &instruction.kind else {
            return None;
        };
        Some((instruction_index_for_offset(&instructions, instruction.offset)?, call))
    }) {
        if call.kind != CallKind::Special || call.name != "<init>" {
            continue;
        }
        if !implements_autocloseable(&call.owner, class_index) {
            continue;
        }
        if !matches_return_type(&call.owner, return_type, class_index) {
            continue;
        }
        if constructed_value_is_returned(lambda_method, &instructions, instruction_index) {
            return Ok(Some(call.owner.clone()));
        }
    }

    Ok(None)
}

fn instruction_index_for_offset(instructions: &[&Instruction], offset: u32) -> Option<usize> {
    instructions
        .iter()
        .position(|instruction| instruction.offset == offset)
}

fn matches_return_type(
    constructed_type: &str,
    return_type: &str,
    class_index: &BTreeMap<&str, &Class>,
) -> bool {
    return_type == "java/lang/Object"
        || constructed_type == return_type
        || is_assignable_to(constructed_type, return_type, class_index)
}

fn constructed_value_is_returned(
    lambda_method: &Method,
    instructions: &[&Instruction],
    constructor_index: usize,
) -> bool {
    let Some(next_instruction) = instructions.get(constructor_index + 1) else {
        return false;
    };
    if next_instruction.opcode == opcodes::ARETURN {
        return true;
    }

    let Some(local_index) = astore_local_index(lambda_method, next_instruction) else {
        return false;
    };

    let mut scan_index = constructor_index + 2;
    while let Some(instruction) = instructions.get(scan_index) {
        if overwrites_local(lambda_method, instruction, local_index) {
            return false;
        }
        if aload_local_index(lambda_method, instruction) == Some(local_index) {
            let Some(return_instruction) = instructions.get(scan_index + 1) else {
                return false;
            };
            return return_instruction.opcode == opcodes::ARETURN;
        }
        scan_index += 1;
    }

    false
}

fn astore_local_index(method: &Method, instruction: &Instruction) -> Option<usize> {
    match instruction.opcode {
        opcodes::ASTORE => method.bytecode.get(instruction.offset as usize + 1).map(|v| *v as usize),
        opcodes::ASTORE_0 => Some(0),
        opcodes::ASTORE_1 => Some(1),
        opcodes::ASTORE_2 => Some(2),
        opcodes::ASTORE_3 => Some(3),
        _ => None,
    }
}

fn aload_local_index(method: &Method, instruction: &Instruction) -> Option<usize> {
    match instruction.opcode {
        opcodes::ALOAD => method.bytecode.get(instruction.offset as usize + 1).map(|v| *v as usize),
        opcodes::ALOAD_0 => Some(0),
        opcodes::ALOAD_1 => Some(1),
        opcodes::ALOAD_2 => Some(2),
        opcodes::ALOAD_3 => Some(3),
        _ => None,
    }
}

fn overwrites_local(method: &Method, instruction: &Instruction, local_index: usize) -> bool {
    astore_local_index(method, instruction) == Some(local_index)
}

fn lambda_calls_close(lambda_method: &Method) -> bool {
    lambda_method.calls.iter().any(|call| {
        call.name == "close"
            && call.descriptor == "()V"
            && matches!(
                call.kind,
                CallKind::Virtual | CallKind::Interface | CallKind::Special
            )
    })
}

fn implements_autocloseable(type_name: &str, class_index: &BTreeMap<&str, &Class>) -> bool {
    is_assignable_to(type_name, "java/lang/AutoCloseable", class_index)
        || is_assignable_to(type_name, "java/io/Closeable", class_index)
}

fn is_assignable_to(
    type_name: &str,
    target_name: &str,
    class_index: &BTreeMap<&str, &Class>,
) -> bool {
    let mut pending = vec![type_name.to_string()];
    let mut seen = BTreeSet::new();

    while let Some(current) = pending.pop() {
        if !seen.insert(current.clone()) {
            continue;
        }
        if current == target_name {
            return true;
        }
        let Some(class) = class_index.get(current.as_str()).copied() else {
            continue;
        };
        if let Some(super_name) = &class.super_name {
            pending.push(super_name.clone());
        }
        pending.extend(class.interfaces.iter().cloned());
    }

    false
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::engine::EngineOutput;
    use crate::test_harness::{JvmTestHarness, Language, SourceFile};

    fn rule_messages(output: &EngineOutput) -> Vec<String> {
        output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("KOIN_AUTOCLOSEABLE_NOT_CLOSED"))
            .filter_map(|result| result.message.text.clone())
            .collect()
    }

    fn koin_stub_sources() -> Vec<SourceFile> {
        vec![
            SourceFile {
                path: "org/koin/core/module/Module.kt".to_string(),
                contents: r#"
package org.koin.core.module

class Module {
    fun <T> single(definition: () -> T): BeanDefinition<T> = BeanDefinition(definition)
}

class BeanDefinition<T>(val definition: () -> T)
"#
                .to_string(),
            },
            SourceFile {
                path: "org/koin/core/module/dsl/OptionDSL.kt".to_string(),
                contents: r#"
package org.koin.core.module.dsl

import org.koin.core.module.BeanDefinition

infix fun <T> BeanDefinition<T>.onClose(callback: (T?) -> Unit): BeanDefinition<T> = this
"#
                .to_string(),
            },
        ]
    }

    fn compile_and_analyze(
        harness: &JvmTestHarness,
        sources: Vec<SourceFile>,
        classpath: &[PathBuf],
    ) -> EngineOutput {
        harness
            .compile_and_analyze(Language::Kotlin, &sources, classpath)
            .expect("run harness analysis")
    }

    #[test]
    fn koin_autocloseable_not_closed_reports_missing_onclose() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let mut sources = koin_stub_sources();
        sources.push(SourceFile {
            path: "com/example/ClassA.kt".to_string(),
            contents: r#"
package com.example

import org.koin.core.module.Module

class ClassA : AutoCloseable {
    override fun close() {}
}

fun methodOne(module: Module) {
    module.single { ClassA() }
}
"#
            .to_string(),
        });

        let output = compile_and_analyze(&harness, sources, &[]);
        let messages = rule_messages(&output);
        assert_eq!(messages.len(), 1, "expected one finding, got {messages:?}");
        assert!(messages[0].contains("ClassA"));
        assert!(messages[0].contains("onClose"));
    }

    #[test]
    fn koin_autocloseable_not_closed_reports_callback_without_close() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let mut sources = koin_stub_sources();
        sources.push(SourceFile {
            path: "com/example/ClassA.kt".to_string(),
            contents: r#"
package com.example

import org.koin.core.module.Module
import org.koin.core.module.dsl.onClose

class ClassA : AutoCloseable {
    override fun close() {}
}

fun methodTwo(module: Module) {
    module.single { ClassA() } onClose { _ -> println("ignored") }
}
"#
            .to_string(),
        });

        let output = compile_and_analyze(&harness, sources, &[]);
        let messages = rule_messages(&output);
        assert_eq!(messages.len(), 1, "expected one finding, got {messages:?}");
        assert!(messages[0].contains("ClassA"));
    }

    #[test]
    fn koin_autocloseable_not_closed_ignores_proper_cleanup() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let mut sources = koin_stub_sources();
        sources.push(SourceFile {
            path: "com/example/ClassA.kt".to_string(),
            contents: r#"
package com.example

import org.koin.core.module.Module
import org.koin.core.module.dsl.onClose

class ClassA : AutoCloseable {
    override fun close() {}
}

fun methodThree(module: Module) {
    module.single { ClassA() } onClose { it?.close() }
}
"#
            .to_string(),
        });

        let output = compile_and_analyze(&harness, sources, &[]);
        let messages = rule_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect findings for close in onClose: {messages:?}"
        );
    }

    #[test]
    fn koin_autocloseable_not_closed_ignores_non_autocloseable_types() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let mut sources = koin_stub_sources();
        sources.push(SourceFile {
            path: "com/example/ClassB.kt".to_string(),
            contents: r#"
package com.example

import org.koin.core.module.Module

class ClassB

fun methodFour(module: Module) {
    module.single { ClassB() }
}
"#
            .to_string(),
        });

        let output = compile_and_analyze(&harness, sources, &[]);
        let messages = rule_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect findings for non-AutoCloseable resource: {messages:?}"
        );
    }

    #[test]
    fn koin_autocloseable_not_closed_reports_only_leaking_definition_in_mixed_method() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let mut sources = koin_stub_sources();
        sources.push(SourceFile {
            path: "com/example/ClassMixed.kt".to_string(),
            contents: r#"
package com.example

import org.koin.core.module.Module
import org.koin.core.module.dsl.onClose

class ClassA : AutoCloseable {
    override fun close() {}
}

class ClassB : AutoCloseable {
    override fun close() {}
}

fun methodFive(module: Module) {
    module.single { ClassA() }
    module.single { ClassB() } onClose { it?.close() }
}
"#
            .to_string(),
        });

        let output = compile_and_analyze(&harness, sources, &[]);
        let messages = rule_messages(&output);
        assert_eq!(messages.len(), 1, "expected one finding, got {messages:?}");
        assert!(messages[0].contains("ClassA"));
        assert!(!messages[0].contains("ClassB"));
    }

    #[test]
    fn koin_autocloseable_not_closed_supports_classpath_resource_types() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let dependency_sources = vec![SourceFile {
            path: "com/dependency/ClassDep.kt".to_string(),
            contents: r#"
package com.dependency

class ClassDep : AutoCloseable {
    override fun close() {}
}
"#
            .to_string(),
        }];
        let dependency_output = harness
            .compile(Language::Kotlin, &dependency_sources, &[])
            .expect("compile dependency classes");

        let mut sources = koin_stub_sources();
        sources.push(SourceFile {
            path: "com/example/ClassUseDep.kt".to_string(),
            contents: r#"
package com.example

import com.dependency.ClassDep
import org.koin.core.module.Module

fun methodSix(module: Module) {
    module.single { ClassDep() }
}
"#
            .to_string(),
        });

        let output =
            compile_and_analyze(&harness, sources, &[dependency_output.classes_dir().to_path_buf()]);
        let messages = rule_messages(&output);
        assert_eq!(messages.len(), 1, "expected one finding, got {messages:?}");
        assert!(messages[0].contains("ClassDep"));
    }

    #[test]
    fn koin_autocloseable_not_closed_ignores_classpath_only_module_code() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let mut dependency_sources = koin_stub_sources();
        dependency_sources.push(SourceFile {
            path: "com/example/ClassDepModule.kt".to_string(),
            contents: r#"
package com.example

import org.koin.core.module.Module

class ClassA : AutoCloseable {
    override fun close() {}
}

fun methodSeven(module: Module) {
    module.single { ClassA() }
}
"#
            .to_string(),
        });
        let dependency_output = harness
            .compile(Language::Kotlin, &dependency_sources, &[])
            .expect("compile dependency classes");

        let app_sources = vec![SourceFile {
            path: "com/example/ClassApp.kt".to_string(),
            contents: r#"
package com.example

class ClassApp
"#
            .to_string(),
        }];

        let output = compile_and_analyze(
            &harness,
            app_sources,
            &[dependency_output.classes_dir().to_path_buf()],
        );
        let messages = rule_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect findings from classpath-only module code: {messages:?}"
        );
    }

    #[test]
    fn koin_autocloseable_not_closed_ignores_non_returned_autocloseable_construction() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let mut sources = koin_stub_sources();
        sources.push(SourceFile {
            path: "com/example/ClassUnused.kt".to_string(),
            contents: r#"
package com.example

import org.koin.core.module.Module

class ClassA : AutoCloseable {
    override fun close() {}
}

fun methodEight(module: Module) {
    module.single {
        ClassA()
        "value"
    }
}
"#
            .to_string(),
        });

        let output = compile_and_analyze(&harness, sources, &[]);
        let messages = rule_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect findings when AutoCloseable is not returned: {messages:?}"
        );
    }

    #[test]
    fn koin_autocloseable_not_closed_reports_returned_local_resource() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let mut sources = koin_stub_sources();
        sources.push(SourceFile {
            path: "com/example/ClassLocal.kt".to_string(),
            contents: r#"
package com.example

import org.koin.core.module.Module

class ClassA : AutoCloseable {
    override fun close() {}
}

fun methodNine(module: Module) {
    module.single {
        val varOne = ClassA()
        println("marker")
        varOne
    }
}
"#
            .to_string(),
        });

        let output = compile_and_analyze(&harness, sources, &[]);
        let messages = rule_messages(&output);
        assert_eq!(messages.len(), 1, "expected one finding, got {messages:?}");
        assert!(messages[0].contains("ClassA"));
    }
}
