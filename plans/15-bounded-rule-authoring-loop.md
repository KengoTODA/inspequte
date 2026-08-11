# Plan: Bounded Evidence-Driven Rule Authoring Loop

## Objective
Turn rule authoring into a real, bounded evaluator-optimizer loop that can repair actionable verification failures while preserving independent verification and explicit human gates.

## Background
The authoring prompt describes iterative implementation and verification, but the GitHub Actions workflow effectively performs one implementation pass followed by one verification pass. An actionable No-Go does not reliably route back to the phase that can fix it.

Closing the loop should improve completion rate only if every completion claim is backed by fresh evidence tied to the exact source revision. The workflow also needs explicit retry and authority boundaries so it cannot loop indefinitely or silently revise an approved specification.

## Implementation Approach
- Define structured verification outcomes and reason categories:
  - Implementation defect.
  - Missing or incorrect test.
  - Specification ambiguity.
  - Suspected false positive or rule-design problem.
  - Stale or missing evidence.
  - Infrastructure failure.
- Route each category to the appropriate workflow node:
  - Implementation and test defects return to implementation.
  - Specification ambiguity returns through a human-gated specification revision.
  - False-positive concerns return to rule design/specification and pinned fixtures.
  - Infrastructure failures regenerate evidence according to a documented policy.
- Add a configurable retry budget, initially no more than three implementation/verification iterations.
- Add `verify-input/manifest.json` binding evidence to:
  - Base and head commit SHAs.
  - Diff hash.
  - Changed-file hashes.
  - Commands and exit codes.
  - Tool versions.
  - Timestamps.
- Reject stale, missing, or mismatched evidence mechanically before semantic LLM verification.
- Make a schema-validated JSON verification result normative and generate Markdown summaries from it.
- Add evaluator calibration fixtures with known-good and intentionally defective implementations.
- Measure false-Go and false-No-Go outcomes.
- Preserve telemetry for attempts, routes, duration, evidence identity, and terminal outcome without recording secrets or unnecessary prompt content.
- Retain No-Go branches and artifacts long enough for diagnosis instead of immediately discarding useful evidence.

## Test Cases
- An implementation defect routes back through implementation, evidence collection, and independent verification.
- A specification ambiguity cannot modify the specification without the configured human gate.
- A stale test report from a previous commit is rejected before semantic verification.
- Changed-file or diff hash mismatches are rejected deterministically.
- A Go result terminates the loop and preserves its evidence manifest.
- Three consecutive actionable No-Go outcomes exhaust the default retry budget and escalate.
- Infrastructure failures follow their documented retry policy without consuming semantic repair attempts incorrectly.
- Known-good and intentionally defective calibration fixtures produce their expected outcomes.
- Re-running the workflow for the same inputs produces the same route and machine-readable report, excluding documented volatile fields.

## Success Criteria
- An actionable implementation No-Go automatically triggers another bounded implementation/evidence/verification iteration.
- The loop stops on Go, a human-gate condition, infrastructure policy, or retry-budget exhaustion.
- Verification evidence is cryptographically bound to the exact source state, and stale evidence is rejected deterministically.
- Verification produces a schema-validated machine-readable result with a documented reason taxonomy.
- A calibration suite reports false-Go and false-No-Go behavior for known cases.
- GitHub Actions and the authoring prompt describe the same state transitions and retry limits.
- Workflow telemetry supports comparison of success rate, retries, duration, and terminal reasons over time.
- No-Go branches or artifacts remain available for diagnosis and follow-up under a documented retention policy.

## Dependencies
- Existing rule-authoring workflow, phase prompts, and file-based verification process.
- Existing OpenTelemetry conventions for workflow observability.
- Stable JSON schema and hashing tools available in GitHub Actions.
- Human approval policy for specification changes and retry-budget exhaustion.

## Complexity Estimate
High
