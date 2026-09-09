# Development validation

During implementation, run focused tests for the affected behavior. Add regression
coverage for real behavior changes (including non-reporting cases for rules).
For shared analysis changes, expand to affected consumers and regression suites.
Format before validation or handoff. Do not repeat checks on unchanged inputs
merely because another prompt lists the same command.

Before accepting a rule implementation or releasing, require `cargo build`,
`cargo test` with Java 21 via `JAVA_HOME`, and `cargo audit --format sarif`.
The handoff evidence collector owns this full set of checks and their reports;
implementation stages need not run the same full set immediately before it.
Install cargo-audit with `cargo install cargo-audit --locked` if needed.
Spec/runtime metadata changes also require `bash scripts/validate-rule-specs.sh`
and regeneration with `bash scripts/generate-rule-docs.sh`.
Documentation-only changes need relevant format/link/generation checks, not JVM tests.

Evidence records must identify source, commands, exit codes, and tool environment.
If source or relevant environment changes, regenerate affected evidence. Report
failed or unavailable checks accurately; never manufacture a passing report.
Pinned representative E2E fixtures are required for new detection behavior or
changes affecting precision/performance; document scope and thresholds in the spec.
No additional E2E run is needed for a documentation-only change.

The legacy authoring GitHub workflows are scheduled for removal and still contain
duplicate checks. This policy governs the replacement/manual authoring flow; it
does not claim those existing workflow steps have been removed.
