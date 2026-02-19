use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::engine::AnalysisContext;
use crate::ir::{CallSite, Method};
use crate::opcodes;
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

/// Rule that detects direct getter calls on Optional APIs.
#[derive(Default)]
pub(crate) struct OptionalGetCallRule;

crate::register_rule!(OptionalGetCallRule);

impl Rule for OptionalGetCallRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "OPTIONAL_GET_CALL",
            name: "Optional direct getter call",
            description: "Optional.get/getAs* can throw when empty",
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
                    let artifact_uri = context.class_artifact_uri(class);
                    for method in &class.methods {
                        let guarded_getter_offsets = guarded_optional_getter_offsets(method)?;
                        for call in &method.calls {
                            if is_optional_getter_call(&call.owner, &call.name, &call.descriptor) {
                                if guarded_getter_offsets.contains(&call.offset) {
                                    continue;
                                }
                                let message = result_message(format!(
                                    "Avoid Optional direct getter in {}.{}{}; use orElse/orElseThrow/ifPresent instead.",
                                    class.name, method.name, method.descriptor
                                ));
                                let line = method.line_for_offset(call.offset);
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
                    }
                    Ok(class_results)
                })?;
            results.extend(class_results);
        }
        Ok(results)
    }
}

fn is_optional_getter_call(owner: &str, name: &str, descriptor: &str) -> bool {
    matches!(
        (owner, name, descriptor),
        ("java/util/Optional", "get", "()Ljava/lang/Object;")
            | ("java/util/OptionalInt", "getAsInt", "()I")
            | ("java/util/OptionalLong", "getAsLong", "()J")
            | ("java/util/OptionalDouble", "getAsDouble", "()D")
    )
}

/// Bytecode instruction metadata needed for local guard tracking.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BytecodeInstruction {
    offset: u32,
    opcode: u8,
    length: usize,
}

/// Bytecode range where an Optional local is guaranteed non-empty.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NonEmptyGuardRange {
    start_offset: u32,
    end_offset: u32,
    local_index: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PresenceCheckKind {
    IsPresent,
    IsEmpty,
}

fn guarded_optional_getter_offsets(method: &Method) -> Result<BTreeSet<u32>> {
    let instructions = collect_instructions(method)?;
    let offset_to_instruction_index: BTreeMap<u32, usize> = instructions
        .iter()
        .enumerate()
        .map(|(index, instruction)| (instruction.offset, index))
        .collect();
    let guard_ranges = collect_non_empty_guard_ranges(method, &instructions)?;

    let mut guarded_offsets = BTreeSet::new();
    for call in &method.calls {
        if !is_optional_getter_call(&call.owner, &call.name, &call.descriptor) {
            continue;
        }
        let Some(instruction_index) = offset_to_instruction_index.get(&call.offset).copied() else {
            continue;
        };
        let Some(local_index) = receiver_local_index(method, &instructions, instruction_index)
        else {
            continue;
        };
        let guarded = guard_ranges.iter().any(|range| {
            range.local_index == local_index
                && call.offset >= range.start_offset
                && call.offset < range.end_offset
                && !has_store_to_local_between(
                    method,
                    &instructions,
                    local_index,
                    range.start_offset,
                    call.offset,
                )
        });
        if guarded {
            guarded_offsets.insert(call.offset);
        }
    }

    Ok(guarded_offsets)
}

fn collect_instructions(method: &Method) -> Result<Vec<BytecodeInstruction>> {
    let mut instructions = Vec::new();
    let mut offset = 0usize;
    while offset < method.bytecode.len() {
        let opcode = method.bytecode[offset];
        let length = crate::scan::opcode_length(&method.bytecode, offset)?;
        instructions.push(BytecodeInstruction {
            offset: offset as u32,
            opcode,
            length,
        });
        offset += length;
    }
    Ok(instructions)
}

fn collect_non_empty_guard_ranges(
    method: &Method,
    instructions: &[BytecodeInstruction],
) -> Result<Vec<NonEmptyGuardRange>> {
    let calls_by_offset: BTreeMap<u32, &CallSite> = method
        .calls
        .iter()
        .map(|call| (call.offset, call))
        .collect();
    let mut ranges = Vec::new();
    for (index, instruction) in instructions.iter().enumerate() {
        if !is_invoke_opcode(instruction.opcode) {
            continue;
        }
        let Some(call) = calls_by_offset.get(&instruction.offset).copied() else {
            continue;
        };
        let Some(kind) = optional_presence_check_kind(call) else {
            continue;
        };
        let Some(local_index) = receiver_local_index(method, instructions, index) else {
            continue;
        };
        let Some(branch) = instructions.get(index + 1) else {
            continue;
        };
        let Some(target_offset) = conditional_branch_target(&method.bytecode, branch)? else {
            continue;
        };
        if let Some(range) = fallthrough_non_empty_guard_range(kind, branch, target_offset) {
            ranges.push(NonEmptyGuardRange {
                start_offset: range.0,
                end_offset: range.1,
                local_index,
            });
        }
    }
    Ok(ranges)
}

