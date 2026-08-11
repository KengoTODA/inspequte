# Plan: Stable Finding Identities

## Objective
Give each finding a stable identity that survives non-semantic changes and can be reused by baselines, SARIF consumers, and coding-agent repair workflows.

## Background
Current baseline matching uses the rule ID, full message, and locations. This makes identity sensitive to message improvements, line movement, path normalization, and better source mapping. An existing defect can therefore appear new even though the underlying program point is unchanged.

A stable identity is also necessary to correlate a finding before and after an automated repair attempt. Identity must be separate from presentation: messages and source locations should be allowed to improve without silently changing the semantic finding key.

## Implementation Approach
- Define a versioned identity algorithm based on semantic program coordinates where available:
  - Rule ID.
  - Class internal name.
  - Method name and descriptor.
  - Bytecode offset or another stable instruction/program-point identifier.
  - Finding kind and relevant symbol identity when needed to distinguish results.
- Define deterministic fallbacks for class-level, artifact-level, and other findings without method or bytecode coordinates.
- Separate the identity input from user-facing messages and presentation locations.
- Use the new identity for baseline matching.
- Expose an appropriate representation through SARIF `partialFingerprints` or namespaced properties without claiming to replace GitHub's own `primaryLocationLineHash` behavior.
- Normalize source artifact URIs to repository-relative paths when a source root is known.
- Define algorithm versioning, collision handling, and migration behavior for existing baselines.
- Keep the hash input format documented and deterministic across platforms.

## Test Cases
- The same finding keeps its identity after a message-only change.
- The same finding keeps its identity after unrelated source lines are inserted.
- Semantically distinct findings at different program points receive distinct identities.
- Overloaded methods are distinguished by their descriptors.
- Class-level and artifact-level fallback identities are deterministic.
- Windows-style and Unix-style input paths normalize to the same repository-relative artifact identity where appropriate.
- Existing baseline input receives the documented migration or version-mismatch behavior.
- Repeated parallel runs produce identical identities and result ordering.

## Success Criteria
- A documented, versioned finding identity algorithm is implemented.
- Message-only and unrelated line-shift changes do not create a new identity.
- Distinct program points do not collapse into one baseline entry in the covered collision cases.
- Baseline matching uses the new identity with explicit handling for legacy baselines.
- SARIF output exposes the identity without violating SARIF 2.1.0.
- Source URIs are repository-relative when the necessary source-root information is available.
- Unit and integration tests cover determinism, collisions, path normalization, and baseline behavior.

## Dependencies
- Existing baseline implementation and source-location mapping.
- Plan 14 for SARIF validation.
- Plan 13 should reuse these stable program-point identities for evidence locations where useful.

## Complexity Estimate
High
