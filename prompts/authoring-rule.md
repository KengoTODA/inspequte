# inspequte Rule Authoring Orchestration Prompt (subagents + iterative verify)

Use stage-specific prompts in isolated subagents to reduce context mixing:

1. `prompts/ideate-rule.md`
2. `prompts/authoring-plan.md`
3. `prompts/authoring-spec.md`
4. `prompts/authoring-impl.md`
5. `prompts/authoring-verify.md`
6. `prompts/authoring-no-go-resume.md` (for external No-Go resume flows)

## Subagent contract
- Launch one subagent per phase (`ideation`, `plan`, `spec`, `impl`, `verify`).
- Pass only the minimum required inputs to each subagent.
- Carry forward only phase outputs (do not forward full chat logs).
- Treat each phase output as the next phase input contract.

## Recommended sequence
1. Launch `ideation` subagent with `prompts/ideate-rule.md` to get:
   - `rule-id`
   - `rule idea`
   - while referencing `prompts/references/no-go-history.md` to avoid duplicate ideas
2. Launch `plan` subagent with `prompts/authoring-plan.md`.
3. Launch `spec` subagent with `prompts/authoring-spec.md`.
4. Run iterative `impl` <-> `verify` loop (up to 3 iterations):
   - Launch `impl` subagent with `prompts/authoring-impl.md`.
   - Prepare isolated verify inputs:
     - `scripts/prepare-verify-input.sh <RULE_ID> [<BASE_REF_OR_EMPTY>]`
     - `cargo build > verify-input/reports/cargo-build.txt 2>&1`
     - `cargo test > verify-input/reports/cargo-test.txt 2>&1`
     - `cargo audit --format sarif > verify-input/reports/cargo-audit.sarif`
   - Launch `verify` subagent with `prompts/authoring-verify.md` using only `verify-input/`.
   - If recommendation is `Go`, stop looping.
   - If recommendation is `No-Go`, feed `verify-input/verify-report.md` findings into the next `impl` iteration.
5. If still `No-Go` after 3 iterations, stop and surface blockers clearly.
6. Regenerate deterministic rule docs:
   - `scripts/generate-rule-docs.sh`
7. Use `prompts/authoring-no-go-resume.md` only when resuming a prior external No-Go PR.

## Non-negotiable rules
- `spec.md` is the contract.
- Verify must use only files under `verify-input/`.
- Verify must not read `plan.md` or implementation discussion logs.
