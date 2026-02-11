# LOCK_NOT_RELEASED_ON_EXCEPTION_PATH

## Summary
- Rule ID: `LOCK_NOT_RELEASED_ON_EXCEPTION_PATH`
- Name: Lock acquired without guaranteed release
- Description: Detects methods where `Lock.lock()` is followed by at least one reachable exit path without a subsequent `unlock()` in the same method.
- Annotation policy: `@Suppress`-style suppression is unsupported. Annotation-driven semantics support JSpecify only; non-JSpecify annotations are unsupported for this rule.

## Motivation
Failing to release a lock on all reachable paths can cause deadlocks, request stalls, and thread starvation. The bug is often hidden in exceptional paths or early returns and is hard to catch with manual review.

## What it detects
- A method invokes `lock()` on `java.util.concurrent.locks.Lock` or `java.util.concurrent.locks.ReentrantLock`.
- From that acquisition point, at least one reachable method exit path does not execute `unlock()` later in the same method.
- The rule reports the acquisition site that is missing guaranteed release.

## What it does NOT detect
- Cases where release happens only in a different method.
- Proof that `lock()` and `unlock()` target the exact same runtime receiver instance.
- Rules based on non-JSpecify annotations.
- Any suppression behavior via `@Suppress` or `@SuppressWarnings`.

## Examples (TP/TN/Edge)
### TP (reported)
```java
import java.util.concurrent.locks.Lock;
import java.util.concurrent.locks.ReentrantLock;

class ClassA {
    private final Lock varOne = new ReentrantLock();

    void methodX() {
        varOne.lock();
        if (System.currentTimeMillis() > 0) {
            throw new RuntimeException("tmpValue");
        }
        varOne.unlock();
    }
}
```

### TN (not reported)
```java
import java.util.concurrent.locks.Lock;
import java.util.concurrent.locks.ReentrantLock;

class ClassA {
    private final Lock varOne = new ReentrantLock();

    void methodX() {
        varOne.lock();
        try {
            tmpAction();
        } finally {
            varOne.unlock();
        }
    }

    void tmpAction() {}
}
```

### Edge (reported once for unsafe site)
```java
import java.util.concurrent.locks.Lock;
import java.util.concurrent.locks.ReentrantLock;

class ClassA {
    private final Lock varOne = new ReentrantLock();

    void methodX(boolean varTwo) {
        varOne.lock();
        try {
            if (varTwo) {
                return;
            }
        } finally {
            varOne.unlock();
        }

        varOne.lock();
        if (varTwo) {
            throw new IllegalStateException("tmpValue");
        }
        varOne.unlock();
    }
}
```

## Output
- Report one finding per unsafe lock acquisition site.
- Message must be actionable and include the method context, for example:
  `Lock acquired in <class>.<method><descriptor> may exit without unlock(); release it in a finally block.`
- Primary fix guidance: place `unlock()` in a `finally` block that always runs after `lock()`.

## Performance considerations
- Analysis should remain bounded by method CFG size and number of lock acquisitions in the method.
- Traversal order and output order must be deterministic.

## Acceptance criteria
- Reports when a `lock()` site has at least one reachable exit path without `unlock()` afterward in the same method.
- Does not report when all reachable exits after `lock()` pass through an `unlock()`.
- Covers TP, TN, and edge cases in tests.
- Produces deterministic finding order and count across repeated runs.
- Keeps `@Suppress`-style suppression unsupported and does not add non-JSpecify annotation semantics.
