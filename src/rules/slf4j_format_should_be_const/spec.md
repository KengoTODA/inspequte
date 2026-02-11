# SLF4J_FORMAT_SHOULD_BE_CONST

## Summary
- Rule ID: `SLF4J_FORMAT_SHOULD_BE_CONST`
- Name: SLF4J format should be const
- Problem: Non-constant format strings reduce log consistency and can hide placeholder mistakes.

## What This Rule Reports
This rule reports SLF4J logger calls where the format argument is not a compile-time constant string.

### Java Example (reported)
```java
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

class ClassA {
    private static final Logger LOG = LoggerFactory.getLogger(ClassA.class);

    void methodOne(String varOne) {
        String tmpValue = "prefix: " + varOne;
        LOG.info(tmpValue);
    }
}
```

## What This Rule Does Not Report
- Constant string format literals
- Overloads where message is passed as a dedicated message-only argument
- Marker + message overloads with constant message

### Java Example (not reported)
```java
LOG.info("user={} action={}", varOne, "run");
```

## Recommended Fix
Use a constant format string and pass dynamic values as arguments.

## Message Shape
Findings are reported as `SLF4J format string should be constant`.
