# inspequte Rule Verify Prompt (verify-input -> recommendation)

You are Codex working in this repository root (`.`).
Use the following skill to verify one rule change:

- `.codex/skills/inspequte-rule-verify/SKILL.md`

## Inputs
- `rule-id`: `<RULE_ID>`
- `verify-input`: `verify-input/`

## Non-negotiable rules
- Verify must use only files under `verify-input/`.
- Verify must not read `plan.md` or implementation discussion logs.
- Evidence validation must have succeeded before this phase. If required verify
  files are missing, stop without inventing evidence.

## Execution steps
1. Use `inspequte-rule-verify`.
2. Write the normative structured result to `verify-input/verify-result.json`.
3. Run `scripts/finalize-verify-result.sh` to validate the JSON and generate
   `verify-input/verify-report.md`.

## Final response format
Output briefly:
1. `result-path: verify-input/verify-result.json`
2. `report-path: verify-input/verify-report.md`
3. whether finalization succeeded

---

Values to replace before use:
- `<RULE_ID>`: e.g. `new_rule_example`
