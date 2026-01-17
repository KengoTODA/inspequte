# AGENTS

## Project context
- Repo: inspequte (CLI command: `inspequte`).
- Purpose: fast, CLI-first static analysis for JVM class/JAR files.
- Output: SARIF v2.1.0 only.
- License: AGPL-3.0.
- Commit style: Conventional Commits v1.0.0.

## Decisions
- Do not document multithreading for now.
- Add documentation comments to each struct.
- Use Java 21 for the test harness via `JAVA_HOME`.
- Use release-please with crates.io trusted publisher (OIDC).

## Current scaffold
- `README.md` includes goals, planned analyses, CLI usage, SARIF example, and CI snippet.
- `CONTRIBUTING.md` covers Conventional Commits and AGPL contribution terms.
- `MILESTONES.md` tracks milestones.
- `Cargo.toml` declares the crate and `inspequte` binary.
- `.github/workflows/ci.yml` builds, tests, and uploads release artifacts.
