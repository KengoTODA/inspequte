---
type: 'Static Analysis Rule'
title: 'Explicit finalization trigger call'
description: 'System/Runtime runFinalization calls are unpredictable'
tags: ['jvm', 'static-analysis']
status: 'stable'
rule_id: 'RUN_FINALIZATION_CALL'
---

# RUN_FINALIZATION_CALL

## Summary
- Rule ID: `RUN_FINALIZATION_CALL`
- Name: Explicit finalization trigger call
- Problem: explicit finalization triggers are unpredictable and should be avoided in regular application logic.

## What This Rule Reports
This rule reports direct calls to:
- `java/lang/System.runFinalization()V`
- `java/lang/Runtime.runFinalization()V`

### Examples (reported)
```java
package com.example;
public class ClassA {
    public void methodX() {
        System.runFinalization();
    }
}
```

```java
package com.example;
public class ClassB {
    public void methodY() {
        Runtime.getRuntime().runFinalization();
    }
}
```

## What This Rule Does Not Report
- Unrelated runtime APIs (for example `Runtime.getRuntime()`).
- Calls that appear only in classpath/dependency classes outside the analysis target.

### Examples (not reported)
```java
package com.example;
public class ClassC {
    public Runtime methodZ() {
        return Runtime.getRuntime();
    }
}
```

## Recommended Fix
Replace explicit finalization triggers with deterministic cleanup logic, such as explicit close operations or try-with-resources.

## Message Shape
Findings are reported as `Avoid explicit finalization trigger in <class>.<method><descriptor>; use deterministic resource cleanup instead.`
