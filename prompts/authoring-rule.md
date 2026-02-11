# inspequte Rule Authoring Orchestration Prompt (split phases)

Use stage-specific prompts to reduce context mixing:

1. `prompts/ideate-rule.md`
2. `prompts/authoring-plan.md`
3. `prompts/authoring-spec.md`
4. `prompts/authoring-impl.md`
5. `prompts/authoring-verify.md`

## Recommended sequence
1. Run ideation (`prompts/ideate-rule.md`) to get:
   - `rule-id`
   - `rule idea`
2. Run plan (`prompts/authoring-plan.md`) with those inputs.
3. Run spec (`prompts/authoring-spec.md`) with those inputs.
4. Run implementation (`prompts/authoring-impl.md`) based on `spec.md`.
5. Prepare isolated verify inputs:
   - `scripts/prepare-verify-input.sh <RULE_ID> [<BASE_REF_OR_EMPTY>]`
   - `cargo build > verify-input/reports/cargo-build.txt 2>&1`
   - `cargo test > verify-input/reports/cargo-test.txt 2>&1`
   - `cargo audit --format sarif > verify-input/reports/cargo-audit.sarif`
6. Run verify (`prompts/authoring-verify.md`) using only `verify-input/`.
7. Regenerate deterministic rule docs:
   - `scripts/generate-rule-docs.sh`

## Non-negotiable rules
- `spec.md` is the contract.
- Verify must use only files under `verify-input/`.
- Verify must not read `plan.md` or implementation discussion logs.
