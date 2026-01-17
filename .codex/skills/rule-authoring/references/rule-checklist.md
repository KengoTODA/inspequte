# Rule checklist

- Add new rule file in `src/rules/` if needed.
- Add `RuleMetadata` with stable `id`/`name`/`description`.
- Use `method_location_with_line` or `class_location` for SARIF locations.
- Add harness tests in the same rule file with `JvmTestHarness`.
- Register the rule in `src/rules/mod.rs` and `src/engine.rs`.
- Keep results deterministic (stable ordering, no hash map iteration order).
- Add doc comments to any new structs.
