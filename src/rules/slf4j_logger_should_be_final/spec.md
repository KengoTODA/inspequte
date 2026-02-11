# SLF4J_LOGGER_SHOULD_BE_FINAL

## Summary
- Rule ID: `SLF4J_LOGGER_SHOULD_BE_FINAL`
- Name: SLF4J logger should be final
- Problem: Mutable logger fields are unnecessary and increase accidental reassignment risk.

## What This Rule Reports
This rule reports SLF4J logger fields that are not declared `final`.

### Java Example (reported)
```java
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

class ClassA {
    private Logger log = LoggerFactory.getLogger(ClassA.class);
}
```

## What This Rule Does Not Report
- Logger fields declared `final`

### Java Example (not reported)
```java
class ClassA {
    private final Logger log = LoggerFactory.getLogger(ClassA.class);
}
```

## Recommended Fix
Declare logger fields as `final`.

## Message Shape
Findings are reported as `Logger field <class>.<field> should be final`.
