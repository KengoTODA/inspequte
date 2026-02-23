use std::collections::BTreeSet;

use anyhow::Result;
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::engine::AnalysisContext;
use crate::ir::{Class, FieldRef, Instruction, InstructionKind, Method};
use crate::opcodes;
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

/// Rule that detects non-atomic read-modify-write updates on volatile fields.
#[derive(Default)]
pub(crate) struct VolatileIncrementNonAtomicRule;

crate::register_rule!(VolatileIncrementNonAtomicRule);

/// Field identity used while matching volatile field update bytecode sequences.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct FieldKey {
    owner: String,
    name: String,
    descriptor: String,
    is_static: bool,
}

/// Candidate finding location for a non-atomic volatile update site.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct UpdateSite {
    field_name: String,
    offset: u32,
}

impl Rule for VolatileIncrementNonAtomicRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "VOLATILE_INCREMENT_NON_ATOMIC",
            name: "Non-atomic update on volatile field",
            description: "Read-modify-write updates on volatile fields can lose concurrent updates",
        }
    }

    fn run(&self, context: &AnalysisContext) -> Result<Vec<SarifResult>> {
        let mut results = Vec::new();
        for class in context.analysis_target_classes() {
            let mut attributes = vec![KeyValue::new("inspequte.class", class.name.clone())];
            if let Some(uri) = context.class_artifact_uri(class) {
                attributes.push(KeyValue::new("inspequte.artifact_uri", uri));
            }
            let class_results =
                context.with_span("scan.class", &attributes, || -> Result<Vec<SarifResult>> {
                    let mut class_results = Vec::new();
                    let volatile_fields = volatile_fields(class);
                    if volatile_fields.is_empty() {
                        return Ok(class_results);
                    }
                    let artifact_uri = context.class_artifact_uri(class);
                    for method in &class.methods {
                        let sites = find_non_atomic_update_sites(method, &volatile_fields);
                        for site in sites {
                            let message = result_message(format!(
                                "Non-atomic update on volatile field '{}' in {}.{}{}; replace with an atomic type or synchronize the update.",
                                site.field_name, class.name, method.name, method.descriptor
                            ));
                            let line = method.line_for_offset(site.offset);
                            let location = method_location_with_line(
                                &class.name,
                                &method.name,
                                &method.descriptor,
                                artifact_uri.as_deref(),
                                line,
                            );
                            class_results.push(
                                SarifResult::builder()
                                    .message(message)
                                    .locations(vec![location])
                                    .build(),
                            );
                        }
                    }
                    Ok(class_results)
                })?;
            results.extend(class_results);
        }
        Ok(results)
    }
}

fn volatile_fields(class: &Class) -> BTreeSet<FieldKey> {
    class
        .fields
        .iter()
        .filter(|field| field.access.is_volatile)
        .map(|field| FieldKey {
            owner: class.name.clone(),
            name: field.name.clone(),
            descriptor: field.descriptor.clone(),
            is_static: field.access.is_static,
        })
        .collect()
}

fn find_non_atomic_update_sites(method: &Method, volatile_fields: &BTreeSet<FieldKey>) -> Vec<UpdateSite> {
    const LOOKBACK_WINDOW: usize = 8;

    let mut instructions: Vec<&Instruction> = method
        .cfg
        .blocks
        .iter()
        .flat_map(|block| block.instructions.iter())
        .collect();
    instructions.sort_by_key(|instruction| instruction.offset);

    let mut seen_offsets = BTreeSet::new();
    let mut sites = Vec::new();

    for (index, instruction) in instructions.iter().enumerate() {
        let Some(write_field) = write_field_key(instruction, volatile_fields) else {
            continue;
        };
        if index == 0 || !is_rmw_arithmetic(instructions[index - 1].opcode) {
            continue;
        }
        let start = index.saturating_sub(LOOKBACK_WINDOW);
        let has_matching_read = instructions[start..index]
            .iter()
            .any(|candidate| read_field_key(candidate, volatile_fields) == Some(write_field.clone()));
        if has_matching_read && seen_offsets.insert(instruction.offset) {
            sites.push(UpdateSite {
                field_name: write_field.name.clone(),
                offset: instruction.offset,
            });
        }
    }

    sites.sort_by_key(|site| site.offset);
    sites
}

fn read_field_key(instruction: &Instruction, volatile_fields: &BTreeSet<FieldKey>) -> Option<FieldKey> {
    if instruction.opcode != opcodes::GETFIELD && instruction.opcode != opcodes::GETSTATIC {
        return None;
    }
    let field = instruction_field(instruction)?;
    if volatile_fields.contains(&field) {
        Some(field)
    } else {
        None
    }
}

fn write_field_key(instruction: &Instruction, volatile_fields: &BTreeSet<FieldKey>) -> Option<FieldKey> {
    if instruction.opcode != opcodes::PUTFIELD && instruction.opcode != opcodes::PUTSTATIC {
        return None;
    }
    let field = instruction_field(instruction)?;
    if volatile_fields.contains(&field) {
        Some(field)
    } else {
        None
    }
}

