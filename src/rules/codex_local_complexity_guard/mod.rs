use std::collections::BTreeSet;

use anyhow::{Context, Result};
use serde_sarif::sarif::Result as SarifResult;

use crate::engine::AnalysisContext;
use crate::ir::Method;
use crate::opcodes;
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

const RULE_ID: &str = "codex_local_complexity_guard";
const LOCAL_COMPLEXITY_THRESHOLD: u32 = 10;

/// Rule that reports methods with local cyclomatic complexity above a strict threshold.
#[derive(Default)]
pub(crate) struct CodexLocalComplexityGuardRule;

crate::register_rule!(CodexLocalComplexityGuardRule);

impl Rule for CodexLocalComplexityGuardRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: RULE_ID,
            name: "Local cyclomatic complexity guard",
            description: "Reports concrete methods whose local cyclomatic complexity exceeds a strict fixed threshold",
        }
    }

    fn run(&self, context: &AnalysisContext) -> Result<Vec<SarifResult>> {
        let mut findings = Vec::new();
        let mut seen_identities = BTreeSet::new();

        for class in context.analysis_target_classes() {
            let artifact_uri = context.class_artifact_uri(class);
            for method in &class.methods {
                if !is_executable_method(method) || is_compiler_generated_noise(method) {
                    continue;
                }

                let complexity = method_local_complexity(method)?;
                if complexity <= LOCAL_COMPLEXITY_THRESHOLD {
                    continue;
                }

                let identity =
                    MethodIdentity::new(class.name.clone(), method.name.clone(), method.descriptor.clone());
                if !seen_identities.insert(identity.clone()) {
                    continue;
                }

                findings.push(LocalComplexityFinding {
                    identity,
                    complexity,
                    line: method.line_for_offset(0),
                    artifact_uri: artifact_uri.clone(),
                });
            }
        }

        findings.sort_by(|left, right| left.identity.cmp(&right.identity));

        Ok(findings
            .into_iter()
            .map(|finding| {
                let message = result_message(format!(
                    "Method complexity {} exceeds local threshold {} in {}.{}{}; simplify control flow or split this method.",
                    finding.complexity,
                    LOCAL_COMPLEXITY_THRESHOLD,
                    finding.identity.class_name,
                    finding.identity.method_name,
                    finding.identity.descriptor
                ));
                let location = method_location_with_line(
                    &finding.identity.class_name,
                    &finding.identity.method_name,
                    &finding.identity.descriptor,
                    finding.artifact_uri.as_deref(),
                    finding.line,
                );
                SarifResult::builder()
                    .message(message)
                    .locations(vec![location])
                    .build()
            })
            .collect())
    }
}

fn is_executable_method(method: &Method) -> bool {
    !method.access.is_abstract && !method.bytecode.is_empty()
}

fn is_compiler_generated_noise(method: &Method) -> bool {
    method.access.is_synthetic || method.access.is_bridge
}

fn method_local_complexity(method: &Method) -> Result<u32> {
    let mut offset = 0usize;
    let mut decision_points = 0u32;
    let bytecode = method.bytecode.as_slice();

    while offset < bytecode.len() {
        let opcode = bytecode[offset];
        let branch_contribution = match opcode {
            opcodes::TABLESWITCH => tableswitch_non_default_branch_count(bytecode, offset)?,
            opcodes::LOOKUPSWITCH => lookupswitch_non_default_branch_count(bytecode, offset)?,
            _ if is_conditional_branch_opcode(opcode) => 1,
            _ => 0,
        };
        decision_points = decision_points.saturating_add(branch_contribution);

        let length = crate::scan::opcode_length(bytecode, offset)
            .with_context(|| format!("invalid opcode length at offset {offset}"))?;
        offset += length;
    }

    let catch_handlers = method
        .exception_handlers
        .iter()
        .filter(|handler| handler.catch_type.is_some())
        .count();
    let catch_handlers = u32::try_from(catch_handlers).context("catch handler count overflow")?;

    Ok(1u32
        .saturating_add(decision_points)
        .saturating_add(catch_handlers))
}

