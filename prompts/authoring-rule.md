# inspequte Rule Authoring Prompt (plan -> spec -> impl -> verify)

You are Codex working in this repository root (`.`).  
Use the following four skills **in this exact order** to design, implement, and verify one rule.

- `.codex/skills/inspequte-rule-plan/SKILL.md`
- `.codex/skills/inspequte-rule-spec/SKILL.md`
- `.codex/skills/inspequte-rule-impl/SKILL.md`
- `.codex/skills/inspequte-rule-verify/SKILL.md`

## Inputs
- `rule-id`: `<RULE_ID>`
- `rule idea`: `<RULE_IDEA_SHORT_TEXT>`
- `base-ref` (optional): `<BASE_REF_OR_EMPTY>`

## Non-negotiable rules
- `spec.md` is the contract. Do not change it for implementation convenience (if changed, treat it as an explicit spec change).
- Verify must use only files under `verify-input/`.
- Verify must not read `plan.md` or implementation discussion logs.
- Read only the minimum required files (avoid unnecessary repo-wide scanning).

## Execution steps
1. **Plan** (`inspequte-rule-plan`)
   - Input: `rule idea`, `rule-id`
   - Output: `src/rules/<RULE_ID>/plan.md` (including a risk checklist)
   - Do not create `spec.md` in this step.

2. **Spec** (`inspequte-rule-spec`)
   - Input: `rule idea`, `rule-id`, and `plan.md` if needed
   - Output: `src/rules/<RULE_ID>/spec.md`
   - Use this exact heading order:
     - `## Summary`
     - `## Motivation`
     - `## What it detects`
     - `## What it does NOT detect`
     - `## Examples (TP/TN/Edge)`
     - `## Output`
     - `## Performance considerations`
     - `## Acceptance criteria`

3. **Implement** (`inspequte-rule-impl`)
   - Input: implement against `src/rules/<RULE_ID>/spec.md` as the contract.
   - Output: rule implementation, tests (TP/TN/Edge), and minimal docs updates if needed.
   - Run:
     - `cargo fmt`

4. **Generate verify-input**
   - Run:
     - `scripts/prepare-verify-input.sh <RULE_ID> [<BASE_REF_OR_EMPTY>]`
     - `cargo build > verify-input/reports/cargo-build.txt 2>&1`
     - `cargo test > verify-input/reports/cargo-test.txt 2>&1`
     - `cargo audit --format sarif > verify-input/reports/cargo-audit.sarif`

5. **Verify** (`inspequte-rule-verify`)
   - Input: `verify-input/` only.
   - Output:
     - Console output + `verify-input/verify-report.md`
     - Required sections:
       - `## Spec compliance findings`
       - `## FP/noise risks`
       - `## Determinism/stability risks`
       - `## Performance and regression concerns`
       - `## Recommendation (Go/No-Go)`

6. **Regenerate rule docs (deterministic)**
   - `scripts/generate-rule-docs.sh`

## Final report format
At the end, report briefly:
1. Changed files
2. Executed commands and results (build/test/audit/verify)
3. Verify recommendation (Go/No-Go)
4. Remaining issues (if any)

---

Values to replace before use:
- `<RULE_ID>`: e.g. `new_rule_example`
- `<RULE_IDEA_SHORT_TEXT>`: e.g. `Detect catch blocks that swallow exceptions without handling`
- `<BASE_REF_OR_EMPTY>`: e.g. `origin/main` (or empty)
