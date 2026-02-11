# SLF4J_LOGGER_SHOULD_BE_PRIVATE

## Summary
- Rule ID: `SLF4J_LOGGER_SHOULD_BE_PRIVATE`
- Name: SLF4J logger should be private
- Problem: Exposing logger fields broadens visibility unnecessarily and increases accidental external use.

## What This Rule Reports
This rule reports SLF4J logger fields that are not declared `private`.

### Java Example (reported)
```java
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

class ClassA {
    Logger log = LoggerFactory.getLogger(ClassA.class);
}
```

## What This Rule Does Not Report
- Logger fields declared `private`

### Java Example (not reported)
```java
class ClassA {
    private Logger log = LoggerFactory.getLogger(ClassA.class);
}
```

## Recommended Fix
Declare logger fields as `private` (typically `private static final`).

## Message Shape
Findings are reported as `Logger field <class>.<field> should be private`.
