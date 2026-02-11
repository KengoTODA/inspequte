# inspequte Rule Spec Prompt (plan -> spec)

You are Codex working in this repository root (`.`).
Use the following skill to write one rule spec:

- `.codex/skills/inspequte-rule-spec/SKILL.md`

## Inputs
- `rule-id`: `<RULE_ID>`
- `rule idea`: `<RULE_IDEA_SHORT_TEXT>`
- `plan-path`: `src/rules/<RULE_ID>/plan.md` (optional but recommended)

## Non-negotiable rules
- `spec.md` is a behavior contract.
- Keep scope contractual; avoid implementation details.
- Read only the minimum required files (avoid unnecessary repo-wide scanning).
- Create or update only `src/rules/<RULE_ID>/spec.md` in this phase.
- Do not implement or verify in this phase.

## Execution steps
1. Use `inspequte-rule-spec`.
2. Create `src/rules/<RULE_ID>/spec.md`.
3. Use this exact heading order:
   - `## Summary`
   - `## Motivation`
   - `## What it detects`
   - `## What it does NOT detect`
   - `## Examples (TP/TN/Edge)`
   - `## Output`
   - `## Performance considerations`
   - `## Acceptance criteria`
4. Keep findings/messages intuitive and actionable.
5. State annotation policy explicitly:
   - `@Suppress`-style suppression unsupported
   - JSpecify-only for annotation-driven semantics

## Final response format
Output briefly:
1. `rule-id`
2. `spec-path`
3. Any blocking issue (if present)

---

Values to replace before use:
- `<RULE_ID>`: e.g. `new_rule_example`
- `<RULE_IDEA_SHORT_TEXT>`: e.g. `Detect catch blocks that swallow exceptions without handling`
