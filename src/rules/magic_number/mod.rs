use std::collections::HashSet;

use anyhow::Result;
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::engine::AnalysisContext;
use crate::ir::{Class, InstructionKind, Method};
use crate::opcodes;
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

/// Rule that detects magic numbers in method bytecode.
#[derive(Default)]
pub(crate) struct MagicNumberRule;

crate::register_rule!(MagicNumberRule);

impl Rule for MagicNumberRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "MAGIC_NUMBER",
            name: "Magic number",
            description: "Numeric literals used directly in method bodies reduce readability and maintainability; extract them into named constants",
        }
    }

    fn run(&self, context: &AnalysisContext) -> Result<Vec<SarifResult>> {
        let allowlist = build_allowlist();
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
                        if method.access.is_synthetic || method.access.is_bridge {
                            continue;
                        }
                        if method.name == "hashCode" && method.descriptor == "()I" {
                            continue;
                        }

                        scan_method_body(
                            method,
                            &class.name,
                            artifact_uri.as_deref(),
                            &allowlist,
                            &mut class_results,
                        );

                        // Scan lambda bodies reachable via invokedynamic and
                        // attribute any magic numbers to this enclosing method.
                        scan_lambda_bodies(
                            method,
                            class,
                            artifact_uri.as_deref(),
                            &allowlist,
                            &mut class_results,
                        );
                    }
                    Ok(class_results)
                })?;
            results.extend(class_results);
        }
        Ok(results)
    }
}

