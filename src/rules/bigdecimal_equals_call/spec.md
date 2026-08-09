---
type: 'Static Analysis Rule'
title: 'BigDecimal equals call'
description: 'BigDecimal.equals compares value and scale instead of numeric equality'
tags: ['jvm', 'static-analysis']
status: 'stable'
rule_id: 'BIGDECIMAL_EQUALS_CALL'
---

# BIGDECIMAL_EQUALS_CALL

## Summary
- Rule ID: `BIGDECIMAL_EQUALS_CALL`
- Name: BigDecimal equals call
- Problem: `BigDecimal.equals(Object)` compares value and scale, which often differs from numeric equality intent.

## What This Rule Reports
This rule reports direct calls to:
- `java/math/BigDecimal.equals(Ljava/lang/Object;)Z`

### Examples (reported)
```java
package com.example;
import java.math.BigDecimal;
public class ClassA {
    public boolean methodX(BigDecimal varOne, BigDecimal varTwo) {
        return varOne.equals(varTwo);
    }
}
```

## What This Rule Does Not Report
- Numeric comparisons using `compareTo(...) == 0`.
- Calls that appear only in classpath/dependency classes outside the analysis target.

### Examples (not reported)
```java
package com.example;
import java.math.BigDecimal;
public class ClassB {
    public boolean methodY(BigDecimal varOne, BigDecimal varTwo) {
        return varOne.compareTo(varTwo) == 0;
    }
}
```

## Recommended Fix
Use `compareTo(...) == 0` when the intent is numeric equality independent of scale.

## Message Shape
Findings are reported as `Avoid BigDecimal.equals() in <class>.<method><descriptor>; use compareTo(...) == 0 for numeric equality.`
