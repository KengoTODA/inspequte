use std::collections::{HashMap, HashSet};

use anyhow::Result;
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::descriptor::{method_param_slots, method_param_start_slots};
use crate::engine::AnalysisContext;
use crate::ir::{Class, InstructionKind, Method};
use crate::opcodes;
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

/// Rule that detects unused lambda parameters in Java and Kotlin lambda expressions.
#[derive(Default)]
pub(crate) struct UnusedLambdaParametersRule;

crate::register_rule!(UnusedLambdaParametersRule);

impl Rule for UnusedLambdaParametersRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "UNUSED_LAMBDA_PARAMETERS",
            name: "Unused lambda parameter",
            description: "Reports lambda parameters that are never referenced in the lambda body",
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
                    let artifact_uri = context.class_artifact_uri(class);
                    let mut findings = Vec::new();
                    findings.extend(check_java_lambdas(class, artifact_uri.as_deref())?);
                    findings.extend(check_kotlin_inline_lambdas(
                        class,
                        artifact_uri.as_deref(),
                    ));
                    Ok(findings)
                })?;
            results.extend(class_results);
        }
        Ok(results)
    }
}

/// Collect loaded local variable slots within a bytecode offset range.
fn loaded_slots_in_range(method: &Method, start: u32, end: u32) -> HashSet<u16> {
    let mut slots = HashSet::new();
    for block in &method.cfg.blocks {
        for instr in &block.instructions {
            if instr.offset >= start && instr.offset < end {
                if let Some(slot) =
                    load_slot(instr.opcode, &method.bytecode, instr.offset as usize)
                {
                    slots.insert(slot);
                }
            }
        }
    }
    slots
}

/// Extract the local variable slot index from a LOAD instruction, if applicable.
fn load_slot(opcode: u8, bytecode: &[u8], offset: usize) -> Option<u16> {
    match opcode {
        opcodes::ILOAD | opcodes::LLOAD | opcodes::FLOAD | opcodes::DLOAD | opcodes::ALOAD => {
            bytecode.get(offset + 1).map(|&b| b as u16)
        }
        opcodes::ILOAD_0 | opcodes::LLOAD_0 | opcodes::FLOAD_0 | opcodes::DLOAD_0
        | opcodes::ALOAD_0 => Some(0),
        opcodes::ILOAD_1 | opcodes::LLOAD_1 | opcodes::FLOAD_1 | opcodes::DLOAD_1
        | opcodes::ALOAD_1 => Some(1),
        opcodes::ILOAD_2 | opcodes::LLOAD_2 | opcodes::FLOAD_2 | opcodes::DLOAD_2
        | opcodes::ALOAD_2 => Some(2),
        opcodes::ILOAD_3 | opcodes::LLOAD_3 | opcodes::FLOAD_3 | opcodes::DLOAD_3
        | opcodes::ALOAD_3 => Some(3),
        _ => None,
    }
}

/// Check for unused lambda parameters in Java lambda synthetic methods.
///
/// Java lambdas are compiled to private synthetic methods named `lambda$<method>$<n>`.
/// The `invokedynamic` call-site descriptor tells us how many captured variables
/// are passed as leading parameters. The remaining parameters are the actual lambda params.
fn check_java_lambdas(class: &Class, artifact_uri: Option<&str>) -> Result<Vec<SarifResult>> {
    let mut results = Vec::new();

    // Collect invokedynamic info: impl_method_name -> captured_slot_count
    let mut lambda_info: HashMap<String, usize> = HashMap::new();
    for method in &class.methods {
        for block in &method.cfg.blocks {
            for instr in &block.instructions {
                if let InstructionKind::InvokeDynamic {
                    descriptor,
                    impl_method: Some(impl_name),
                } = &instr.kind
                {
                    let captured_slots = method_param_slots(descriptor).unwrap_or(0);
                    lambda_info.insert(impl_name.clone(), captured_slots);
                }
            }
        }
    }

    for method in &class.methods {
        // Find matching invokedynamic info: the method must be referenced as an
        // impl_method by an invokedynamic instruction (covers both Java `lambda$`
        // and Kotlin `$lambda-` naming patterns).
        let Some(&captured_slots) = lambda_info.get(&method.name) else {
            continue;
        };

        // For static methods, params start at slot 0; for instance, slot 0 is `this`
        let base_slot: u16 = if method.access.is_static { 0 } else { 1 };
        let param_slots: Vec<u16> = method_param_start_slots(&method.descriptor)
            .unwrap_or_default()
            .into_iter()
            .map(|s| base_slot + s)
            .filter(|&s| s >= base_slot + captured_slots as u16)
            .collect();

        if param_slots.is_empty() {
            continue;
        }

        let used = loaded_slots_in_range(method, 0, u32::MAX);
        report_unused_slots(
            &mut results,
            class,
            method,
            artifact_uri,
            &param_slots,
            &used,
            0,
        );
    }

    Ok(results)
}

