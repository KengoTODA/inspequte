# LOG4J2_LOGGER_SHOULD_BE_FINAL

## Summary
- Rule ID: `LOG4J2_LOGGER_SHOULD_BE_FINAL`
- Name: Log4j2 logger should be final
- Problem: Logger fields should not be reassigned.

## What This Rule Reports
This rule reports Log4j2 logger fields that are not `final`.

### Java Example (reported)
```java
import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;

class ClassA {
    private Logger log = LogManager.getLogger(ClassA.class);
}
```

## What This Rule Does Not Report
- Logger fields declared `final`

### Java Example (not reported)
```java
class ClassA {
    private final Logger log = LogManager.getLogger(ClassA.class);
}
```

## Recommended Fix
Declare logger fields as `final`.

## Message Shape
Findings are reported as `Logger field <class>.<field> should be final`.
