# inspequte Rule No-Go Resume Prompt (No-Go -> completed impl)

You are Codex working in this repository root (`.`).
Use the following skill to resume one rule implementation that failed as `No-Go`:

- `.codex/skills/inspequte-rule-no-go-resume/SKILL.md`

## Inputs
- `source-pr`: `<PR_URL>`

## Non-negotiable rules
- Derive `rule-id` from the source PR (head branch name or changed `src/rules/<rule-id>/` path).
- Import `spec.md` and any existing `plan.done.md`/`plan.md` from the source PR as-is; a plan is optional.
- Fix missing implementation/tests based on the No-Go reason.
- Update `prompts/references/no-go-history.md` for this rule with implemented status and remediation notes.

## Execution steps
1. Use `inspequte-rule-no-go-resume`.
2. Complete implementation and tests against imported `spec.md`.
3. Run `cargo fmt`, `cargo build`, `cargo test`, and `cargo audit --format sarif`.
4. Ensure diff contains implementation/tests and no-go-history updates.

## Final response format
Output briefly:
1. `rule-id`
2. changed files
3. command results (`cargo fmt`, `cargo build`, `cargo test`, `cargo audit --format sarif`)
4. no-go-history update summary

---

Values to replace before use:
- `<PR_URL>`: e.g. `https://github.com/KengoTODA/inspequte/pull/47`
