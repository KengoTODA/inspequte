---
type: 'Static Analysis Rule'
title: 'System.exit call'
description: 'Direct calls to System.exit(int) terminate the JVM abruptly'
tags: ['jvm', 'static-analysis']
status: 'stable'
rule_id: 'SYSTEM_EXIT'
---

# SYSTEM_EXIT

## Summary
- Rule ID: `SYSTEM_EXIT`
- Name: System.exit call
- Problem: `System.exit(int)` terminates the whole JVM and can abruptly stop applications or services.

## What This Rule Reports
This rule reports direct process-termination calls in analysis target classes:
- `java/lang/System.exit(I)V`
- `kotlin.system.exitProcess(Int)` (treated equivalently)

### Examples (reported)
```java
package com.example;
public class ClassA {
    public void methodX() {
        System.exit(1);
    }
}
```

```kotlin
package com.example

import kotlin.system.exitProcess

fun methodX(varOne: Boolean) {
    if (varOne) {
        exitProcess(0)
    }
}
```

## What This Rule Does Not Report
- Other `System` APIs that do not terminate the JVM (for example `System.lineSeparator()`).
- Calls that appear only in classpath/dependency classes outside the analysis target.
- Calls inside application entry points:
  - Java: `public static void main(String[] args)`
  - Kotlin top-level `main` entry points (`fun main(args: Array<String>)` and `fun main()`)

### Examples (not reported)
```java
package com.example;
public class ClassA {
    public String methodX() {
        return System.lineSeparator();
    }
}
```

```java
package com.example;
public class ClassB {
    public static void main(String[] varOne) {
        System.exit(0);
    }
}
```

```kotlin
package com.example

import kotlin.system.exitProcess

fun main() {
    exitProcess(0)
}
```

## Recommended Fix
Avoid terminating the JVM from library/application internals. Return an error to the caller or throw/propagate an exception so the caller decides shutdown behavior.

## Message Shape
Findings are reported as `Avoid System.exit() in <class>.<method><descriptor>; return an error or throw an exception instead.`
