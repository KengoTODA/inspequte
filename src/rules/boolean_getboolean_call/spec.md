---
type: 'Static Analysis Rule'
title: 'Boolean.getBoolean call'
description: 'Boolean.getBoolean reads system properties, not text booleans'
tags: ['jvm', 'static-analysis']
status: 'stable'
rule_id: 'BOOLEAN_GETBOOLEAN_CALL'
---

# BOOLEAN_GETBOOLEAN_CALL

## Summary
- Rule ID: `BOOLEAN_GETBOOLEAN_CALL`
- Name: Boolean.getBoolean call
- Problem: `Boolean.getBoolean(...)` reads system properties and is often mistakenly used for string-to-boolean parsing.

## What This Rule Reports
This rule reports direct calls to:
- `java/lang/Boolean.getBoolean(Ljava/lang/String;)Z`

### Examples (reported)
```java
package com.example;
public class ClassA {
    public boolean methodX(String varOne) {
        return Boolean.getBoolean(varOne);
    }
}
```

## What This Rule Does Not Report
- String parsing APIs such as `Boolean.parseBoolean(...)` and `Boolean.valueOf(...)`.
- Calls that appear only in classpath/dependency classes outside the analysis target.

### Examples (not reported)
```java
package com.example;
public class ClassB {
    public boolean methodY(String varOne) {
        return Boolean.parseBoolean(varOne);
    }
}
```

## Recommended Fix
Use `Boolean.parseBoolean(...)` for string parsing and reserve `Boolean.getBoolean(...)` for explicit system property reads.

## Message Shape
Findings are reported as `Avoid Boolean.getBoolean() in <class>.<method><descriptor>; use Boolean.parseBoolean()/valueOf() for text parsing or keep it only for system property reads.`