/// Report unused lambda parameters at the given slot indices.
fn report_unused_slots(
    results: &mut Vec<SarifResult>,
    class: &Class,
    method: &Method,
    artifact_uri: Option<&str>,
    param_slots: &[u16],
    used: &HashSet<u16>,
    offset_for_line: u32,
) {
    for &slot in param_slots {
        if used.contains(&slot) || is_unnamed_param(method, slot) {
            continue;
        }
        let line = method.line_for_offset(offset_for_line);
        let location = method_location_with_line(
            &class.name,
            &method.name,
            &method.descriptor,
            artifact_uri,
            line,
        );
        results.push(
            SarifResult::builder()
                .message(result_message(format!(
                    "Unused lambda parameter in {}.{}{}: parameter at index {} is never referenced.",
                    class.name, method.name, method.descriptor, slot
                )))
                .locations(vec![location])
                .build(),
        );
    }
}

/// Check for unused parameters in Kotlin inline lambda bodies.
///
/// Kotlin inline lambdas are merged into the caller method. The compiler inserts
/// `$i$a$` markers in the `LocalVariableTable` to delineate lambda body ranges.
/// Lambda parameters appear as local variables whose scope overlaps the marker range.
fn check_kotlin_inline_lambdas(class: &Class, artifact_uri: Option<&str>) -> Vec<SarifResult> {
    let mut results = Vec::new();

    for method in &class.methods {
        // Find $i$a$ markers in local variables
        let markers: Vec<_> = method
            .local_variables
            .iter()
            .filter(|lv| lv.name.starts_with("$i$a$"))
            .collect();

        for marker in &markers {
            let marker_start = marker.start_pc;
            let marker_end = marker.start_pc + marker.length;

            // Find lambda parameter variables: their scope overlaps the marker range,
            // they are not marker variables, not `this`, and not `$i$f$` markers
            let lambda_params: Vec<_> = method
                .local_variables
                .iter()
                .filter(|lv| {
                    if lv.name.starts_with("$i$a$")
                        || lv.name.starts_with("$i$f$")
                        || lv.name == "this"
                        // Variables from the inlined function itself, not user lambda params
                        || lv.name.ends_with("$iv")
                        || lv.name.starts_with("$this$")
                    {
                        return false;
                    }
                    let lv_start = lv.start_pc;
                    let lv_end = lv.start_pc + lv.length;
                    // The param scope must overlap the marker range
                    lv_start < marker_end && lv_end > marker_start
                        // Lambda params are defined just before their marker (typically
                        // within ~5 bytes). Variables starting much earlier are outer
                        // scope captures, not parameters of this specific lambda.
                        && lv_start <= marker_start
                        && marker_start - lv_start <= 5
                })
                .collect();

            let used = loaded_slots_in_range(method, marker_start, marker_end);

            for param in &lambda_params {
                if param.name == "_" || param.name.starts_with("$noName_") {
                    continue;
                }
                if used.contains(&param.index) {
                    continue;
                }
                let line = method.line_for_offset(marker_start);
                let location = method_location_with_line(
                    &class.name,
                    &method.name,
                    &method.descriptor,
                    artifact_uri,
                    line,
                );
                results.push(
                    SarifResult::builder()
                        .message(result_message(format!(
                            "Unused lambda parameter in {}.{}{}: parameter '{}' is never referenced.",
                            class.name, method.name, method.descriptor, param.name
                        )))
                        .locations(vec![location])
                        .build(),
                );
            }
        }
    }

    results
}

