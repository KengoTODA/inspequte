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
- If required verify files are missing, report blocked status clearly.

## Execution steps
1. Use `inspequte-rule-verify`.
2. Produce console output and `verify-input/verify-report.md`.
3. Required sections:
   - `## Spec compliance findings`
   - `## FP/noise risks`
   - `## Determinism/stability risks`
   - `## Performance and regression concerns`
   - `## Recommendation (Go/No-Go)`

## Final response format
Output briefly:
1. `recommendation: Go|No-Go`
2. `report-path: verify-input/verify-report.md`
3. critical blockers (if any)

---

Values to replace before use:
- `<RULE_ID>`: e.g. `new_rule_example`
