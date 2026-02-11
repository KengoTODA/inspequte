# SLF4J_MANUALLY_PROVIDED_MESSAGE

## Summary
- Rule ID: `SLF4J_MANUALLY_PROVIDED_MESSAGE`
- Name: SLF4J preformatted message
- Problem: Manually formatted log messages lose SLF4J placeholder benefits (lazy formatting, structured argument handling).

## What This Rule Reports
This rule reports SLF4J calls where the message is preformatted before logging (for example `String.format(...)` or concatenation fed as final message string).

### Java Example (reported)
```java
LOG.info(String.format("user=%s action=%s", varOne, varTwo));
```

## What This Rule Does Not Report
- Placeholder-based logging
- Cases where static analysis cannot reliably identify manual preformatting

### Java Example (not reported)
```java
LOG.info("user={} action={}", varOne, varTwo);
```

## Recommended Fix
Use SLF4J placeholders and pass values as arguments instead of prebuilding the message string.

## Message Shape
Findings explain that SLF4J messages should use placeholders instead of manual formatting.
