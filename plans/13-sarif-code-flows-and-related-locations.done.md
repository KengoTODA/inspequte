# Plan: SARIF Code Flows and Related Locations

## Objective
Expose concise, deterministic, machine-readable evidence paths for rules whose findings depend on multiple program points.

## Background
A single primary location tells users where a finding is reported, but not necessarily why it was reached. Path-sensitive and multi-location rules are easier to trust and remediate when the SARIF result contains a witness such as lock acquisition to blocking wait, resource allocation to an unclosed exit, or nullable origin to dereference.

GitHub Code Scanning and other SARIF viewers can render `codeFlows` and link `relatedLocations`. Coding agents can consume the same evidence rather than reconstructing the analysis from scratch.

## Implementation Approach
- Add documented internal result and evidence structures capable of representing:
  - One primary location.
  - Named related locations.
  - A deterministic ordered witness path.
  - Optional per-step messages and logical locations.
- Serialize the structures to SARIF 2.1.0 `codeFlows`, `threadFlows`, `threadFlowLocations`, and `relatedLocations`.
- Start with `RUN_BLOCKING_REACHABLE_FROM_COROUTINE` and `EXCEPTION_CAUSE_NOT_PRESERVED` as representative path-sensitive or multi-location rules.
- Prefer one minimal and actionable witness per finding instead of serializing every possible path.
- Define deterministic witness selection when multiple equivalent paths exist.
- Add internally configurable limits for path length and total evidence size so evidence cannot create unbounded output. Plan 09 may later provide shared runtime budget configuration, but is not a prerequisite.
- Preserve bytecode-level logical locations when source mapping is unavailable.
- Ensure evidence generation composes with shared analysis budgets and future abstract-domain infrastructure.

## Test Cases
- The coroutine rule links the selected coroutine root and call chain to the `runBlocking` call in execution order.
- The exception rule links the catch-handler entry to the cause-dropping throw.
- Related-location IDs are valid and unique within a result.
- Witness ordering remains deterministic across repeated and parallel runs.
- Multiple candidate witnesses produce the documented minimal deterministic path.
- Missing source mappings fall back to useful logical or bytecode locations.
- Evidence exceeding configured limits is truncated deterministically without suppressing the finding.
- Generated SARIF validates and renders acceptably in a supported viewer.

## Success Criteria
- The engine can represent and serialize primary, related, and ordered flow locations.
- At least two representative rules emit useful witness evidence.
- Generated SARIF validates against the supported SARIF 2.1.0 schema.
- Evidence order and messages are deterministic.
- Tests verify the semantic relationship between the primary location, related locations, and witness steps.
- Configured budgets limit path length and total evidence size without changing whether a finding is reported.
- A fixture or CI artifact demonstrates acceptable rendering in GitHub Code Scanning or another SARIF viewer.

## Dependencies
- Plan 14 for official SARIF schema validation.
- Existing path-sensitive rule implementations from which witness information can be extracted.
- Plan 09 is an optional future integration point for shared runtime budget configuration, not an implementation dependency.

## Complexity Estimate
High

## Post-mortem
- Went well: Both initial rules already computed deterministic multi-point evidence, and `serde-sarif` exposed the required SARIF 2.1.0 structures without adding a dependency.
- Tricky: Evidence had to be retained before each rule flattened its analysis into a primary location, while result-local related-location IDs had to remain clearly separate from stable finding identity.
- Follow-up: Plan 09 can expose the internal evidence limits as shared runtime budgets; additional path-sensitive rules can adopt the same evidence model after they retain suitable witnesses.
