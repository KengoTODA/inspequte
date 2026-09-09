---
name: inspequte-rule-impl
description: Implement an inspequte rule from its spec, including behavior tests and required documentation, while preserving approved detection behavior.
---

# inspequte rule implementation

Use this guidance within the single-author flow in `prompts/authoring-rule.md`.
Do not spawn a new author for this activity. When the user requests only this
standalone task, keep its output scope; end-to-end authoring may continue to other
activities in the same context.

## Inputs
- `src/rules/<rule-id>/spec.md` only, as the rule contract.
- Existing codebase files needed to implement the rule.

## Outputs
- Implementation changes in Rust source files.
- Unit or harness tests covering TP/TN/edge behavior from the spec.
- Minimal docs updates only when behavior or coverage changed.
- `spec.md` must remain unchanged unless explicitly instructed otherwise.

## Relevant Context
1. Read `src/rules/<rule-id>/spec.md`.
2. Read only relevant implementation files (target rule module, shared helpers, targeted tests).
3. Follow dependencies and search more broadly when needed for correctness, feasibility, or reuse.

## Workflow
1. Map each acceptance criterion in `spec.md` to code and test tasks.
2. Ensure acceptance criteria have implementation and test coverage. Persist a concise mapping only when it helps review or handoff; no running reasoning log is required.
3. Implement or update rule metadata with unique `id`, clear `name`, and short `description`.
4. Ensure user-facing findings are intuitive and actionable: explain what is wrong and what to change.
5. Keep rule wiring correct:
   - Add `#[derive(Default)]` to the rule struct.
   - Add `crate::register_rule!(RuleName);` after the rule struct declaration.
   - Implement `Rule::run` with `AnalysisContext` and relevant helpers from `crate::rules` (for example: `result_message`, `method_location_with_line`, `class_location`).
   - Guard class scans with `if !context.is_analysis_target_class(class) { continue; }` to skip classpath-only classes.
6. Add harness tests in the same rule module (`#[cfg(test)]`) using `JvmTestHarness`:
   - Use `JAVA_HOME` pointing to Java 21.
   - Prefer local stub sources; avoid downloading jars.
   - Assert findings by filtering SARIF results with `rule_id`.
   - Cover report and non-report paths (TP/TN/edge, including FP/FN control).
   - Use generic Java names (`ClassA`, `ClassB`, `MethodX`, `varOne`) unless validating real JDK/library APIs.
7. Rule modules are auto-discovered by `build.rs`; do not manually declare them in `src/rules/mod.rs`.
8. If the registered rule set changes, update snapshot expectations (`INSPEQUTE_UPDATE_SNAPSHOTS=1 cargo test sarif_callgraph_snapshot`).
9. Keep output deterministic (stable ordering and IDs; no hash-order dependence).
10. Run `cargo fmt` before validation and focused behavior tests during implementation. The handoff collector owns full build/test/audit evidence; see `docs/development-validation.md`.
11. Run a completeness gate before finalizing:
   - `git diff --name-only` must include rule implementation (`src/rules/<rule-id>/mod.rs`) and tests.
   - New rules require `crate::register_rule!(...)` in their module, not changes to the shared module registry.
   - If diff only contains registration/docs/prompt changes, treat the implementation as incomplete and continue.
12. Optional verify handoff safety check (recommended when preparing CI verify):
   - Run `scripts/prepare-verify-input.sh <rule-id> [base-ref]`.
   - Confirm `verify-input/changed-files.txt` includes the rule implementation file and tests; otherwise stop and regenerate inputs.
   - After build, test, and audit reports plus `reports/command-status.txt` exist, run `scripts/create-verify-manifest.sh` and `scripts/validate-verify-input.sh`.
13. Update docs only when externally visible behavior changed.

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
- Changes to approved detection behavior require explicit authorization; present the proposed spec change and its rationale when needed.
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
- Acceptance checklist items are all mapped to concrete tests.
- `cargo fmt` has been run.
- Relevant development checks pass or failures are reported with evidence. Full acceptance checks are collected once at handoff, per `docs/development-validation.md`.
- Final diff includes real rule implementation and tests (not only rule registration/doc updates).
- `spec.md` is unchanged unless explicitly requested.

Use this Definition of Done as the compact implementation checklist for rule work.
