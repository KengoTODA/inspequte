---
name: inspequte-rule-verify
description: Perform isolated, file-based verification of an inspequte rule change using verify-input/. Use when producing a structured go/no-go verification result from spec.md, patch/diff, and source-bound report files without reading plan.md or chat logs.
---

# inspequte rule verify

## Required Input Directory
- `verify-input/`

Required files:
- `verify-input/spec.md`
- `verify-input/diff.patch` (or equivalent change set)
- `verify-input/reports/*` (test/build/audit evidence)
- `verify-input/manifest.json`

Optional but recommended:
- `verify-input/changes/*`
- `verify-input/changed-files.txt`

## Isolation Policy
- Verify must only use `spec.md`, change set (`diff.patch`), and report files.
- Do not read `src/rules/<rule-id>/plan.md`.
- Do not use implementation discussion logs, chat context, or author intent.
- Do not semantically verify evidence unless `scripts/validate-verify-input.sh` succeeds before this isolated phase starts.
- If required input files are missing, return the failure to the workflow; do not invent evidence.

## Output
- Write the normative result to `verify-input/verify-result.json` using
  `schemas/rule-authoring/verification-result.schema.json`.
- Copy the SHA-256 of the exact `verify-input/manifest.json` bytes into
  `evidenceManifestSha256`.
- Use only the closed reason taxonomy documented in
  `docs/rule-authoring-contract.md`.
- Every finding must cite one or more existing files relative to
  `verify-input/`.
- Run `scripts/finalize-verify-result.sh` after writing JSON. It validates the
  result and generates `verify-input/verify-report.md`; do not write the
  Markdown report manually.

## Minimal Context Loading
1. Read only files under `verify-input/` while making the semantic decision.
2. Avoid reading the broader repository unless a missing required file blocks verification.

## Definition of Done
- `verify-result.json` passes `scripts/finalize-verify-result.sh`.
- Every finding cites concrete evidence from files inside `verify-input/`.
- Recommendation, reason, and `implementationRetryable` are consistent.
- The generated report contains all required human-readable sections.
- Report does not reference `plan.md` or discussion history.
- Report calls out deviations from policy, including any `@Suppress` suppression behavior or non-JSpecify annotation semantics introduced without an explicit spec change.