fn is_invoke_opcode(opcode: u8) -> bool {
    matches!(
        opcode,
        opcodes::INVOKEVIRTUAL
            | opcodes::INVOKESPECIAL
            | opcodes::INVOKEINTERFACE
            | opcodes::INVOKESTATIC
    )
}

fn optional_presence_check_kind(call: &CallSite) -> Option<PresenceCheckKind> {
    if call.descriptor != "()Z" || !is_optional_owner(&call.owner) {
        return None;
    }
    match call.name.as_str() {
        "isPresent" => Some(PresenceCheckKind::IsPresent),
        "isEmpty" => Some(PresenceCheckKind::IsEmpty),
        _ => None,
    }
}

fn is_optional_owner(owner: &str) -> bool {
    matches!(
        owner,
        "java/util/Optional"
            | "java/util/OptionalInt"
            | "java/util/OptionalLong"
            | "java/util/OptionalDouble"
    )
}

fn receiver_local_index(
    method: &Method,
    instructions: &[BytecodeInstruction],
    instruction_index: usize,
) -> Option<usize> {
    let previous = instructions.get(instruction_index.checked_sub(1)?)?;
    aload_local_index(&method.bytecode, previous)
}

fn aload_local_index(code: &[u8], instruction: &BytecodeInstruction) -> Option<usize> {
    match instruction.opcode {
        opcodes::ALOAD => code
            .get(instruction.offset as usize + 1)
            .copied()
            .map(usize::from),
        opcodes::ALOAD_0..=opcodes::ALOAD_3 => {
            Some((instruction.opcode - opcodes::ALOAD_0) as usize)
        }
        0xc4 => {
            if code.get(instruction.offset as usize + 1).copied() != Some(opcodes::ALOAD) {
                return None;
            }
            crate::scan::read_u16(code, instruction.offset as usize + 2)
                .ok()
                .map(usize::from)
        }
        _ => None,
    }
}

fn astore_local_index(code: &[u8], instruction: &BytecodeInstruction) -> Option<usize> {
    match instruction.opcode {
        opcodes::ASTORE => code
            .get(instruction.offset as usize + 1)
            .copied()
            .map(usize::from),
        opcodes::ASTORE_0..=opcodes::ASTORE_3 => {
            Some((instruction.opcode - opcodes::ASTORE_0) as usize)
        }
        0xc4 => {
            if code.get(instruction.offset as usize + 1).copied() != Some(opcodes::ASTORE) {
                return None;
            }
            crate::scan::read_u16(code, instruction.offset as usize + 2)
                .ok()
                .map(usize::from)
        }
        _ => None,
    }
}

fn conditional_branch_target(
    code: &[u8],
    instruction: &BytecodeInstruction,
) -> Result<Option<u32>> {
    if !matches!(instruction.opcode, opcodes::IFEQ | opcodes::IFNE) {
        return Ok(None);
    }
    let branch = crate::scan::read_u16(code, instruction.offset as usize + 1)?;
    let branch = i16::from_be_bytes(branch.to_be_bytes()) as i32;
    let target = instruction.offset as i32 + branch;
    if target < 0 {
        return Ok(None);
    }
    Ok(Some(target as u32))
}

fn fallthrough_non_empty_guard_range(
    kind: PresenceCheckKind,
    branch: &BytecodeInstruction,
    branch_target: u32,
) -> Option<(u32, u32)> {
    let non_empty_on_fallthrough = matches!(
        (kind, branch.opcode),
        (PresenceCheckKind::IsPresent, opcodes::IFEQ) | (PresenceCheckKind::IsEmpty, opcodes::IFNE)
    );
    if !non_empty_on_fallthrough {
        return None;
    }

    let start_offset = branch.offset + branch.length as u32;
    if start_offset >= branch_target {
        return None;
    }
    Some((start_offset, branch_target))
}

