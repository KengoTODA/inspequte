---
type: 'Static Analysis Rule'
title: 'Thread.run direct call'
description: 'Direct Thread.run() calls execute synchronously on the current thread'
tags: ['jvm', 'static-analysis']
status: 'stable'
rule_id: 'THREAD_RUN_DIRECT_CALL'
---

# THREAD_RUN_DIRECT_CALL

## Summary
- Rule ID: `THREAD_RUN_DIRECT_CALL`
- Name: Thread.run direct call
- Problem: Calling `Thread.run()` directly runs on the current thread and usually indicates a missed `start()` call.

## What This Rule Reports
This rule reports direct invocations of:
- `java/lang/Thread.run()V`

### Examples (reported)
```java
package com.example;
public class ClassA {
    public void methodX() {
        Thread varOne = new Thread(() -> {});
        varOne.run();
    }
}
```

## What This Rule Does Not Report
- Calls to `Thread.start()`.
- `super.run()` calls inside an overridden `run()V` method.
- Calls that appear only in classpath/dependency classes outside the analysis target.

### Examples (not reported)
```java
package com.example;
public class ClassB {
    public void methodY() {
        Thread varOne = new Thread(() -> {});
        varOne.start();
    }
}
```

```java
package com.example;
public class ClassC extends Thread {
    @Override
    public void run() {
        super.run();
    }
}
```

## Recommended Fix
If asynchronous execution is intended, replace direct `run()` calls with `start()`.

## Message Shape
Findings are reported as `Avoid direct Thread.run() in <class>.<method><descriptor>; call start() for asynchronous execution.`
