# SLF4J_SIGN_ONLY_FORMAT

## Summary
- Rule ID: `SLF4J_SIGN_ONLY_FORMAT`
- Name: SLF4J placeholder-only format
- Problem: Format strings containing only `{}` placeholders provide little diagnostic context.

## What This Rule Reports
This rule reports SLF4J format strings that are placeholder-only (no descriptive text).

### Java Example (reported)
```java
LOG.info("{} {}", varOne, varTwo);
```

## What This Rule Does Not Report
- Format strings with descriptive text plus placeholders
- Message-only overloads with meaningful text

### Java Example (not reported)
```java
LOG.info("user={} action={}", varOne, varTwo);
```

## Recommended Fix
Include context text in the format string so logs are readable without external knowledge.

## Message Shape
Findings state that the SLF4J format string should include text.
