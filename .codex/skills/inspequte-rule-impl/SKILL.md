---
name: inspequte-rule-impl
description: Implement an inspequte rule from spec.md, including tests and minimal docs updates. Use when coding src/rules/<rule-id>/mod.rs and related tests while treating spec.md as immutable unless explicitly asked to change it.
---

# inspequte rule implementation

## Inputs
- `src/rules/<rule-id>/spec.md` only, as the rule contract.
- Existing codebase files needed to implement the rule.

## Outputs
- Implementation changes in Rust source files.
- Unit or harness tests covering TP/TN/edge behavior from the spec.
- Minimal docs updates only when behavior or coverage changed.
- `spec.md` must remain unchanged unless explicitly instructed otherwise.

## Minimal Context Loading
1. Read `src/rules/<rule-id>/spec.md`.
2. Read only relevant implementation files (target rule module, shared helpers, targeted tests).
3. Avoid repository-wide scans beyond what is required for compile/test fixes.

## Workflow
1. Map each acceptance criterion in `spec.md` to code and test tasks.
2. Implement or update rule metadata with unique `id`, clear `name`, and short `description`.
3. Ensure user-facing findings are intuitive and actionable: explain what is wrong and what to change.
4. Keep rule wiring correct:
   - Add `#[derive(Default)]` to the rule struct.
   - Add `crate::register_rule!(RuleName);` after the rule struct declaration.
   - Implement `Rule::run` with `AnalysisContext` and relevant helpers from `crate::rules` (for example: `result_message`, `method_location_with_line`, `class_location`).
   - Guard class scans with `if !context.is_analysis_target_class(class) { continue; }` to skip classpath-only classes.
5. Add harness tests in the same rule module (`#[cfg(test)]`) using `JvmTestHarness`:
   - Use `JAVA_HOME` pointing to Java 21.
   - Prefer local stub sources; avoid downloading jars.
   - Assert findings by filtering SARIF results with `rule_id`.
   - Cover report and non-report paths (TP/TN/edge, including FP/FN control).
   - Use generic Java names (`ClassA`, `ClassB`, `MethodX`, `varOne`) unless validating real JDK/library APIs.
6. If adding a brand-new rule module, declare it in `src/rules/mod.rs`.
7. If the registered rule set changes, update snapshot expectations (`INSPEQUTE_UPDATE_SNAPSHOTS=1 cargo test sarif_callgraph_snapshot`).
8. Keep output deterministic (stable ordering and IDs; no hash-order dependence).
9. Run `cargo fmt`, then `cargo build`, `cargo test`, and `cargo audit --format sarif`.
10. Update docs only when externally visible behavior changed.

## Template Snippets
Rule skeleton:
```rust
/// Rule that detects [describe what your rule checks].
#[derive(Default)]
pub(crate) struct MyNewRule;

crate::register_rule!(MyNewRule);

impl Rule for MyNewRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "MY_NEW_RULE",
            name: "My new rule",
            description: "Brief description of what this rule checks",
        }
    }

    fn run(&self, context: &AnalysisContext) -> Result<Vec<SarifResult>> {
        // Implementation here
        Ok(vec![])
    }
}
```

Harness test skeleton:
```rust
let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
let sources = vec![SourceFile {
    path: "com/example/ClassA.java".to_string(),
    contents: r#"
package com.example;
public class ClassA {
    public void methodOne() {
        // code under test
    }
}
"#.to_string(),
}];
let output = harness
    .compile_and_analyze(Language::Java, &sources, &[])
    .expect("run harness analysis");
let messages: Vec<String> = output
    .results
    .iter()
    .filter(|result| result.rule_id.as_deref() == Some("RULE_ID"))
    .filter_map(|result| result.message.text.clone())
    .collect();
assert!(messages.iter().any(|msg| msg.contains("expected")));
```

## Guardrails
- Do not edit `src/rules/<rule-id>/spec.md` by default.
- Any desired behavior change requires a separate spec change request.
- Keep implementation aligned with deterministic and low-noise principles in `src/rules/AGENTS.md`.
- Do not implement annotation-based suppression via `@Suppress` or `@SuppressWarnings`.
- For annotation-driven semantics, support JSpecify only and treat non-JSpecify annotations as unsupported unless the spec explicitly changes.
- Keep tests close to the rule module to avoid a large shared test module.
- Use ASCII-only edits unless a touched file already requires Unicode.
- Add doc comments to new structs.
- Rules are discovered via `inventory`; do not add manual registration in `src/engine.rs`.
- If `cargo audit` is unavailable, install it first with `cargo install cargo-audit --locked`.

## Definition of Done
- Code and tests implement all acceptance criteria from `spec.md`.
- `cargo fmt` has been run.
- `cargo build`, `cargo test`, and `cargo audit --format sarif` pass, or failures are reported with concrete evidence.
- `spec.md` is unchanged unless explicitly requested.

Use this Definition of Done as the compact implementation checklist for rule work.