fn is_conditional_branch_opcode(opcode: u8) -> bool {
    matches!(opcode, 0x99..=0xa6 | opcodes::IFNULL | opcodes::IFNONNULL)
}

fn tableswitch_non_default_branch_count(code: &[u8], offset: usize) -> Result<u32> {
    let padding = crate::scan::padding(offset);
    let base = offset + 1 + padding;
    let low = read_i32(code, base + 4)?;
    let high = read_i32(code, base + 8)?;
    let count = high
        .checked_sub(low)
        .and_then(|distance| distance.checked_add(1))
        .context("invalid tableswitch range")?;
    u32::try_from(count).context("negative tableswitch branch count")
}

fn lookupswitch_non_default_branch_count(code: &[u8], offset: usize) -> Result<u32> {
    let padding = crate::scan::padding(offset);
    let base = offset + 1 + padding;
    let npairs = read_i32(code, base + 4)?;
    u32::try_from(npairs).context("negative lookupswitch pair count")
}

fn read_i32(code: &[u8], offset: usize) -> Result<i32> {
    let value = crate::scan::read_u32(code, offset)?;
    Ok(i32::from_be_bytes(value.to_be_bytes()))
}

/// Stable method identity used for deduplication and deterministic ordering.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct MethodIdentity {
    class_name: String,
    method_name: String,
    descriptor: String,
}

impl MethodIdentity {
    fn new(class_name: String, method_name: String, descriptor: String) -> Self {
        Self {
            class_name,
            method_name,
            descriptor,
        }
    }
}

/// Internal finding payload before conversion into SARIF results.
#[derive(Clone, Debug)]
struct LocalComplexityFinding {
    identity: MethodIdentity,
    complexity: u32,
    line: Option<u32>,
    artifact_uri: Option<String>,
}

#[cfg(test)]
mod tests {
    use crate::descriptor::method_param_count;
    use crate::engine::build_context;
    use crate::ir::{
        Class, ControlFlowGraph, ExceptionHandler, LineNumber, Method, MethodAccess, MethodNullness,
    };
    use crate::test_harness::{JvmTestHarness, Language, SourceFile};

    use super::*;

