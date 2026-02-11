# Copilot Instructions for inspequte

## Project context
- Repo: inspequte (CLI command: `inspequte`).
- Purpose: fast, CLI-first static analysis for JVM class/JAR files.
- Output: SARIF v2.1.0 only.
- License: AGPL-3.0.

## Review priorities
- Correctness of rule logic and bytecode handling.
- SARIF schema stability and deterministic output ordering.
- Coverage for false positives and false negatives in rule tests.
- No reliance on external JARs for tests; prefer local stubs.

## Rule review workflow
- When reviewing changes under `src/rules/`, first refer to:
  - `.codex/skills/inspequte-rule-verify/SKILL.md`
- Use the related verification inputs/evidence under:
  - `verify-input/spec.md`
  - `verify-input/diff.patch`
  - `verify-input/reports/*`
  - `verify-input/changes/*` (if present)
  - `verify-input/changed-files.txt` (if present)
- Follow the isolation policy in the skill: for verification, do not read `src/rules/<rule-id>/plan.md` or discussion logs.

## Constraints and tooling
- Use Java 21 for harness tests via `JAVA_HOME`.
- Run `cargo fmt` after code changes.
- Commit style: Conventional Commits v1.0.0.
- Span naming convention: `scope.action` (e.g., `scan.jar`, `scan.class`).

## Test harness code naming
- Use meaningless, generic names for Java harness code.
- Avoid reusing class/method/variable names from user examples.
- Prefer names like: `ClassA`, `ClassB`, `methodOne`, `varOne`, `tmpValue`.
- Exception: use real names for JDK/library APIs (`String`, `List`, `Map`, etc.).

## Rule authoring expectations
- Add doc comments to any new structs.
- Keep harness tests in the same rule file (`#[cfg(test)]`).
- If output shape changes, update SARIF snapshots intentionally.
