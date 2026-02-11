# inspequte Rule Implementation Prompt (spec -> impl)

You are Codex working in this repository root (`.`).
Use the following skill to implement one rule:

- `.codex/skills/inspequte-rule-impl/SKILL.md`

## Inputs
- `rule-id`: `<RULE_ID>`
- `spec-path`: `src/rules/<RULE_ID>/spec.md`

## Non-negotiable rules
- `spec.md` is the contract. Do not change it for implementation convenience.
- Read only the minimum required files (avoid unnecessary repo-wide scanning).
- Implement only what is required by `spec.md`.

## Execution steps
1. Use `inspequte-rule-impl`.
2. Implement the rule and tests (TP/TN/Edge) against `spec.md`.
3. Run `cargo fmt`.
4. If rule registration changes, update snapshots as needed.
5. Do not run verify in this phase.

## Final response format
Output briefly:
1. `rule-id`
2. changed files
3. command results (`cargo fmt`, and any build/test/audit run if executed)

---

Values to replace before use:
- `<RULE_ID>`: e.g. `new_rule_example`