fn instruction_field(instruction: &Instruction) -> Option<FieldKey> {
    let InstructionKind::FieldAccess(FieldRef {
        owner,
        name,
        descriptor,
    }) = &instruction.kind
    else {
        return None;
    };
    Some(FieldKey {
        owner: owner.clone(),
        name: name.clone(),
        descriptor: descriptor.clone(),
        is_static: instruction.opcode == opcodes::GETSTATIC || instruction.opcode == opcodes::PUTSTATIC,
    })
}

fn is_rmw_arithmetic(opcode: u8) -> bool {
    matches!(
        opcode,
        opcodes::IADD
            | opcodes::LADD
            | opcodes::FADD
            | opcodes::DADD
            | opcodes::ISUB
            | opcodes::LSUB
            | opcodes::FSUB
            | opcodes::DSUB
            | opcodes::IMUL
            | opcodes::LMUL
            | opcodes::FMUL
            | opcodes::DMUL
            | opcodes::IDIV
            | opcodes::LDIV
            | opcodes::FDIV
            | opcodes::DDIV
            | opcodes::IREM
            | opcodes::LREM
            | opcodes::FREM
            | opcodes::DREM
            | opcodes::ISHL
            | opcodes::LSHL
            | opcodes::ISHR
            | opcodes::LSHR
            | opcodes::IUSHR
            | opcodes::LUSHR
            | opcodes::IAND
            | opcodes::LAND
            | opcodes::IOR
            | opcodes::LOR
            | opcodes::IXOR
            | opcodes::LXOR
    )
}

#[cfg(test)]
mod tests {
    use crate::engine::EngineOutput;
    use crate::test_harness::{JvmTestHarness, Language, SourceFile};

    fn rule_messages(output: &EngineOutput) -> Vec<String> {
        output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("VOLATILE_INCREMENT_NON_ATOMIC"))
            .filter_map(|result| result.message.text.clone())
            .collect()
    }

    fn analyze_java(source: SourceFile) -> Vec<String> {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let output = harness
            .compile_and_analyze(Language::Java, &[source], &[])
            .expect("run harness analysis");
        rule_messages(&output)
    }

    #[test]
    fn reports_volatile_increment() {
        let messages = analyze_java(SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;

class ClassA {
    private volatile int varOne = 0;

    void methodOne() {
        varOne++;
    }
}
"#
            .to_string(),
        });

        assert_eq!(messages.len(), 1);
        assert!(messages[0].contains("volatile field 'varOne'"));
        assert!(messages[0].contains("atomic type or synchronize"));
    }

    #[test]
    fn does_not_report_plain_assignment_on_volatile() {
        let messages = analyze_java(SourceFile {
            path: "com/example/ClassB.java".to_string(),
            contents: r#"
package com.example;

class ClassB {
    private volatile int varOne = 0;

    void methodOne(int varTwo) {
        varOne = varTwo;
    }
}
"#
            .to_string(),
        });

        assert!(messages.is_empty(), "expected no finding, got {messages:?}");
    }

    #[test]
    fn does_not_report_increment_on_non_volatile() {
        let messages = analyze_java(SourceFile {
            path: "com/example/ClassC.java".to_string(),
            contents: r#"
package com.example;

class ClassC {
    private int varOne = 0;

    void methodOne() {
        varOne++;
    }
}
"#
            .to_string(),
        });

        assert!(messages.is_empty(), "expected no finding, got {messages:?}");
    }

    #[test]
    fn reports_post_increment_expression_once() {
        let messages = analyze_java(SourceFile {
            path: "com/example/ClassD.java".to_string(),
            contents: r#"
package com.example;

class ClassD {
    private volatile int varOne = 0;

    int methodOne() {
        int tmpValue = varOne++;
        return tmpValue;
    }
}
"#
            .to_string(),
        });

        assert_eq!(messages.len(), 1, "expected one finding, got {messages:?}");
    }

    #[test]
    fn reports_volatile_decrement() {
        let messages = analyze_java(SourceFile {
            path: "com/example/ClassE.java".to_string(),
            contents: r#"
package com.example;

class ClassE {
    private volatile int varOne = 10;

    void methodOne() {
        varOne--;
    }
}
"#
            .to_string(),
        });

        assert_eq!(messages.len(), 1, "expected one finding, got {messages:?}");
        assert!(messages[0].contains("volatile field 'varOne'"));
    }

    #[test]
    fn reports_volatile_compound_assignment() {
        let messages = analyze_java(SourceFile {
            path: "com/example/ClassF.java".to_string(),
            contents: r#"
package com.example;

class ClassF {
    private static volatile long varOne = 0L;

    static void methodOne(long varTwo) {
        varOne += varTwo;
    }
}
"#
            .to_string(),
        });

        assert_eq!(messages.len(), 1, "expected one finding, got {messages:?}");
        assert!(messages[0].contains("atomic type or synchronize"));
    }
}
