# RUNTIME_HALT_CALL

## Summary
- Rule ID: `RUNTIME_HALT_CALL`
- Name: Runtime.halt call
- Problem: `Runtime.halt(int)` terminates the JVM abruptly without graceful shutdown.

## What This Rule Reports
This rule reports direct calls to:
- `java/lang/Runtime.halt(I)V`

### Examples (reported)
```java
package com.example;
public class ClassA {
    public void methodX() {
        Runtime.getRuntime().halt(1);
    }
}
```

## What This Rule Does Not Report
- Other termination APIs that are not `Runtime.halt(int)`.
- Calls that appear only in classpath/dependency classes outside the analysis target.

### Examples (not reported)
```java
package com.example;
public class ClassB {
    public void methodY() {
        System.exit(1);
    }
}
```

## Recommended Fix
Prefer orderly shutdown paths (for example explicit error handling and controlled process termination) instead of `Runtime.halt(int)` where possible.

## Message Shape
Findings are reported as `Avoid Runtime.halt() in <class>.<method><descriptor>; prefer orderly shutdown and explicit error handling.`