fn has_store_to_local_between(
    method: &Method,
    instructions: &[BytecodeInstruction],
    local_index: usize,
    start_offset: u32,
    end_offset: u32,
) -> bool {
    instructions
        .iter()
        .filter(|instruction| instruction.offset >= start_offset && instruction.offset < end_offset)
        .filter_map(|instruction| astore_local_index(&method.bytecode, instruction))
        .any(|stored| stored == local_index)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::test_harness::{JvmTestHarness, Language, SourceFile};

    fn optional_get_messages(output: &crate::engine::EngineOutput) -> Vec<String> {
        output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("OPTIONAL_GET_CALL"))
            .filter_map(|result| result.message.text.clone())
            .collect()
    }

    fn compile_and_analyze(
        harness: &JvmTestHarness,
        sources: &[SourceFile],
        classpath: &[PathBuf],
    ) -> crate::engine::EngineOutput {
        harness
            .compile_and_analyze(Language::Java, sources, classpath)
            .expect("run harness analysis")
    }

    #[test]
    fn optional_get_call_reports_optional_get() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
import java.util.Optional;
public class ClassA {
    public String methodX() {
        Optional<String> varOne = Optional.empty();
        return varOne.get();
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = optional_get_messages(&output);
        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("Avoid Optional direct getter")),
            "expected OPTIONAL_GET_CALL finding for Optional#get, got {messages:?}"
        );
    }

    #[test]
    fn optional_get_call_reports_optional_int_getter() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassB.java".to_string(),
            contents: r#"
package com.example;
import java.util.OptionalInt;
public class ClassB {
    public int methodY() {
        OptionalInt varOne = OptionalInt.empty();
        return varOne.getAsInt();
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = optional_get_messages(&output);
        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("Avoid Optional direct getter")),
            "expected OPTIONAL_GET_CALL finding for OptionalInt#getAsInt, got {messages:?}"
        );
    }

    #[test]
    fn optional_get_call_ignores_or_else() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassC.java".to_string(),
            contents: r#"
package com.example;
import java.util.Optional;
public class ClassC {
    public String methodZ() {
        Optional<String> varOne = Optional.empty();
        return varOne.orElse("fallback");
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = optional_get_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect OPTIONAL_GET_CALL finding for Optional#orElse: {messages:?}"
        );
    }

    #[test]
    fn optional_get_call_ignores_get_in_is_present_guard() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassD.java".to_string(),
            contents: r#"
package com.example;
import java.util.Optional;
public class ClassD {
    public String methodW(Optional<String> varOne) {
        if (varOne.isPresent()) {
            return varOne.get();
        }
        return "fallback";
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = optional_get_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect OPTIONAL_GET_CALL finding for Optional#get in isPresent guard: {messages:?}"
        );
    }

    #[test]
    fn optional_get_call_reports_get_outside_is_present_guard() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassE.java".to_string(),
            contents: r#"
package com.example;
import java.util.Optional;
public class ClassE {
    public String methodQ(Optional<String> varOne) {
        if (varOne.isPresent()) {
            return "ok";
        }
        return varOne.get();
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = optional_get_messages(&output);
        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("Avoid Optional direct getter")),
            "expected OPTIONAL_GET_CALL finding for Optional#get outside isPresent guard, got {messages:?}"
        );
    }

    #[test]
    fn optional_get_call_ignores_classpath_calls() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let dependency_sources = vec![SourceFile {
            path: "com/example/ClassB.java".to_string(),
            contents: r#"
package com.example;
import java.util.Optional;
public class ClassB {
    public String methodY() {
        Optional<String> varOne = Optional.empty();
        return varOne.get();
    }
}
"#
            .to_string(),
        }];
        let dependency_output = harness
            .compile(Language::Java, &dependency_sources, &[])
            .expect("compile dependency classes");

        let app_sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
public class ClassA {
    public void methodX() {}
}
"#
            .to_string(),
        }];
        let app_output = harness
            .compile(
                Language::Java,
                &app_sources,
                &[dependency_output.classes_dir().to_path_buf()],
            )
            .expect("compile app classes");

        let analysis = harness
            .analyze(
                app_output.classes_dir(),
                &[dependency_output.classes_dir().to_path_buf()],
            )
            .expect("run harness analysis");
        let messages = optional_get_messages(&analysis);
        assert!(
            messages.is_empty(),
            "classpath classes must be out of scope for OPTIONAL_GET_CALL: {messages:?}"
        );
    }
}
