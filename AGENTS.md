# AGENTS

## Project context
- Repo: inspequte (CLI command: `inspequte`).
- Purpose: fast, CLI-first static analysis for JVM class/JAR files.
- Output: SARIF v2.1.0 only.
- License: AGPL-3.0.
- Commit style: Conventional Commits v1.0.0.

## Decisions
- Add documentation comments to each struct.
- Use Java 21 for the test harness via `JAVA_HOME`.
- Use release-please with crates.io trusted publisher (OIDC).
- Run `cargo fmt` after each code modification.
- Prefer simplicity over backward compatibility; avoid compatibility shims or fallback paths unless they come for free without adding complexity.
- Span naming convention: `scope.action` (e.g., `scan.jar`, `scan.class`).
- Rule messages for users must be intuitive and actionable: clearly state the problem and what to do to fix it.
- Do not support suppression annotations (for example `@Suppress` / `@SuppressWarnings`) as a findings control mechanism.
- Support only JSpecify annotations for annotation-driven rule semantics; treat non-JSpecify annotations as unsupported unless explicitly specified in the rule spec.

## Test Harness Guidelines
- Use meaningless, generic names for variables and classes in test harness Java code.
- Avoid using the same names as examples provided in issues or documentation.
- Prefer names like: `ClassA`, `ClassB`, `MethodX`, `MethodY`, `varOne`, `varTwo`, `tmpValue`.
- This prevents tests from accidentally passing due to name-based matching with user examples.
- Exception: use meaningful names when testing actual JDK or library APIs (e.g., `String`, `List`, `Map`).

## Current scaffold
- `README.md` includes goals, planned analyses, CLI usage, SARIF example, and CI snippet.
- `CONTRIBUTING.md` covers Conventional Commits and AGPL contribution terms.
- `MILESTONES.md` tracks milestones.
- `Cargo.toml` declares the crate and `inspequte` binary.
- `.github/workflows/ci.yml` builds, tests, and uploads release artifacts.
- `plans/` directory contains implementation plans for future features.

## Working with Plans
- Review `plans/` directory for detailed implementation specifications before starting new features.
- **When you complete a feature implementation, rename the corresponding plan file with a `.done.md` suffix.**
  - Example: `01-foo.md` → `01.foo.done.md`
- This marks completed work while preserving the implementation history.
- **Add a short post-mortem to completed plan files.**
  - Include 2-3 bullets: what went well, what was tricky, and any follow-ups.

## Release checklist
- Always run `cargo test`.
- Verify with `cargo build`, `cargo test`, and `cargo audit --format sarif`.
- Install `cargo-audit` as needed via `cargo install cargo-audit --locked` before running `cargo audit`.
- Add/maintain SARIF sanity checks via tests.
- Document any new telemetry attributes.
- Update `README.md`/`CONTRIBUTING.md` if CLI behavior or rule coverage changed.
