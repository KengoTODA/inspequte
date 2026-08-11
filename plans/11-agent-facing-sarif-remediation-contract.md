# Plan: Agent-Facing SARIF Remediation Contract

## Objective
Make inspequte's SARIF output a complete remediation contract that a human or coding agent can use to understand, prioritize, and fix a finding without relying on undocumented rule knowledge.

## Background
The current SARIF rule descriptors primarily expose an ID, name, and short description. This identifies a rule, but it does not consistently communicate severity, precision, remediation guidance, or the reasoning an automated consumer needs before modifying source code.

Richer structured metadata also improves GitHub Code Scanning presentation and lets downstream tools prioritize findings without parsing prose. The contract should remain source-oriented: inspequte should explain the expected repair, but it should not emit SARIF `fixes` until it can identify safe and unambiguous source edits from bytecode analysis.

## Implementation Approach
- Define required rule metadata for:
  - Default severity or SARIF level.
  - Precision.
  - Tags.
  - Full description.
  - Actionable remediation guidance.
  - Help Markdown and/or a stable help URI.
- Define which metadata is normative at the rule level and which fields may be overridden for an individual result.
- Extend `RuleMetadata` and SARIF `reportingDescriptor` generation with the agreed fields.
- Use each rule's specification as the single source of truth where practical so specifications, runtime registration, documentation, and SARIF descriptors do not drift.
- Add deterministic validation for required metadata and for intuitive, actionable user-facing messages.
- Document the metadata contract for rule authors and downstream coding agents.
- Explicitly defer SARIF `fixes` until source-level edit ranges and transformations can be proven safe.

## Test Cases
- Every registered rule produces all required descriptor fields.
- Missing severity, precision, remediation guidance, or other required metadata fails validation.
- Invalid metadata values, unsupported severity levels, and empty help content are rejected.
- A representative SARIF fixture contains enough structured information to determine what is wrong, why it matters, and what source-level repair is expected.
- Rule registration order and parallel analysis do not change generated descriptor ordering.
- Existing rule messages remain intuitive and actionable after the metadata migration.

## Success Criteria
- Every registered rule emits the agreed required metadata in `tool.driver.rules[].reportingDescriptor`.
- Severity, precision, tags, full description, and remediation help have documented schemas and semantics.
- Missing or invalid required metadata fails a deterministic test or build-time validation.
- At least one generated SARIF fixture demonstrates the complete agent-facing remediation contract.
- SARIF snapshot and schema tests remain deterministic.
- Rule-authoring documentation explains how to add and review remediation metadata.

## Dependencies
- Existing rule specifications and registration metadata.
- Plan 14 for the normative SARIF schema and validation policy.
- Coordination with Plan 13 for evidence-path presentation in remediation guidance.

## Complexity Estimate
Medium-High
