# THREAD_SLEEP_CALL

## Summary
- Rule ID: `THREAD_SLEEP_CALL`
- Name: Thread.sleep call
- Problem: `Thread.sleep(...)` introduces blocking, timing-coupled behavior that is often brittle and hard to validate.

## What This Rule Reports
This rule reports direct calls to:
- `java/lang/Thread.sleep(J)V`
- `java/lang/Thread.sleep(JI)V`

### Examples (reported)
```java
package com.example;
public class ClassA {
    public void methodX() throws InterruptedException {
        Thread.sleep(10L);
        Thread.sleep(10L, 0);
    }
}
```

## What This Rule Does Not Report
- Other thread APIs such as `Thread.currentThread()`.
- Calls that appear only in classpath/dependency classes outside the analysis target.

### Examples (not reported)
```java
package com.example;
public class ClassB {
    public Thread methodY() {
        return Thread.currentThread();
    }
}
```

## Recommended Fix
Use explicit synchronization/coordination primitives or scheduler abstractions instead of timing-based `Thread.sleep(...)` coordination.

## Message Shape
Findings are reported as `Avoid Thread.sleep() in <class>.<method><descriptor>; prefer explicit synchronization or scheduler abstractions over timing-based sleeps.`
