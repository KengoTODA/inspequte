---
type: 'Static Analysis Rule'
title: 'URL hashCode call'
description: 'URL.hashCode may trigger host resolution and surprising hash semantics'
tags: ['jvm', 'static-analysis']
status: 'stable'
rule_id: 'URL_HASHCODE_CALL'
---

# URL_HASHCODE_CALL

## Summary
- Rule ID: `URL_HASHCODE_CALL`
- Name: URL hashCode call
- Problem: `URL.hashCode()` can involve host resolution and lead to surprising hashing behavior.

## What This Rule Reports
This rule reports direct calls to:
- `java/net/URL.hashCode()I`

### Examples (reported)
```java
package com.example;
import java.net.URL;
public class ClassA {
    public int methodX(URL varOne) {
        return varOne.hashCode();
    }
}
```

## What This Rule Does Not Report
- `URI.hashCode()` usage.
- Calls that appear only in classpath/dependency classes outside the analysis target.

### Examples (not reported)
```java
package com.example;
import java.net.URI;
public class ClassB {
    public int methodY(URI varOne) {
        return varOne.hashCode();
    }
}
```

## Recommended Fix
Prefer hashing normalized `URI` values or explicit URL components instead of `URL.hashCode()`.

## Message Shape
Findings are reported as `Avoid URL.hashCode() in <class>.<method><descriptor>; hash normalized URI values or explicit URL components instead.`
