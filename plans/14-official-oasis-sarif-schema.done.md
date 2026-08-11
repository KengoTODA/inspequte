# Plan: Official OASIS SARIF Schema Alignment

## Objective
Make the official OASIS SARIF 2.1.0 specification and Errata 01 schema the normative compatibility target for inspequte.

## Background
The repository currently carries and references a SchemaStore-derived SARIF schema. SchemaStore is useful, but it should not be the normative source for a SARIF-only CLI. Using the official OASIS artifact reduces ambiguity, makes the supported version explicit, and prepares the project to monitor future revisions without prematurely adopting drafts.

GitHub Code Scanning currently supports a subset of SARIF 2.1.0, so interoperability requires both standards conformance and a focused consumer compatibility check. SARIF 2.2 must remain a monitored draft until it is finalized and relevant consumers support it.

## Implementation Approach
- Replace or reproducibly regenerate the bundled schema from the official OASIS SARIF 2.1.0 Plus Errata 01 schema:
  - `https://docs.oasis-open.org/sarif/sarif/v2.1.0/errata01/os/schemas/sarif-schema-2.1.0.json`
- Record the upstream URL, specification version, checksum, license or provenance, and refresh procedure.
- Ensure emitted `version` and `$schema` values match the supported contract.
- Validate inspequte's generated SARIF in normal CI tests rather than relying only on an opt-in environment variable.
- Add a focused interoperability check for the GitHub-supported SARIF subset used by inspequte.
- Add a deterministic refresh/check script if the schema is vendored.
- Document the policy for monitoring and eventually adopting future SARIF versions.
- Do not change production output to SARIF 2.2 while it remains a draft or lacks required consumer support.

## Test Cases
- All CLI integration fixtures validate against the official Errata 01 schema.
- Invalid or misspelled SARIF properties fail deterministic tests.
- The emitted `version` and `$schema` values are asserted directly.
- The bundled schema checksum matches the documented upstream artifact.
- A representative report containing rule descriptors, artifacts, invocations, and results passes validation.
- A GitHub compatibility fixture exercises only fields that the project intentionally emits and verifies required GitHub assumptions.
- Schema validation failure reports the offending path clearly.

## Success Criteria
- The bundled schema is byte-for-byte sourced from, or reproducibly generated from, the official OASIS Errata 01 artifact.
- Provenance, checksum, and refresh instructions are documented.
- All CLI integration fixtures validate against the official schema in CI.
- Emitted SARIF declares version 2.1.0 and the agreed official schema URI.
- A deterministic test catches unsupported or misspelled SARIF properties.
- A GitHub Code Scanning interoperability fixture or canary verifies the subset used by inspequte.
- Documentation explicitly states that SARIF 2.2 is not a production target while it remains unsuitable for supported consumers.

## Dependencies
- Existing bundled SARIF schema and JSON validation infrastructure.
- Network access only when intentionally refreshing the vendored upstream schema.
- GitHub Code Scanning documentation or a controlled ingestion canary for consumer-specific checks.

## Complexity Estimate
Medium

## Post-mortem
- Went well: Pinning the immutable OASIS artifact by URL and SHA-256 made offline CI verification simple and reproducible.
- Tricky: `serde-sarif` still exports the SchemaStore URI, so inspequte needed its own normative URI and tests; local verification also required working around unavailable future lockfile dependencies.
- Follow-up: Monitor the GitHub ingestion canary and OASIS SARIF 2.2 progress, but retain 2.1.0 until the specification and required consumers are ready.