    fn complexity_messages(sources: &[SourceFile]) -> Vec<String> {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let output = harness
            .compile_and_analyze(Language::Java, sources, &[])
            .expect("run harness analysis");
        output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some(RULE_ID))
            .filter_map(|result| result.message.text.clone())
            .collect()
    }

    fn class_with_methods(name: &str, methods: Vec<Method>) -> Class {
        Class {
            name: name.to_string(),
            source_file: None,
            super_name: None,
            interfaces: Vec::new(),
            type_parameters: Vec::new(),
            referenced_classes: Vec::new(),
            fields: Vec::new(),
            methods,
            annotation_defaults: Vec::new(),
            artifact_index: 0,
            is_record: false,
        }
    }

    fn method_with(name: &str, access: MethodAccess, bytecode: Vec<u8>, catch_handlers: usize) -> Method {
        Method {
            name: name.to_string(),
            descriptor: "()V".to_string(),
            signature: None,
            access,
            nullness: MethodNullness::unknown(method_param_count("()V").expect("param count")),
            type_use: None,
            bytecode,
            line_numbers: vec![LineNumber {
                start_pc: 0,
                line: 1,
            }],
            cfg: ControlFlowGraph {
                blocks: Vec::new(),
                edges: Vec::new(),
            },
            calls: Vec::new(),
            string_literals: Vec::new(),
            exception_handlers: (0..catch_handlers)
                .map(|_| ExceptionHandler {
                    start_pc: 0,
                    end_pc: 1,
                    handler_pc: 0,
                    catch_type: Some("java/lang/RuntimeException".to_string()),
                })
                .collect(),
            local_variable_types: Vec::new(),
        }
    }

    fn access_flags(
        is_abstract: bool,
        is_synthetic: bool,
        is_bridge: bool,
    ) -> MethodAccess {
        MethodAccess {
            is_public: true,
            is_static: false,
            is_abstract,
            is_synthetic,
            is_bridge,
        }
    }

    fn bytecode_with_if_count(if_count: usize) -> Vec<u8> {
        let mut bytecode = Vec::with_capacity(if_count * 3 + 1);
        for _ in 0..if_count {
            bytecode.extend_from_slice(&[opcodes::IFEQ, 0, 0]);
        }
        bytecode.push(opcodes::RETURN);
        bytecode
    }

    fn logical_name(result: &SarifResult) -> String {
        result
            .locations
            .as_ref()
            .and_then(|locations| locations.first())
            .and_then(|location| location.logical_locations.as_ref())
            .and_then(|logical_locations| logical_locations.first())
            .and_then(|logical| logical.name.as_ref())
            .cloned()
            .expect("logical location name")
    }

    #[test]
    fn reports_method_above_threshold() {
        let sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
public class ClassA {
    public void methodX(int varOne) {
        if (varOne > 0) { }
        if (varOne > 1) { }
        if (varOne > 2) { }
        if (varOne > 3) { }
        if (varOne > 4) { }
        if (varOne > 5) { }
        if (varOne > 6) { }
        if (varOne > 7) { }
        if (varOne > 8) { }
        if (varOne > 9) { }
    }
}
"#
            .to_string(),
        }];

        let messages = complexity_messages(&sources);

        assert_eq!(messages.len(), 1);
        assert_eq!(
            messages[0],
            "Method complexity 11 exceeds local threshold 10 in com/example/ClassA.methodX(I)V; simplify control flow or split this method."
        );
    }

    #[test]
    fn does_not_report_method_at_boundary() {
        let sources = vec![SourceFile {
            path: "com/example/ClassB.java".to_string(),
            contents: r#"
package com.example;
public class ClassB {
    public void methodX(int varOne) {
        if (varOne > 0) { }
        if (varOne > 1) { }
        if (varOne > 2) { }
        if (varOne > 3) { }
        if (varOne > 4) { }
        if (varOne > 5) { }
        if (varOne > 6) { }
        if (varOne > 7) { }
        if (varOne > 8) { }
    }
}
"#
            .to_string(),
        }];

        let messages = complexity_messages(&sources);

        assert!(
            messages.is_empty(),
            "did not expect findings at the strict boundary: {messages:?}"
        );
    }

    #[test]
    fn counts_catch_handlers_as_decisions() {
        let sources = vec![SourceFile {
            path: "com/example/ClassD.java".to_string(),
            contents: r#"
package com.example;
public class ClassD {
    public void methodX(int varOne) {
        if (varOne > 0) { }
        if (varOne > 1) { }
        if (varOne > 2) { }
        if (varOne > 3) { }
        if (varOne > 4) { }
        if (varOne > 5) { }
        if (varOne > 6) { }
        if (varOne > 7) { }
        if (varOne > 8) { }
        try {
            methodY();
        } catch (IllegalArgumentException varTwo) {
        } catch (IllegalStateException varThree) {
        }
    }
    private void methodY() {
    }
}
"#
            .to_string(),
        }];

        let messages = complexity_messages(&sources);

        assert!(
            messages.iter().any(|message| message == "Method complexity 12 exceeds local threshold 10 in com/example/ClassD.methodX(I)V; simplify control flow or split this method."),
            "expected complexity increase from catch handlers, got {messages:?}"
        );
    }

    #[test]
    fn counts_non_default_switch_branches() {
        let sources = vec![SourceFile {
            path: "com/example/ClassE.java".to_string(),
            contents: r#"
package com.example;
public class ClassE {
    public void methodX(int varOne) {
        switch (varOne) {
            case 1: break;
            case 2: break;
            case 3: break;
            case 4: break;
            case 5: break;
            case 6: break;
            case 7: break;
            case 8: break;
            case 9: break;
            case 10: break;
            default: break;
        }
    }
}
"#
            .to_string(),
        }];

        let messages = complexity_messages(&sources);

        assert!(
            messages.iter().any(|message| message == "Method complexity 11 exceeds local threshold 10 in com/example/ClassE.methodX(I)V; simplify control flow or split this method."),
            "expected switch branches to be counted, got {messages:?}"
        );
    }

    #[test]
    fn suppress_warnings_does_not_change_behavior() {
        let sources = vec![SourceFile {
            path: "com/example/ClassF.java".to_string(),
            contents: r#"
package com.example;
public class ClassF {
    @SuppressWarnings("codex_local_complexity_guard")
    @Deprecated
    public void methodX(int varOne) {
        if (varOne > 0) { }
        if (varOne > 1) { }
        if (varOne > 2) { }
        if (varOne > 3) { }
        if (varOne > 4) { }
        if (varOne > 5) { }
        if (varOne > 6) { }
        if (varOne > 7) { }
        if (varOne > 8) { }
        if (varOne > 9) { }
    }
}
"#
            .to_string(),
        }];

        let messages = complexity_messages(&sources);

        assert_eq!(messages.len(), 1);
        assert!(
            messages[0].contains("com/example/ClassF.methodX(I)V"),
            "expected finding despite suppression annotation, got {messages:?}"
        );
    }

    #[test]
    fn skips_synthetic_bridge_and_non_executable_methods() {
        let concrete = method_with(
            "methodA",
            access_flags(false, false, false),
            bytecode_with_if_count(10),
            0,
        );
        let synthetic = method_with(
            "methodB",
            access_flags(false, true, false),
            bytecode_with_if_count(10),
            0,
        );
        let bridge = method_with(
            "methodC",
            access_flags(false, false, true),
            bytecode_with_if_count(10),
            0,
        );
        let abstract_method = method_with("methodD", access_flags(true, false, false), Vec::new(), 0);

        let context = build_context(
            vec![class_with_methods(
                "com/example/ClassG",
                vec![synthetic, bridge, abstract_method, concrete],
            )],
            &[],
        );

        let results = CodexLocalComplexityGuardRule
            .run(&context)
            .expect("rule execution");

        assert_eq!(results.len(), 1);
        assert_eq!(logical_name(&results[0]), "com/example/ClassG.methodA()V");
    }

    #[test]
    fn findings_are_sorted_by_method_identity() {
        let class_b = class_with_methods(
            "com/example/ClassB",
            vec![method_with(
                "methodY",
                access_flags(false, false, false),
                bytecode_with_if_count(10),
                0,
            )],
        );
        let class_a = class_with_methods(
            "com/example/ClassA",
            vec![method_with(
                "methodX",
                access_flags(false, false, false),
                bytecode_with_if_count(20),
                0,
            )],
        );

        let context = build_context(vec![class_b, class_a], &[]);
        let results = CodexLocalComplexityGuardRule
            .run(&context)
            .expect("rule execution");

        let names = results.iter().map(logical_name).collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "com/example/ClassA.methodX()V",
                "com/example/ClassB.methodY()V"
            ]
        );
    }

    #[test]
    fn rerun_is_deterministic() {
        let sources = vec![SourceFile {
            path: "com/example/ClassH.java".to_string(),
            contents: r#"
package com.example;
public class ClassH {
    public void methodX(int varOne) {
        if (varOne > 0) { }
        if (varOne > 1) { }
        if (varOne > 2) { }
        if (varOne > 3) { }
        if (varOne > 4) { }
        if (varOne > 5) { }
        if (varOne > 6) { }
        if (varOne > 7) { }
        if (varOne > 8) { }
        if (varOne > 9) { }
    }
}
"#
            .to_string(),
        }];

        let first = complexity_messages(&sources);
        let second = complexity_messages(&sources);

        assert_eq!(first, second);
    }
}
