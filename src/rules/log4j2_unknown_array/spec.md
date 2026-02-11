# LOG4J2_UNKNOWN_ARRAY

## Summary
- Rule ID: `LOG4J2_UNKNOWN_ARRAY`
- Name: Log4j2 unknown array
- Problem: Passing unknown arrays into varargs logging calls can produce confusing argument expansion behavior.

## What This Rule Reports
This rule reports Log4j2 varargs calls where the provided array arguments are not statically known.

### Java Example (reported)
```java
Object[] varOne = methodOne();
LOG.info("value={} {}", varOne);
```

## What This Rule Does Not Report
- Calls with known array construction/local shape

### Java Example (not reported)
```java
Object[] varOne = new Object[] {"a", 1};
LOG.info("value={} {}", varOne);
```

## Recommended Fix
Prefer explicit arguments or clearly constructed arrays so logging argument structure stays predictable.

## Message Shape
Findings explain that an unknown array is passed to a Log4j2 varargs logging call.

## Source of Truth
- Implementation: `src/rules/log4j2_unknown_array/mod.rs`
- Plan: `src/rules/log4j2_unknown_array/plan.md`
- Behavior inferred from in-file harness tests.
