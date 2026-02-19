# BIGDECIMAL_DIVIDE_WITHOUT_ROUNDING

## Summary
- Rule ID: `BIGDECIMAL_DIVIDE_WITHOUT_ROUNDING`
- Name: BigDecimal divide without rounding
- Problem: `BigDecimal.divide(BigDecimal)` can throw at runtime for non-terminating decimal expansions.

## What This Rule Reports
This rule reports direct calls to:
- `java/math/BigDecimal.divide(Ljava/math/BigDecimal;)Ljava/math/BigDecimal;`

### Examples (reported)
```java
package com.example;
import java.math.BigDecimal;
public class ClassA {
    public BigDecimal methodX(BigDecimal varOne, BigDecimal varTwo) {
        return varOne.divide(varTwo);
    }
}
```

## What This Rule Does Not Report
- Overloads that specify rounding or context (for example `RoundingMode`, `MathContext`).
- Kotlin BigDecimal operator division (`a / b`), which is compiled with an explicit rounding mode.
- Calls that appear only in classpath/dependency classes outside the analysis target.

### Examples (not reported)
```java
package com.example;
import java.math.BigDecimal;
import java.math.RoundingMode;
public class ClassB {
    public BigDecimal methodY(BigDecimal varOne, BigDecimal varTwo) {
        return varOne.divide(varTwo, RoundingMode.HALF_UP);
    }
}
```

```kotlin
package com.example

import java.math.BigDecimal

fun methodKotlinSafe(varOne: BigDecimal, varTwo: BigDecimal): BigDecimal {
    return varOne / varTwo
}
```

## Recommended Fix
Use an overload that specifies `RoundingMode` or `MathContext` to make division behavior explicit.

## Message Shape
Findings are reported as `Avoid BigDecimal.divide(...) without rounding in <class>.<method><descriptor>; specify RoundingMode or MathContext.`
