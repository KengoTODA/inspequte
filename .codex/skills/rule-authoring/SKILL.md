---
name: rule-authoring
description: Create or update inspequte analysis rules and harness-based tests. Use when adding new rules, modifying rule metadata, or writing JVM harness tests for rules in src/rules/*.rs.
---

# Rule authoring (inspequte)

## Workflow
1) Define rule metadata: unique `id`, clear `name`, and short `description`.
2) Add `#[derive(Default)]` to the rule struct (required for automatic registration).
3) Add `crate::register_rule!(RuleName);` after the struct declaration to enable automatic discovery.
4) Implement `Rule::run` using `AnalysisContext` and helpers from `crate::rules` (ex: `result_message`, `method_location_with_line`, `class_location`).
5) Add harness tests in the same rule file (`#[cfg(test)]`): compile Java sources with `JvmTestHarness`, analyze, then assert on `rule_id` and message text.
6) Declare the new rule module in `src/rules/mod.rs` (ex: `pub(crate) mod my_new_rule;`).
7) Update SARIF snapshot tests if rule list changes (see `tests/snapshots/` and `INSPEQUTE_UPDATE_SNAPSHOTS=1 cargo test sarif_callgraph_snapshot`).
8) Keep output deterministic (results are sorted by `rule_id`/message; avoid non-deterministic ordering in rule code).

**Note:** Rules are automatically discovered and registered at compile time using the `inventory` crate. No manual registration in `src/engine.rs` is needed.

See `references/rule-checklist.md` for a compact checklist.

## Harness testing
- Use `JvmTestHarness::new()`; it requires `JAVA_HOME` (Java 21).
- Prefer local stub sources over downloading jars.
- Filter SARIF results by `rule_id` for assertions.
- Cover both happy-path and edge cases: include cases that should report, cases that should not report (false positives), and cases that should not miss reports (false negatives).
- Use generic names in Java harness code (ex: `ClassA`, `methodOne`, `varOne`) and avoid names from user examples; keep real JDK/library API names where required.

### Complete rule example with automatic registration
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

### Harness test template
```rust
let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
let sources = vec![SourceFile {
    path: "com/example/Sample.java".to_string(),
    contents: r#"
package com.example;
public class Sample {
    public void run() {
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
- Keep tests in the rule file to avoid a massive shared test module.
- Use ASCII-only edits unless the file already uses Unicode.
- Add doc comments to any new structs.
