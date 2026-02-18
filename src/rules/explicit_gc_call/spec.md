# EXPLICIT_GC_CALL

## Summary
- Rule ID: `EXPLICIT_GC_CALL`
- Name: Explicit GC call
- Problem: Explicit GC calls in application/library code are usually unnecessary and can reduce runtime predictability.

## What This Rule Reports
This rule reports direct calls to explicit GC APIs in analysis target classes:
- `java/lang/System.gc()V`
- `java/lang/Runtime.gc()V`

### Examples (reported)
```java
package com.example;
public class ClassA {
    public void methodX() {
        System.gc();
    }
}
```

```java
package com.example;
public class ClassB {
    public void methodY() {
        Runtime.getRuntime().gc();
    }
}
```

## What This Rule Does Not Report
- Non-GC `System` APIs (for example `System.lineSeparator()`).
- Calls that appear only in classpath/dependency classes outside the analysis target.

### Examples (not reported)
```java
package com.example;
public class ClassC {
    public String methodZ() {
        return System.lineSeparator();
    }
}
```

## Recommended Fix
Remove explicit GC calls and let the JVM manage garbage collection; tune GC behavior through JVM options if needed.

## Message Shape
Findings are reported as `Avoid explicit GC call in <class>.<method><descriptor>; let the JVM manage garbage collection.`
