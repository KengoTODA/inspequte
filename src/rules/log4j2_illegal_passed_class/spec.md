# LOG4J2_ILLEGAL_PASSED_CLASS

## Summary
- Rule ID: `LOG4J2_ILLEGAL_PASSED_CLASS`
- Name: Log4j2 illegal passed class
- Problem: `LogManager.getLogger(...)` should receive the declaring class to keep logger category correct.

## What This Rule Reports
This rule reports `LogManager.getLogger(Class)` calls that pass a class literal different from the current class.

### Java Example (reported)
```java
import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;

class ClassA {
    private static final Logger LOG = LogManager.getLogger(String.class);
}
```

## What This Rule Does Not Report
- `LogManager.getLogger(ClassA.class)` from `ClassA`
- No-argument logger factory overloads

### Java Example (not reported)
```java
class ClassA {
    private static final Logger LOG = LogManager.getLogger(ClassA.class);
}
```

## Recommended Fix
Pass the declaring class literal (`ClassA.class`) to `getLogger`.

## Message Shape
Findings explain that `LogManager.getLogger` should be called with the caller class and show expected/actual values.

## Source of Truth
- Implementation: `src/rules/log4j2_illegal_passed_class/mod.rs`
- Plan: `src/rules/log4j2_illegal_passed_class/plan.md`
- Behavior inferred from in-file harness tests.
