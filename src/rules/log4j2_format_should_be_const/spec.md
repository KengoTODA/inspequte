# LOG4J2_FORMAT_SHOULD_BE_CONST

## Summary
- Rule ID: `LOG4J2_FORMAT_SHOULD_BE_CONST`
- Name: Log4j2 format should be const
- Problem: Dynamic format strings reduce readability and make placeholder behavior less predictable.

## What This Rule Reports
This rule reports Log4j2 logger calls where the format/message argument is not a compile-time constant string.

### Java Example (reported)
```java
import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;

class ClassA {
    private static final Logger LOG = LogManager.getLogger(ClassA.class);

    void methodOne(String varOne) {
        String tmpValue = "value=" + varOne;
        LOG.info(tmpValue);
    }
}
```

## What This Rule Does Not Report
- Constant format literals
- Marker + message overloads with constant message text

### Java Example (not reported)
```java
LOG.info("user={} action={}", varOne, varTwo);
```

## Recommended Fix
Use a constant format string and pass dynamic data as arguments.

## Message Shape
Findings are reported as `Log4j2 format string should be constant`.
