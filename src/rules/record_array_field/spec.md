# RECORD_ARRAY_FIELD

## Summary
- Rule ID: `RECORD_ARRAY_FIELD`
- Name: Record array field
- Problem: Record components should be immutable-by-design data carriers; array-typed components are mutable and can break that expectation.

## What This Rule Reports
This rule reports record components whose declared type is an array.

### Java Example (reported)
```java
record ClassA(String[] varOne) {}
```

## What This Rule Does Not Report
- Non-array record components
- Non-record classes
- Static array fields (not record components)

### Java Example (not reported)
```java
record ClassA(String varOne) {}
```

## Recommended Fix
Prefer immutable collection/value alternatives (for example `List<T>`) or wrap/copy arrays defensively.

## Message Shape
Findings are reported as `Record component <class>.<component> uses array type <descriptor>`.

## Source of Truth
- Implementation: `src/rules/record_array_field/mod.rs`
- Behavior inferred from in-file unit and harness tests.
