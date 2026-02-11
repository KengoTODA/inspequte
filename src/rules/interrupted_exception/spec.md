# INTERRUPTED_EXCEPTION_NOT_RESTORED

## Summary
- Rule ID: `INTERRUPTED_EXCEPTION_NOT_RESTORED`
- Name: InterruptedException not properly handled
- Problem: Catching `InterruptedException` without restoring interrupt status can break cancellation and shutdown behavior.

## What This Rule Reports
This rule reports catch handlers that handle `InterruptedException` (including broad catches like `Exception` or `Throwable`) but do not preserve interruption semantics.

### Java Example (reported)
```java
class ClassA {
    void methodOne() {
        try {
            methodTwo();
        } catch (InterruptedException varOne) {
            System.out.println(varOne.getMessage());
        }
    }

    void methodTwo() throws InterruptedException {}
}
```

## Accepted Handling Patterns
The rule does not report when the handler does at least one of the following:
- Calls `Thread.currentThread().interrupt()`
- Rethrows `InterruptedException`
- Propagates interruption by method signature and control flow
- Restores interrupt in a `finally` block

### Java Example (not reported)
```java
class ClassA {
    void methodOne() {
        try {
            methodTwo();
        } catch (InterruptedException varOne) {
            Thread.currentThread().interrupt();
        }
    }

    void methodTwo() throws InterruptedException {}
}
```

## Recommended Fix
Restore the interrupt status or rethrow to preserve interruption semantics.

## Message Shape
Findings are reported as `InterruptedException not restored in <class>.<method><descriptor> handler`.
