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

## Constraints and tooling
- Use Java 21 for harness tests via `JAVA_HOME`.
- Run `cargo fmt` after code changes.
- Commit style: Conventional Commits v1.0.0.
- Do not document multithreading for now.

## Rule authoring expectations
- Add doc comments to any new structs.
- Keep harness tests in the same rule file (`#[cfg(test)]`).
- If output shape changes, update SARIF snapshots intentionally.
