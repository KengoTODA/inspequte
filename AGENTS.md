# Project guidance

inspequte is a fast, CLI-first static analyzer for JVM class/JAR files.
Its output is SARIF v2.1.0. License: AGPL-3.0. Use Conventional Commits.

## Shared constraints
- Prefer simplicity over backward compatibility; avoid compatibility shims that add complexity.
- Add documentation comments to structs.
- Name tracing spans `scope.action` and document new telemetry attributes.
- Keep user-facing messages intuitive and actionable.

## Development and validation
- Use Java 21 via `JAVA_HOME` for JVM harness tests; see `.java-version`.
- Run `cargo fmt` before validation or handoff, rather than after each edit.
- During development, run tests appropriate to the changed behavior. Expand testing for shared analysis changes, failures, or unresolved risks.
- Complete required checks before acceptance. Reuse results for unchanged source and environment; repeat only when changes or failures justify it.
- See `docs/development-validation.md` for checks and evidence ownership.

## Context and durable records
- Read relevant existing plans when working on their features; there is no requirement to read all plans.
- Create a plan when complexity, unresolved design choices, or handoff needs justify one. Record significant decisions and follow-ups, not every reasoning step.
- When completing an existing plan, rename it with `.done.md` and record unresolved follow-ups or useful lessons. No fixed post-mortem length is required.
- Update user documentation when externally visible behavior changes.

## Task-specific guidance
- Rule behavior, architecture, and test conventions: `src/rules/AGENTS.md`.
- End-to-end rule authoring: `prompts/authoring-rule.md`; one author retains context through design and implementation, followed by independent review.
- Evidence and independent review: `docs/rule-authoring-contract.md`.
- Contribution and release conventions: `CONTRIBUTING.md`; releases use release-please and crates.io trusted publishing.
- Load the relevant skill for the requested task, not every authoring skill up front.
