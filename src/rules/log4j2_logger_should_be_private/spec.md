# LOG4J2_LOGGER_SHOULD_BE_PRIVATE

## Summary
- Rule ID: `LOG4J2_LOGGER_SHOULD_BE_PRIVATE`
- Name: Log4j2 logger should be private
- Problem: Non-private logger fields expose internal logging details and are easy to misuse.

## What This Rule Reports
This rule reports Log4j2 logger fields that are not `private`.

### Java Example (reported)
```java
import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;

class ClassA {
    Logger log = LogManager.getLogger(ClassA.class);
}
```

## What This Rule Does Not Report
- Logger fields declared `private`

### Java Example (not reported)
```java
class ClassA {
    private Logger log = LogManager.getLogger(ClassA.class);
}
```

## Recommended Fix
Declare logger fields as `private` (commonly `private static final`).

## Message Shape
Findings are reported as `Logger field <class>.<field> should be private`.
