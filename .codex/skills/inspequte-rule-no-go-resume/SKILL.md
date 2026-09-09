---
name: inspequte-rule-no-go-resume
description: Resume a rule implementation that was marked No-Go in GitHub Actions by importing plan/spec/impl from a PR, completing missing behavior and tests, and marking the rule as implemented in no-go-history.
---

# inspequte rule no-go resume

## Inputs
- Source PR URL (required).
- Optional `rule-id` override only when automatic extraction fails.
- Optional verify run URL or workflow logs with the No-Go reason.

## Outputs
- Restored rule files under `src/rules/<rule-id>/` using PR artifacts as baseline.
- Completed implementation and tests that satisfy `spec.md`.
- Updated `prompts/references/no-go-history.md` entry for the rule with implemented status and remediation notes.

## Minimal Context Loading
1. Read `prompts/references/no-go-history.md` and locate the target `rule-id` entry.
2. Read the imported spec and any available plan after importing from the PR.
3. Read only the target rule implementation and directly related helpers/tests.
4. Read failure evidence only from CI artifacts/logs relevant to the No-Go reason.

## Workflow
1. Resolve `rule-id` from the source PR:
   - first, read PR head branch name and extract the snake_case suffix when it contains a rule identifier.
   - fallback: inspect changed files in the PR and pick the unique path under `src/rules/<rule-id>/`.
   - if still ambiguous, stop and ask for an explicit override.
2. Confirm the source PR contains rule artifacts for `src/rules/<rule-id>/`:
   - `spec.md` (required)
   - `plan.done.md` or `plan.md` (optional)
   - `mod.rs` (optional baseline)
3. Import `spec.md` and any existing `plan.done.md`/`plan.md` from the PR without content edits. Do not create a plan solely to resume a PR.
4. Review the No-Go reason and map missing acceptance criteria to concrete code/test tasks.
5. Implement missing behavior in `src/rules/<rule-id>/mod.rs` and add/extend harness tests for TP/TN/edge.
6. Ensure rule wiring is complete:
   - `#[derive(Default)]` on rule struct
   - `crate::register_rule!(...)`
   - automatic module discovery by `build.rs`; no manual module registration or rule-count updates
   - snapshot refresh when registered rule set changes
7. Collect the required checks once for the repaired source (reuse this evidence at handoff; see `docs/development-validation.md`):
   - `cargo fmt`
   - `cargo build`
   - `JAVA_HOME=<Java 21 path> cargo test`
   - `cargo audit --format sarif`
8. Update `prompts/references/no-go-history.md` for the target entry:
   - keep original No-Go record for traceability
   - append implementation status and remediation notes in this format:
     - `disposition: implemented`
     - `status: implemented (<YYYY-MM-DD>)`
     - `resolution-ref: <commit hash or PR URL>`
     - `actions: <short summary of what was added/fixed and how it was validated>`
9. Run a completeness gate before finalizing:
   - `git diff --name-only` must include target rule implementation and tests.
   - Diff must include the no-go-history update.
   - If diff only touches registration/docs/prompts, continue implementation.

## Guardrails
- `spec.md` is the contract and must not be changed unless explicitly requested.
- Do not use `@Suppress` / `@SuppressWarnings` as suppression semantics.
- Support only JSpecify-driven annotation semantics unless the spec explicitly states otherwise.
- Keep output deterministic (stable ordering, stable findings).

## Definition of Done
- The PR spec and any existing plan are imported and retained as-is.
- No-Go root cause is addressed with concrete implementation and test coverage.
- `prompts/references/no-go-history.md` marks the rule as implemented and records remediation actions.
- `cargo fmt`, `cargo build`, `cargo test`, and `cargo audit --format sarif` succeed, or failures are reported with concrete evidence.
