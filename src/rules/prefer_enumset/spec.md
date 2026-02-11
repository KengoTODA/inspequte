# PREFER_ENUMSET

## Summary
- Rule ID: `PREFER_ENUMSET`
- Name: Prefer EnumSet for enum collections
- Problem: General-purpose collections for enum-only values are usually slower and heavier than `EnumSet`.

## What This Rule Reports
This rule reports collection usage where element type is an enum and a non-`EnumSet` container is used, including:
- field declarations
- method parameters/returns
- local variables

Targeted alternatives include `HashSet<EnumType>`, `Set<EnumType>`, and `Collection<EnumType>` style declarations where `EnumSet` is a better fit.

### Java Example (reported)
```java
import java.util.HashSet;
import java.util.Set;

enum ClassB { A, B }

class ClassA {
    Set<ClassB> varOne = new HashSet<>();
}
```

## What This Rule Does Not Report
- `EnumSet<EnumType>` usage
- Non-enum element collections
- Enum map value positions where set replacement is not applicable

### Java Example (not reported)
```java
import java.util.EnumSet;

enum ClassB { A, B }

class ClassA {
    EnumSet<ClassB> varOne = EnumSet.noneOf(ClassB.class);
}
```

## Recommended Fix
Replace enum-only set-like containers with `EnumSet` where semantics allow.

## Message Shape
Findings explain the concrete location and recommend `EnumSet` for better performance and intent clarity.
