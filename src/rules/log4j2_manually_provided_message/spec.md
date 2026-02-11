# LOG4J2_MANUALLY_PROVIDED_MESSAGE

## Summary
- Rule ID: `LOG4J2_MANUALLY_PROVIDED_MESSAGE`
- Name: Log4j2 preformatted message
- Problem: Preformatting messages before logging bypasses placeholder-based logging benefits.

## What This Rule Reports
This rule reports Log4j2 logger calls where message text is manually formatted before the logging API call.

### Java Example (reported)
```java
LOG.info(String.format("user=%s action=%s", varOne, varTwo));
```

## What This Rule Does Not Report
- Placeholder-based logging calls
- Message-only forms that are not manually preformatted

### Java Example (not reported)
```java
LOG.info("user={} action={}", varOne, varTwo);
```

## Recommended Fix
Use Log4j2 placeholders and pass dynamic values as separate arguments.

## Message Shape
Findings explain that Log4j2 messages should use placeholders instead of manual formatting.

## Source of Truth
- Implementation: `src/rules/log4j2_manually_provided_message/mod.rs`
- Plan: `src/rules/log4j2_manually_provided_message/plan.md`
- Behavior inferred from in-file harness tests.
