# OBJECT_WAIT_WITHOUT_TIMEOUT

## Summary
- Rule ID: `OBJECT_WAIT_WITHOUT_TIMEOUT`
- Name: Object.wait without timeout
- Problem: timeout-free `Object.wait()` can block indefinitely and cause stuck threads.

## What This Rule Reports
This rule reports direct calls to:
- `java/lang/Object.wait()V`

### Examples (reported)
```java
package com.example;
public class ClassA {
    public void methodX(Object varOne) throws Exception {
        synchronized (varOne) {
            varOne.wait();
        }
    }
}
```

## What This Rule Does Not Report
- Timed waits (`wait(long)`, `wait(long, int)`).
- Calls that appear only in classpath/dependency classes outside the analysis target.

### Examples (not reported)
```java
package com.example;
public class ClassB {
    public void methodY(Object varOne) throws Exception {
        synchronized (varOne) {
            varOne.wait(1000L);
        }
    }
}
```

## Recommended Fix
Use bounded waits (`wait(timeout)`) and explicit condition checks/retries to avoid indefinite blocking.

## Message Shape
Findings are reported as `Avoid timeout-free Object.wait() in <class>.<method><descriptor>; use a timed wait and explicit condition checks.`
