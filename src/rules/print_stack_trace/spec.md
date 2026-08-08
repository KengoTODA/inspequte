---
type: 'Static Analysis Rule'
title: 'Direct printStackTrace call'
description: 'Throwable.printStackTrace should be replaced with structured logging'
tags: ['jvm', 'static-analysis']
status: 'stable'
rule_id: 'PRINT_STACK_TRACE'
---

# PRINT_STACK_TRACE

## Summary
- Rule ID: `PRINT_STACK_TRACE`
- Name: Direct printStackTrace call
- Problem: Calling `Throwable.printStackTrace(...)` bypasses structured logging and can reduce observability in production.

## What This Rule Reports
This rule reports direct calls to `java/lang/Throwable.printStackTrace` overloads in analysis target classes:
- `printStackTrace()`
- `printStackTrace(PrintStream)`
- `printStackTrace(PrintWriter)`

### Examples (reported)
```java
package com.example;
public class ClassA {
    public void methodX(Exception varOne) {
        varOne.printStackTrace();
    }
}
```

```java
package com.example;
import java.io.PrintWriter;
public class ClassB {
    public void methodY(Exception varOne) {
        varOne.printStackTrace(new PrintWriter(System.err));
    }
}
```

## What This Rule Does Not Report
- Unrelated stack-inspection APIs (for example `Thread.currentThread().getStackTrace()`).
- Calls that appear only in classpath/dependency classes outside the analysis target.

### Examples (not reported)
```java
package com.example;
public class ClassC {
    public StackTraceElement[] methodZ() {
        return Thread.currentThread().getStackTrace();
    }
}
```

## Recommended Fix
Use structured logging and include the exception object with context instead of printing directly.

## Message Shape
Findings are reported as `Avoid printStackTrace() in <class>.<method><descriptor>; log exceptions with context instead.`
