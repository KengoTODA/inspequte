# FUTURE_GET_WITHOUT_TIMEOUT

## Summary
- Rule ID: `FUTURE_GET_WITHOUT_TIMEOUT`
- Name: Future.get without timeout
- Problem: timeout-free blocking `Future.get()` calls can wait indefinitely and reduce system responsiveness.

## What This Rule Reports
This rule reports timeout-free zero-argument `get()` calls on Java concurrent future types, including:
- `java/util/concurrent/Future.get()`
- `java/util/concurrent/CompletableFuture.get()`
- `java/util/concurrent/FutureTask.get()`
- `java/util/concurrent/ForkJoinTask.get()`

### Examples (reported)
```java
package com.example;
import java.util.concurrent.Future;
public class ClassA {
    public Object methodX(Future<Object> varOne) throws Exception {
        return varOne.get();
    }
}
```

```java
package com.example;
import java.util.concurrent.CompletableFuture;
public class ClassB {
    public Object methodY(CompletableFuture<Object> varOne) throws Exception {
        return varOne.get();
    }
}
```

## What This Rule Does Not Report
- Timed waits such as `get(long, TimeUnit)`.
- Other APIs such as `getNow(...)` or `join()`.
- Calls that appear only in classpath/dependency classes outside the analysis target.

### Examples (not reported)
```java
package com.example;
import java.util.concurrent.Future;
import java.util.concurrent.TimeUnit;
public class ClassC {
    public Object methodZ(Future<Object> varOne) throws Exception {
        return varOne.get(1L, TimeUnit.SECONDS);
    }
}
```

## Recommended Fix
Use bounded waiting (`get(timeout, unit)`) or switch to non-blocking composition APIs to avoid indefinite blocking.

## Message Shape
Findings are reported as `Avoid timeout-free Future.get() in <class>.<method><descriptor>; prefer get(timeout, unit) or non-blocking composition.`
