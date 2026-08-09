---
type: 'Static Analysis Rule'
title: 'Prefer EnumSet for enum collections'
description: 'Using EnumSet for enum types provides better performance than general collections'
tags: ['jvm', 'static-analysis']
status: 'stable'
rule_id: 'PREFER_ENUMSET'
---

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

Targeted alternatives include set-like declarations such as `HashSet<EnumType>`, `Set<EnumType>`, and `Collection<EnumType>` where `EnumSet` is a better fit.

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
- `List<EnumType>` usage, because `List` preserves ordering semantics that `EnumSet` cannot replace

### Java Example (not reported)
```java
import java.util.ArrayList;
import java.util.EnumSet;
import java.util.List;

enum ClassB { A, B }

class ClassA {
    EnumSet<ClassB> varOne = EnumSet.noneOf(ClassB.class);
    List<ClassB> varTwo = new ArrayList<>();
}
```

## Recommended Fix
Replace enum-only set-like containers with `EnumSet` where semantics allow and ordering is not required.

## Message Shape
Findings explain the concrete location and recommend `EnumSet` for better performance and intent clarity.
