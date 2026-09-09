# inspequte Rule Implementation Prompt (spec -> impl)

This prompt scopes a standalone task when explicitly requested. For end-to-end
rule work, use `prompts/authoring-rule.md`; its author keeps context across
activities and uses the relevant skill without adopting standalone phase limits.

You are Codex working in this repository root (`.`).
Use the following skill to implement one rule:

- `.codex/skills/inspequte-rule-impl/SKILL.md`

## Inputs
- `rule-id`: `<RULE_ID>`
- `spec-path`: `src/rules/<RULE_ID>/spec.md`

## Non-negotiable rules
- `spec.md` is the contract. Do not change it for implementation convenience.
- Start with relevant files and follow dependencies as needed to make a sound decision.
- Implement only what is required by `spec.md`.

## Execution steps
1. Use `inspequte-rule-impl`.
2. Implement the rule and tests (TP/TN/Edge) against `spec.md`.
3. Run `cargo fmt`.
4. Run the skill completeness gate (`git diff --name-only`) and ensure implementation + tests are included in the diff.
5. If rule registration changes, update snapshots as needed.
6. Do not run verify in this phase.

## Final response format
Output briefly:
1. `rule-id`
2. changed files
3. command results (`cargo fmt`, and any build/test/audit run if executed)

---

Values to replace before use:
- `<RULE_ID>`: e.g. `new_rule_example`
