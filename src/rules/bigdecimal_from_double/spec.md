---
type: 'Static Analysis Rule'
title: 'BigDecimal from double'
description: 'BigDecimal constructors with double can introduce precision surprises'
tags: ['jvm', 'static-analysis']
status: 'stable'
rule_id: 'BIGDECIMAL_FROM_DOUBLE'
---

## Summary
- Rule ID: `BIGDECIMAL_FROM_DOUBLE`
- Name: BigDecimal from double
- Description: Reports `BigDecimal` constructor calls that take `double` because they can introduce precision surprises.
- Annotation policy: `@Suppress`/`@SuppressWarnings` are not supported; only JSpecify annotations are recognized for annotation-driven semantics, and non-JSpecify annotations do not change behavior.

## Motivation
Using `new BigDecimal(double)` captures binary floating-point approximation instead of the intended decimal value. This can cause subtle rounding and comparison bugs in financial or precision-sensitive code.

## What it detects
- Constructor calls to `java/math/BigDecimal.<init>(D)V`.
- Constructor calls to `java/math/BigDecimal.<init>(DLjava/math/MathContext;)V`.
- Findings on analysis-target classes only.

## What it does NOT detect
- `BigDecimal.valueOf(double)` calls.
- String-based constructors such as `new BigDecimal("0.1")`.
- Suppression via annotations (`@Suppress`, `@SuppressWarnings`) is not supported.
- Non-JSpecify annotations do not affect rule behavior.

## Examples (TP/TN/Edge)
### True Positive
```java
import java.math.BigDecimal;

class ClassA {
    BigDecimal MethodX() {
        return new BigDecimal(0.1d);
    }
}
```

### True Negative
```java
import java.math.BigDecimal;

class ClassB {
    BigDecimal MethodY() {
        return BigDecimal.valueOf(0.1d);
    }
}
```

### Edge Case
```java
import java.math.BigDecimal;
import java.math.MathContext;

class ClassC {
    BigDecimal MethodZ(double varOne) {
        return new BigDecimal(varOne, MathContext.DECIMAL64);
    }
}
```

## Output
- Message: "BigDecimal constructed from double can lose precision. Use BigDecimal.valueOf(double) or a decimal string constructor."
- Location: the `BigDecimal` constructor call site.

## Performance considerations
- Expected linear scan over extracted call sites per method.
- No control-flow fixpoint analysis is required.

## Acceptance criteria
- Reports a finding for each `BigDecimal` constructor call that accepts `double`.
- Does not report `BigDecimal.valueOf(double)` or string constructors.
- Emits the actionable message defined in Output.
- Applies only to analysis-target classes.
- Applies annotation policy exactly as stated in Summary.
