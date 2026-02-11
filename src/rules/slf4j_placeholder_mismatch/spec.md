# SLF4J_PLACEHOLDER_MISMATCH

## Summary
- Rule ID: `SLF4J_PLACEHOLDER_MISMATCH`
- Name: SLF4J placeholder mismatch
- Problem: Placeholder count mismatch makes logs confusing and can hide missing context.

## What This Rule Reports
This rule reports SLF4J format calls where placeholder count and supplied arguments do not match.
It handles escaped placeholders and common varargs/marker forms.

### Java Example (reported)
```java
LOG.info("user={} action={}", varOne);
```

## What This Rule Does Not Report
- Correctly matched placeholder/argument counts
- Escaped placeholder text that should not count
- Supported marker/throwable patterns where argument treatment differs

### Java Example (not reported)
```java
LOG.info("user={} action={}", varOne, varTwo);
```

## Recommended Fix
Align placeholder count with provided arguments, or rewrite message/arguments for clarity.

## Message Shape
Findings describe expected vs actual argument count for the SLF4J format string.

## Source of Truth
- Implementation: `src/rules/slf4j_placeholder_mismatch/mod.rs`
- Behavior inferred from in-file harness tests, including escaped placeholders and varargs handling.
