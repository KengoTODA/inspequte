# BIGDECIMAL_SET_SCALE_WITHOUT_ROUNDING

## Summary
- Rule ID: `BIGDECIMAL_SET_SCALE_WITHOUT_ROUNDING`
- Name: BigDecimal setScale without rounding
- Problem: `BigDecimal.setScale(int)` can throw at runtime when rounding is necessary.

## What This Rule Reports
This rule reports direct calls to:
- `java/math/BigDecimal.setScale(I)Ljava/math/BigDecimal;`

### Examples (reported)
```java
package com.example;
import java.math.BigDecimal;
public class ClassA {
    public BigDecimal methodX(BigDecimal varOne) {
        return varOne.setScale(2);
    }
}
```

## What This Rule Does Not Report
- Overloads that specify rounding mode explicitly.
- Calls that appear only in classpath/dependency classes outside the analysis target.

### Examples (not reported)
```java
package com.example;
import java.math.BigDecimal;
import java.math.RoundingMode;
public class ClassB {
    public BigDecimal methodY(BigDecimal varOne) {
        return varOne.setScale(2, RoundingMode.HALF_UP);
    }
}
```

## Recommended Fix
Use `setScale(scale, RoundingMode)` to make rounding behavior explicit.

## Message Shape
Findings are reported as `Avoid BigDecimal.setScale(...) without rounding in <class>.<method><descriptor>; specify RoundingMode.`
