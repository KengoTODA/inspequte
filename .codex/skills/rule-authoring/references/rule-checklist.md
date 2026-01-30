# Rule checklist

- Add new rule file in `src/rules/` if needed.
- Add `#[derive(Default)]` to the rule struct.
- Add `crate::register_rule!(RuleName);` after the struct declaration.
- Add `RuleMetadata` with stable `id`/`name`/`description`.
- Use `method_location_with_line` or `class_location` for SARIF locations.
- Add harness tests in the same rule file with `JvmTestHarness`.
- Use generic harness names (ex: `ClassA`, `methodOne`, `varOne`) and avoid names from user examples.
- Declare the module in `src/rules/mod.rs` (ex: `pub(crate) mod my_new_rule;`).
- Keep results deterministic (stable ordering, no hash map iteration order).
- Add doc comments to any new structs.

**Note:** The rule is automatically registered via the `register_rule!` macro. No manual registration in `src/engine.rs` is needed.