/// Check if a parameter at the given slot is named `_` (intentionally unused).
fn is_unnamed_param(method: &Method, slot: u16) -> bool {
    method
        .local_variables
        .iter()
        .any(|lv| lv.index == slot && (lv.name == "_" || lv.name.starts_with("$noName_")))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::test_harness::{JvmTestHarness, Language, SourceFile};

    fn compile_and_analyze(
        harness: &JvmTestHarness,
        language: Language,
        sources: &[SourceFile],
        classpath: &[PathBuf],
    ) -> crate::engine::EngineOutput {
        harness
            .compile_and_analyze(language, sources, classpath)
            .expect("run harness analysis")
    }

    fn unused_lambda_messages(output: &crate::engine::EngineOutput) -> Vec<String> {
        output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("UNUSED_LAMBDA_PARAMETERS"))
            .filter_map(|result| result.message.text.clone())
            .collect()
    }

    // TP: Java lambda with unused parameter
    #[test]
    fn java_lambda_unused_parameter() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set");
        let sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
import java.util.List;
public class ClassA {
    public void methodX() {
        List.of("varOne").forEach(varTwo -> {
            System.out.println("hello");
        });
    }
}
"#
            .to_string(),
        }];
        let output = compile_and_analyze(&harness, Language::Java, &sources, &[]);
        let messages = unused_lambda_messages(&output);
        assert!(
            !messages.is_empty(),
            "expected finding for unused lambda param, got none"
        );
    }

    // TP: Java lambda with multiple params, one unused
    #[test]
    fn java_lambda_multiple_params_one_unused() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set");
        let sources = vec![SourceFile {
            path: "com/example/ClassB.java".to_string(),
            contents: r#"
package com.example;
import java.util.Map;
public class ClassB {
    public void methodX() {
        Map.of("varOne", "varTwo").forEach((varThree, varFour) -> {
            System.out.println(varFour);
        });
    }
}
"#
            .to_string(),
        }];
        let output = compile_and_analyze(&harness, Language::Java, &sources, &[]);
        let messages = unused_lambda_messages(&output);
        assert_eq!(messages.len(), 1, "expected one finding for unused key param, got: {messages:?}");
    }

    // TN: Java lambda where parameter is used
    #[test]
    fn java_lambda_parameter_used() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set");
        let sources = vec![SourceFile {
            path: "com/example/ClassF.java".to_string(),
            contents: r#"
package com.example;
import java.util.List;
public class ClassF {
    public void methodX() {
        List.of("varOne").forEach(varTwo -> {
            System.out.println(varTwo);
        });
    }
}
"#
            .to_string(),
        }];
        let output = compile_and_analyze(&harness, Language::Java, &sources, &[]);
        let messages = unused_lambda_messages(&output);
        assert!(
            messages.is_empty(),
            "expected no findings when param is used, got: {messages:?}"
        );
    }

    // TN: Regular method with unused parameter (not a lambda)
    #[test]
    fn regular_method_unused_parameter_not_reported() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set");
        let sources = vec![SourceFile {
            path: "com/example/ClassH.java".to_string(),
            contents: r#"
package com.example;
public class ClassH {
    public void methodX(String varOne) {
        System.out.println("hello");
    }
}
"#
            .to_string(),
        }];
        let output = compile_and_analyze(&harness, Language::Java, &sources, &[]);
        let messages = unused_lambda_messages(&output);
        assert!(
            messages.is_empty(),
            "regular methods should not be reported: {messages:?}"
        );
    }

    // TN: Method reference (not reported)
    #[test]
    fn method_reference_not_reported() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set");
        let sources = vec![SourceFile {
            path: "com/example/ClassK.java".to_string(),
            contents: r#"
package com.example;
import java.util.List;
public class ClassK {
    public void methodX() {
        List.of("varOne").forEach(System.out::println);
    }
}
"#
            .to_string(),
        }];
        let output = compile_and_analyze(&harness, Language::Java, &sources, &[]);
        let messages = unused_lambda_messages(&output);
        assert!(
            messages.is_empty(),
            "method references should not be reported: {messages:?}"
        );
    }

    // Edge: Lambda capturing outer variable
    #[test]
    fn java_lambda_capturing_outer_variable() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set");
        let sources = vec![SourceFile {
            path: "com/example/ClassL.java".to_string(),
            contents: r#"
package com.example;
import java.util.List;
public class ClassL {
    public void methodX(String varOne) {
        List.of("varTwo").forEach(varThree -> {
            System.out.println(varOne);
        });
    }
}
"#
            .to_string(),
        }];
        let output = compile_and_analyze(&harness, Language::Java, &sources, &[]);
        let messages = unused_lambda_messages(&output);
        assert_eq!(
            messages.len(),
            1,
            "should report varThree as unused but not varOne: {messages:?}"
        );
    }

    // Edge: Two-argument lambda, only second used
    #[test]
    fn java_lambda_two_args_first_unused() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set");
        let sources = vec![SourceFile {
            path: "com/example/ClassM.java".to_string(),
            contents: r#"
package com.example;
import java.util.Map;
public class ClassM {
    public void methodX() {
        Map.of("varOne", 1).forEach((varTwo, varThree) -> {
            System.out.println(varThree);
        });
    }
}
"#
            .to_string(),
        }];
        let output = compile_and_analyze(&harness, Language::Java, &sources, &[]);
        let messages = unused_lambda_messages(&output);
        assert_eq!(
            messages.len(),
            1,
            "should report first param as unused: {messages:?}"
        );
    }

    // Kotlin tests (require kotlinc)

    #[test]
    fn kotlin_inline_lambda_unused_it() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set");
        let sources = vec![SourceFile {
            path: "ClassC.kt".to_string(),
            contents: r#"
class ClassC {
    fun methodX() {
        listOf("varOne").forEach {
            println("hello")
        }
    }
}
"#
            .to_string(),
        }];
        let output = compile_and_analyze(&harness, Language::Kotlin, &sources, &[]);
        let messages = unused_lambda_messages(&output);
        assert!(
            !messages.is_empty(),
            "expected finding for unused implicit 'it', got none"
        );
    }

    #[test]
    fn kotlin_inline_lambda_parameter_used() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set");
        let sources = vec![SourceFile {
            path: "ClassJ.kt".to_string(),
            contents: r#"
class ClassJ {
    fun methodX() {
        listOf("varOne").forEach { varTwo ->
            println(varTwo)
        }
    }
}
"#
            .to_string(),
        }];
        let output = compile_and_analyze(&harness, Language::Kotlin, &sources, &[]);
        let messages = unused_lambda_messages(&output);
        assert!(
            messages.is_empty(),
            "expected no findings when param is used, got: {messages:?}"
        );
    }

    #[test]
    fn kotlin_inline_lambda_underscore_not_reported() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set");
        let sources = vec![SourceFile {
            path: "ClassK.kt".to_string(),
            contents: r#"
class ClassK {
    fun methodX() {
        listOf("varOne").forEach { _ ->
            println("hello")
        }
    }
}
"#
            .to_string(),
        }];
        let output = compile_and_analyze(&harness, Language::Kotlin, &sources, &[]);
        let messages = unused_lambda_messages(&output);
        assert!(
            messages.is_empty(),
            "underscore params should not be reported: {messages:?}"
        );
    }

    // TP: Kotlin non-inline lambda with implicit `it` never referenced (ClassE)
    #[test]
    fn kotlin_non_inline_lambda_implicit_it_unused() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set");
        let sources = vec![SourceFile {
            path: "ClassE.kt".to_string(),
            contents: r#"
class ClassE {
    fun methodX(block: (String?) -> Unit) {
        block(null)
    }
    fun methodY() {
        methodX { println("hello") }
    }
}
"#
            .to_string(),
        }];
        let output = compile_and_analyze(&harness, Language::Kotlin, &sources, &[]);
        let messages = unused_lambda_messages(&output);
        assert!(
            !messages.is_empty(),
            "expected finding for unused implicit 'it' in non-inline lambda"
        );
    }

    // TP: Kotlin inline lambda with named parameter unused (ClassF)
    #[test]
    fn kotlin_inline_lambda_named_param_unused() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set");
        let sources = vec![SourceFile {
            path: "ClassF.kt".to_string(),
            contents: r#"
class ClassF {
    fun methodX() {
        listOf("varOne").forEach { varTwo ->
            println("hello")
        }
    }
}
"#
            .to_string(),
        }];
        let output = compile_and_analyze(&harness, Language::Kotlin, &sources, &[]);
        let messages = unused_lambda_messages(&output);
        assert!(
            !messages.is_empty(),
            "expected finding for unused named param 'varTwo'"
        );
    }

    // TN: Kotlin lambda with `_` for unused parameter in destructuring (ClassH)
    #[test]
    fn kotlin_destructuring_underscore_not_reported() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set");
        let sources = vec![SourceFile {
            path: "ClassH.kt".to_string(),
            contents: r#"
class ClassH {
    fun methodX() {
        mapOf("varOne" to "varTwo").forEach { (_, varThree) ->
            println(varThree)
        }
    }
}
"#
            .to_string(),
        }];
        let output = compile_and_analyze(&harness, Language::Kotlin, &sources, &[]);
        let messages = unused_lambda_messages(&output);
        assert!(
            messages.is_empty(),
            "destructuring with _ should not be reported: {messages:?}"
        );
    }

    // Edge: Kotlin SAM conversion with unused parameter (ClassO)
    #[test]
    fn kotlin_sam_conversion_unused_param() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set");
        let sources = vec![SourceFile {
            path: "ClassO.kt".to_string(),
            contents: r#"
class ClassO {
    fun interface FuncA {
        fun invoke(varOne: String?)
    }
    fun methodX() {
        val varTwo = FuncA { varThree -> println("hello") }
    }
}
"#
            .to_string(),
        }];
        let output = compile_and_analyze(&harness, Language::Kotlin, &sources, &[]);
        let messages = unused_lambda_messages(&output);
        assert!(
            !messages.is_empty(),
            "expected finding for unused SAM param 'varThree'"
        );
    }

    // Edge: Nested inline lambdas (ClassP)
    #[test]
    fn kotlin_nested_inline_lambdas() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set");
        let sources = vec![SourceFile {
            path: "ClassP.kt".to_string(),
            contents: r#"
class ClassP {
    fun methodX() {
        listOf(listOf("varOne")).forEach { varTwo ->
            varTwo.also { varThree ->
                println("hello")
            }
        }
    }
}
"#
            .to_string(),
        }];
        let output = compile_and_analyze(&harness, Language::Kotlin, &sources, &[]);
        let messages = unused_lambda_messages(&output);
        // varTwo is used as receiver of .also, so only varThree should be reported
        let has_var_three = messages.iter().any(|m| m.contains("varThree"));
        let has_var_two = messages.iter().any(|m| m.contains("varTwo"));
        assert!(
            has_var_three,
            "expected finding for unused 'varThree': {messages:?}"
        );
        assert!(
            !has_var_two,
            "varTwo is used as receiver and should not be reported: {messages:?}"
        );
    }

    // Edge: Java lambda with double-width (long/double) parameter to exercise
    // the 2-slot branch in method_param_slots
    #[test]
    fn java_lambda_long_param_unused() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set");
        let sources = vec![SourceFile {
            path: "com/example/ClassQ.java".to_string(),
            contents: r#"
package com.example;
public class ClassQ {
    @FunctionalInterface
    interface LongConsumer {
        void accept(long varOne);
    }
    public void methodX() {
        LongConsumer varTwo = (varThree) -> {
            System.out.println("hello");
        };
    }
}
"#
            .to_string(),
        }];
        let output = compile_and_analyze(&harness, Language::Java, &sources, &[]);
        let messages = unused_lambda_messages(&output);
        assert!(
            !messages.is_empty(),
            "expected finding for unused long param, got none"
        );
    }

    // Edge: Java lambda with mixed single/double-width params (int, double, String)
    // where only the double param is unused — verifies correct slot calculation
    #[test]
    fn java_lambda_mixed_width_params() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set");
        let sources = vec![SourceFile {
            path: "com/example/ClassR.java".to_string(),
            contents: r#"
package com.example;
public class ClassR {
    @FunctionalInterface
    interface MixedFunc {
        void accept(int varOne, double varTwo, String varThree);
    }
    public void methodX() {
        MixedFunc varFour = (varFive, varSix, varSeven) -> {
            System.out.println(varFive);
            System.out.println(varSeven);
        };
    }
}
"#
            .to_string(),
        }];
        let output = compile_and_analyze(&harness, Language::Java, &sources, &[]);
        let messages = unused_lambda_messages(&output);
        assert_eq!(
            messages.len(),
            1,
            "expected exactly one finding for unused double param: {messages:?}"
        );
    }

    // TP: Kotlin non-inline lambda with unused named parameter (ClassD)
    #[test]
    fn kotlin_non_inline_lambda_unused() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set");
        // Use nullable param to avoid Kotlin's checkNotNullParameter null check
        // which would load the param even if unused by user code.
        let sources = vec![SourceFile {
            path: "ClassD.kt".to_string(),
            contents: r#"
class ClassD {
    fun methodX(block: (String?) -> Unit) {
        block(null)
    }
    fun methodY() {
        methodX { varTwo -> println("hello") }
    }
}
"#
            .to_string(),
        }];
        let output = compile_and_analyze(&harness, Language::Kotlin, &sources, &[]);
        let messages = unused_lambda_messages(&output);
        assert!(
            !messages.is_empty(),
            "expected finding for unused non-inline lambda param"
        );
    }
}
