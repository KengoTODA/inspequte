# Plan: Java 26 Class-File Compatibility Lane

## Objective
Continuously verify that inspequte can ingest Java 26 class files while keeping Java 21 as the project's primary test-harness baseline.

## Background
Java SE 26 uses class-file major version 70. A static analyzer can fail before rule execution if its parser rejects a newer version, encounters an unfamiliar attribute, or mishandles updated bytecode metadata.

A dedicated compatibility lane provides early warning without forcing the entire test harness or minimum development environment to move away from Java 21. Draft future features, including proposed null-restricted and nullable type metadata, should be tracked as compatibility watch items rather than implemented as production semantics before standardization.

## Implementation Approach
- Add a separate CI job using an explicitly versioned Java 26 JDK distribution.
- Compile a small generic Java fixture with the Java 26 compiler.
- Assert that the generated class files use major version 70.
- Scan both the compiled class directory and a generated JAR with inspequte.
- Validate the resulting SARIF and assert deterministic output.
- Include representative constructs relevant to class-file ingestion while following the test-harness generic naming guidelines.
- Confirm that supported unknown or non-critical attributes are ignored safely and deterministically.
- Record the JDK vendor and exact version in CI output and artifacts.
- Review and, if needed, update the class-file parsing dependency with the smallest compatible change.
- Keep the existing Java 21 build and test lane authoritative for the normal harness.
- Document the policy and trigger for adding future Java-version compatibility lanes.

## Test Cases
- The Java 26 fixture compiles and its class-file header reports major version 70.
- inspequte scans the class directory without a parser failure.
- inspequte scans the equivalent JAR without a parser failure.
- Both scans emit valid SARIF 2.1.0.
- Repeated scans produce deterministic results and artifact ordering.
- CI failures distinguish class-file parser incompatibility from rule-analysis or SARIF-validation failures.
- Unknown non-critical attributes used by the fixture do not cause a crash or nondeterministic result.
- The existing Java 21 harness remains green and unchanged in purpose.

## Success Criteria
- CI contains a distinct Java 26 compatibility job that does not replace the Java 21 harness.
- The fixture is compiled by Java 26 into asserted major-version-70 class files.
- inspequte scans both the class directory and JAR successfully.
- Generated SARIF validates and is deterministic across repeated runs.
- Failures clearly identify unsupported class-file parsing separately from rule-analysis failures.
- Any required dependency change is minimal and covered by regression tests.
- The compatibility policy and process for future Java lanes are documented.

## Dependencies
- Availability of a Java 26 JDK in GitHub Actions or a trusted setup action.
- Current class-file parser support for major version 70 or an available compatible update.
- Plan 14 for normative SARIF validation in the compatibility lane.
- Java 21 remains configured for the primary test harness.

## Complexity Estimate
Medium
