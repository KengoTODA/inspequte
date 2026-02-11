# inspequte Rule Plan Prompt (idea -> plan)

You are Codex working in this repository root (`.`).
Use the following skill to draft one rule plan:

- `.codex/skills/inspequte-rule-plan/SKILL.md`

## Inputs
- `rule-id`: `<RULE_ID>`
- `rule idea`: `<RULE_IDEA_SHORT_TEXT>`

## Non-negotiable rules
- Read only the minimum required files (avoid unnecessary repo-wide scanning).
- Create or update only `src/rules/<RULE_ID>/plan.md` in this phase.
- Do not create or modify `spec.md` in this phase.

## Execution steps
1. Use `inspequte-rule-plan`.
2. Create `src/rules/<RULE_ID>/plan.md` including a short risk checklist.
3. Keep scope, non-goals, determinism constraints, and test strategy explicit.
4. Record annotation policy constraints:
   - no `@Suppress` / `@SuppressWarnings` suppression semantics
   - support only JSpecify for annotation-driven semantics

## Final response format
Output briefly:
1. `rule-id`
2. `plan-path`
3. Any blocking issue (if present)

---

Values to replace before use:
- `<RULE_ID>`: e.g. `new_rule_example`
- `<RULE_IDEA_SHORT_TEXT>`: e.g. `Detect catch blocks that swallow exceptions without handling`