/// Scan a method body for magic numbers and append findings to `results`.
fn scan_method_body(
    method: &Method,
    class_name: &str,
    artifact_uri: Option<&str>,
    allowlist: &HashSet<i64>,
    results: &mut Vec<SarifResult>,
) {
    let instructions = collect_instructions(method);
    for (idx, inst) in instructions.iter().enumerate() {
        let value_str = match &inst.kind {
            InstructionKind::ConstInt(v) => {
                if is_int_allowlisted(*v, allowlist) {
                    continue;
                }
                format_int(*v)
            }
            InstructionKind::ConstFloat(v) => {
                if is_float_allowlisted(*v) {
                    continue;
                }
                format_float(*v)
            }
            _ => continue,
        };

        if is_array_creation_context(&instructions, idx) {
            continue;
        }
        if is_collection_capacity_context(&instructions, idx) {
            continue;
        }

        let message = result_message(format!(
            "Magic number {} in {}.{}{}",
            value_str, class_name, method.name, method.descriptor
        ));
        let line = method.line_for_offset(inst.offset);
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

/// Find lambda implementation methods reachable from invokedynamic
/// instructions in `method` and scan them for magic numbers, attributing
/// any findings to the enclosing real method.
fn scan_lambda_bodies(
    method: &Method,
    class: &Class,
    artifact_uri: Option<&str>,
    allowlist: &HashSet<i64>,
    results: &mut Vec<SarifResult>,
) {
    let instructions = collect_instructions(method);
    for inst in &instructions {
        let impl_name = match &inst.kind {
            InstructionKind::InvokeDynamic {
                impl_method: Some(name),
                ..
            } => name,
            _ => continue,
        };

        let Some(lambda_method) = class
            .methods
            .iter()
            .find(|m| m.access.is_synthetic && m.name == *impl_name)
        else {
            continue;
        };

        let lambda_instructions = collect_instructions(lambda_method);
        for (idx, lambda_inst) in lambda_instructions.iter().enumerate() {
            let value_str = match &lambda_inst.kind {
                InstructionKind::ConstInt(v) => {
                    if is_int_allowlisted(*v, allowlist) {
                        continue;
                    }
                    format_int(*v)
                }
                InstructionKind::ConstFloat(v) => {
                    if is_float_allowlisted(*v) {
                        continue;
                    }
                    format_float(*v)
                }
                _ => continue,
            };

            if is_array_creation_context(&lambda_instructions, idx) {
                continue;
            }
            if is_collection_capacity_context(&lambda_instructions, idx) {
                continue;
            }

            let message = result_message(format!(
                "Magic number {} in {}.{}{}",
                value_str, class.name, method.name, method.descriptor
            ));
            let line = lambda_method.line_for_offset(lambda_inst.offset);
            let location = method_location_with_line(
                &class.name,
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
}

/// Collected instruction with offset, opcode, and kind from CFG blocks.
struct FlatInstruction {
    offset: u32,
    opcode: u8,
    kind: InstructionKind,
}

/// Flatten all CFG block instructions into a single ordered list.
fn collect_instructions(method: &Method) -> Vec<FlatInstruction> {
    let mut flat = Vec::new();
    for block in &method.cfg.blocks {
        for inst in &block.instructions {
            flat.push(FlatInstruction {
                offset: inst.offset,
                opcode: inst.opcode,
                kind: inst.kind.clone(),
            });
        }
    }
    flat.sort_by_key(|i| i.offset);
    flat
}

/// Build the integer allowlist: -1, 0, 1, 2, powers of two up to 1024,
/// and common bit masks.
fn build_allowlist() -> HashSet<i64> {
    let mut set = HashSet::new();
    // Basic values
    set.insert(-1);
    set.insert(0);
    set.insert(1);
    set.insert(2);
    // Powers of two up to 1024
    let mut p = 4i64;
    while p <= 1024 {
        set.insert(p);
        p *= 2;
    }
    // Common bit masks
    set.insert(0xFF);
    set.insert(0xFFFF);
    set.insert(0xFFFF_FFFF);
    set
}

fn is_int_allowlisted(value: i64, allowlist: &HashSet<i64>) -> bool {
    allowlist.contains(&value)
}

fn is_float_allowlisted(value: f64) -> bool {
    value == 0.0 || value == 1.0
}

/// Check if the next instruction is an array creation opcode.
fn is_array_creation_context(instructions: &[FlatInstruction], idx: usize) -> bool {
    if let Some(next) = instructions.get(idx + 1) {
        matches!(
            next.opcode,
            opcodes::NEWARRAY | opcodes::ANEWARRAY | opcodes::MULTIANEWARRAY
        )
    } else {
        false
    }
}

/// Check if the constant is used as an initial capacity argument for a
/// collection-like type constructor.
fn is_collection_capacity_context(instructions: &[FlatInstruction], idx: usize) -> bool {
    // Look ahead for an invokespecial <init> on a known collection-like type.
    // The pattern is: push_constant, ..., invokespecial Owner.<init>(I)V
    // We look within a small window (up to 4 instructions ahead).
    let limit = (idx + 5).min(instructions.len());
    for i in (idx + 1)..limit {
        if let InstructionKind::Invoke(call) = &instructions[i].kind {
            if call.name == "<init>" && call.descriptor.starts_with("(I)") {
                if is_collection_like_type(&call.owner) {
                    return true;
                }
            }
        }
    }
    false
}

fn is_collection_like_type(owner: &str) -> bool {
    matches!(
        owner,
        "java/lang/StringBuilder"
            | "java/lang/StringBuffer"
            | "java/util/ArrayList"
            | "java/util/LinkedList"
            | "java/util/HashSet"
            | "java/util/LinkedHashSet"
            | "java/util/HashMap"
            | "java/util/LinkedHashMap"
            | "java/util/WeakHashMap"
            | "java/util/IdentityHashMap"
            | "java/util/Hashtable"
            | "java/util/Vector"
            | "java/util/PriorityQueue"
            | "java/util/ArrayDeque"
            | "java/util/concurrent/ConcurrentHashMap"
            | "java/util/concurrent/LinkedBlockingQueue"
            | "java/util/concurrent/ArrayBlockingQueue"
            | "java/util/concurrent/PriorityBlockingQueue"
            | "java/util/concurrent/LinkedBlockingDeque"
    )
}

fn format_int(v: i64) -> String {
    v.to_string()
}

fn format_float(v: f64) -> String {
    if v == v.floor() && v.is_finite() {
        format!("{v:.1}")
    } else {
        v.to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::test_harness::{JvmTestHarness, Language, SourceFile};

    fn magic_number_messages(output: &crate::engine::EngineOutput) -> Vec<String> {
        output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("MAGIC_NUMBER"))
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
    fn reports_non_allowlisted_integer() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
public class ClassA {
    public void methodOne(int varOne) {
        if (varOne > 3600) {
            System.out.println("timeout");
        }
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = magic_number_messages(&output);
        assert!(
            messages.iter().any(|msg| msg.contains("3600")),
            "expected magic number 3600 finding, got {messages:?}"
        );
    }

    #[test]
    fn reports_non_allowlisted_float() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
public class ClassA {
    public double methodOne() {
        return 9.81;
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = magic_number_messages(&output);
        assert!(
            messages.iter().any(|msg| msg.contains("9.81")),
            "expected magic number 9.81 finding, got {messages:?}"
        );
    }

    #[test]
    fn ignores_allowlisted_integers() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
public class ClassA {
    public int methodOne(int varOne) {
        return varOne + 1;
    }
    public int methodTwo(int varOne) {
        return varOne & 0xFF;
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = magic_number_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect findings for allowlisted values: {messages:?}"
        );
    }

    #[test]
    fn ignores_array_creation_size() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
public class ClassA {
    public byte[] methodOne() {
        return new byte[4096];
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = magic_number_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect findings for array creation size: {messages:?}"
        );
    }

    #[test]
    fn ignores_hashcode_method() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
public class ClassA {
    int varOne;
    int varTwo;
    @Override
    public int hashCode() {
        return 31 * varOne + varTwo;
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = magic_number_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect findings inside hashCode(): {messages:?}"
        );
    }

    #[test]
    fn ignores_switch_case_values() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
public class ClassA {
    public void methodOne(int varOne) {
        switch (varOne) {
            case 200: System.out.println("ok"); break;
            case 404: System.out.println("not found"); break;
            default: break;
        }
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = magic_number_messages(&output);
        // Switch case values are embedded in tableswitch/lookupswitch instructions,
        // not pushed via bipush/sipush/ldc, so they should not be reported.
        assert!(
            messages.is_empty(),
            "did not expect findings for switch case values: {messages:?}"
        );
    }

    #[test]
    fn reports_negative_value() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
public class ClassA {
    public boolean methodOne(int varOne) {
        return varOne > -128;
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = magic_number_messages(&output);
        assert!(
            messages.iter().any(|msg| msg.contains("-128")),
            "expected magic number -128 finding, got {messages:?}"
        );
    }

    #[test]
    fn ignores_collection_capacity() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
import java.util.ArrayList;
public class ClassA {
    public void methodOne() {
        ArrayList<String> varOne = new ArrayList<>(50);
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = magic_number_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect findings for collection capacity: {messages:?}"
        );
    }

    #[test]
    fn ignores_bridge_method() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        // A generic interface with a covariant return override produces a
        // synthetic bridge method that contains the same numeric literal.
        let sources = vec![
            SourceFile {
                path: "com/example/Supplier.java".to_string(),
                contents: r#"
package com.example;
public interface Supplier<T> {
    T get();
}
"#
                .to_string(),
            },
            SourceFile {
                path: "com/example/ClassA.java".to_string(),
                contents: r#"
package com.example;
public class ClassA implements Supplier<Integer> {
    @Override
    public Integer get() {
        return 3600;
    }
}
"#
                .to_string(),
            },
        ];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = magic_number_messages(&output);
        // The bridge method `get()Ljava/lang/Object;` delegates to
        // `get()Ljava/lang/Integer;`. Only the real method should report.
        let bridge_findings: Vec<_> = messages
            .iter()
            .filter(|msg| msg.contains("()Ljava/lang/Object;"))
            .collect();
        assert!(
            bridge_findings.is_empty(),
            "did not expect findings in bridge method: {bridge_findings:?}"
        );
        // The real method `get()Ljava/lang/Integer;` still reports the literal.
        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("3600") && msg.contains("()Ljava/lang/Integer;")),
            "expected finding in the real method get()Ljava/lang/Integer;, got {messages:?}"
        );
    }

    #[test]
    fn reports_lambda_magic_number_as_enclosing_method() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        // A lambda capturing a magic number is compiled into a synthetic
        // method (lambda$methodOne$0). The rule should attribute it to the
        // enclosing real method `methodOne`.
        let sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
import java.util.function.IntSupplier;
public class ClassA {
    public IntSupplier methodOne() {
        return () -> 3600;
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = magic_number_messages(&output);
        // The literal 3600 in the lambda is attributed to `methodOne`.
        // No finding should reference the synthetic lambda method name.
        assert_eq!(
            messages,
            vec![
                "Magic number 3600 in com/example/ClassA.methodOne()Ljava/util/function/IntSupplier;"
            ],
        );
    }
}
