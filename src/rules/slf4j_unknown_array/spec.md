# SLF4J_UNKNOWN_ARRAY

## Summary
- Rule ID: `SLF4J_UNKNOWN_ARRAY`
- Name: SLF4J unknown array
- Problem: Passing unknown object arrays to varargs logging calls can produce unintended formatting/output.

## What This Rule Reports
This rule reports SLF4J varargs logging calls where the argument array origin is unknown at analysis time.

### Java Example (reported)
```java
Object[] varOne = methodOne();
LOG.info("value={} {}", varOne);
```

## What This Rule Does Not Report
- Clearly known local array literals / constructions with predictable shape

### Java Example (not reported)
```java
Object[] varOne = new Object[] {"a", 1};
LOG.info("value={} {}", varOne);
```

## Recommended Fix
Prefer explicit placeholder arguments or construct arrays in a way that preserves clear arity and intent.

## Message Shape
Findings explain that an unknown array is passed to an SLF4J varargs call.
