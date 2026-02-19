# OPTIONAL_GET_CALL

## Summary
- Rule ID: `OPTIONAL_GET_CALL`
- Name: Optional direct getter call
- Problem: Calling `Optional.get()` / `getAs*()` directly can throw when the value is empty.

## What This Rule Reports
This rule reports direct getter calls on Optional APIs in analysis target classes:
- `java/util/Optional.get()`
- `java/util/OptionalInt.getAsInt()`
- `java/util/OptionalLong.getAsLong()`
- `java/util/OptionalDouble.getAsDouble()`

### Examples (reported)
```java
package com.example;
import java.util.Optional;
public class ClassA {
    public String methodX() {
        Optional<String> varOne = Optional.empty();
        return varOne.get();
    }
}
```

```java
package com.example;
import java.util.OptionalInt;
public class ClassB {
    public int methodY() {
        OptionalInt varOne = OptionalInt.empty();
        return varOne.getAsInt();
    }
}
```

## What This Rule Does Not Report
- Safer Optional APIs that handle empty explicitly (for example `orElse`, `orElseThrow`, `ifPresent`).
- Direct getter calls inside a branch where non-empty is explicitly guaranteed (for example, inside `if (varOne.isPresent()) { ... }`).
- Calls that appear only in classpath/dependency classes outside the analysis target.

### Examples (not reported)
```java
package com.example;
import java.util.Optional;
public class ClassC {
    public String methodZ() {
        Optional<String> varOne = Optional.empty();
        return varOne.orElse("fallback");
    }
}
```

```java
package com.example;
import java.util.Optional;
public class ClassD {
    public String methodW(Optional<String> varOne) {
        if (varOne.isPresent()) {
            return varOne.get();
        }
        return "fallback";
    }
}
```

## Recommended Fix
Replace direct getter calls with explicit empty handling, such as `orElse`, `orElseThrow`, or `ifPresent`.

## Message Shape
Findings are reported as `Avoid Optional direct getter in <class>.<method><descriptor>; use orElse/orElseThrow/ifPresent instead.`
