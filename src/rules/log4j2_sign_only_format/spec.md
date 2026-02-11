# LOG4J2_SIGN_ONLY_FORMAT

## Summary
- Rule ID: `LOG4J2_SIGN_ONLY_FORMAT`
- Name: Log4j2 placeholder-only format
- Problem: Placeholder-only format strings are hard to understand in logs.

## What This Rule Reports
This rule reports Log4j2 format strings that contain placeholders only and no descriptive text.

### Java Example (reported)
```java
LOG.info("{} {}", varOne, varTwo);
```

## What This Rule Does Not Report
- Format strings containing descriptive text with placeholders
- Message-only forms with meaningful text

### Java Example (not reported)
```java
LOG.info("user={} action={}", varOne, varTwo);
```

## Recommended Fix
Add human-readable context text to the format string.

## Message Shape
Findings are reported as `Log4j2 format string should include text`.

## Source of Truth
- Implementation: `src/rules/log4j2_sign_only_format/mod.rs`
- Plan: `src/rules/log4j2_sign_only_format/plan.md`
- Behavior inferred from in-file harness tests.
