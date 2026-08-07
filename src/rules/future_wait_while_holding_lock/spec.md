---
type: 'Static Analysis Rule'
title: 'Future wait while holding lock'
description: 'Blocking Future waits should not happen while a lock is still held'
tags: ['jvm', 'static-analysis']
status: 'stable'
rule_id: 'FUTURE_WAIT_WHILE_HOLDING_LOCK'
---

# future_wait_while_holding_lock

## Summary
- Rule ID: `FUTURE_WAIT_WHILE_HOLDING_LOCK`
- Name: Future wait while holding lock
- Description: Reports blocking `Future.get(...)` and `CompletableFuture.join()` calls that occur while the current method still definitely holds an intrinsic monitor or a `Lock`-based lock.
- Rationale for users: Waiting for a future while a lock is held can deadlock, stall other threads, and make critical sections much longer than intended.
- Annotation policy: `@Suppress` / `@SuppressWarnings` style suppression is unsupported. Only JSpecify annotations are eligible for annotation-driven semantics, and non-JSpecify annotations must not change this rule's behavior.

## Motivation
Blocking inside a critical section is a high-risk concurrency pattern. If one thread waits for a future while still holding a monitor or explicit lock, other threads may be prevented from making progress, including the thread that would complete the future. Even when no deadlock occurs, the lock stays held longer than necessary and can serialize unrelated work.

This rule exists to flag these waits at the call site and push the code toward a safer structure: finish the critical section first, then wait, or refactor so the blocking work does not happen while the lock is held.

## What it detects
This rule reports a finding when all of the following are true:

- The current method contains a call to one of these blocking waits:
  - `java.util.concurrent.Future.get()`
  - `java.util.concurrent.Future.get(long, java.util.concurrent.TimeUnit)`
  - `java.util.concurrent.CompletableFuture.join()`
- At the point of that call, the method still definitely holds at least one of these lock types:
  - an intrinsic monitor from a `synchronized` method
  - an intrinsic monitor from a `synchronized` block
  - a `java.util.concurrent.locks.Lock`-style lock acquired in the same method and not definitely released yet
- The rule can determine the lock-held state with enough confidence to keep the result precise.

The finding should be attached to the blocking wait call and should tell the user to release the lock before waiting, or to move the wait outside the critical section.

## What it does NOT detect
This rule does not report:

- Waits that occur after the relevant lock is definitely released on all paths before the wait.
- Non-blocking future APIs such as `thenApply`, `whenComplete`, `handle`, or `getNow`.
- Cases that require interprocedural reasoning, such as a helper method that acquires or releases the lock.
- Cases where lock identity or lock-held state cannot be determined reliably enough to stay precise.
- General deadlock detection beyond the local pattern "blocking future wait while a lock is still held in the same method".
- Findings suppressed via `@Suppress`, `@SuppressWarnings`, or similar annotations. Suppression annotations are unsupported.
- Annotation-driven behavior from non-JSpecify annotations.

## Examples (TP/TN/Edge)
### True positive: `synchronized` block
```java
import java.util.concurrent.Future;

class ClassA {
    private final Object lock = new Object();

    void methodX(Future<String> varOne) throws Exception {
        synchronized (lock) {
            varOne.get();
        }
    }
}
```

Reason: the wait happens while the monitor for `lock` is still held.

### True positive: explicit lock
```java
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.locks.Lock;
import java.util.concurrent.locks.ReentrantLock;

class ClassB {
    private final Lock lock = new ReentrantLock();

    void methodY(CompletableFuture<String> varOne) {
        lock.lock();
        try {
            varOne.join();
        } finally {
            lock.unlock();
        }
    }
}
```

Reason: the wait happens before the lock is released.

### True negative: wait after release
```java
import java.util.concurrent.Future;
import java.util.concurrent.locks.Lock;
import java.util.concurrent.locks.ReentrantLock;

class ClassC {
    private final Lock lock = new ReentrantLock();

    void methodX(Future<String> varOne) throws Exception {
        lock.lock();
        try {
            tmpValue();
        } finally {
            lock.unlock();
        }
        varOne.get();
    }

    void tmpValue() {}
}
```

Reason: the wait happens after the lock is no longer held.

### True negative: wait outside synchronized region
```java
import java.util.concurrent.CompletableFuture;

class ClassD {
    private final Object lock = new Object();

    void methodY(CompletableFuture<String> varOne) {
        synchronized (lock) {
            tmpValue();
        }
        varOne.join();
    }

    void tmpValue() {}
}
```

Reason: the monitor is released before the wait call.

### Edge case: timed get is still blocking
```java
import java.util.concurrent.Future;
import java.util.concurrent.TimeUnit;

class ClassE {
    private final Object lock = new Object();

    void methodX(Future<String> varOne) throws Exception {
        synchronized (lock) {
            varOne.get(10L, TimeUnit.SECONDS);
        }
    }
}
```

Expected behavior: report a finding, because a timed `get(...)` still blocks while the monitor is held.

### Edge case: ambiguous lock tracking is skipped
```java
import java.util.concurrent.Future;
import java.util.concurrent.locks.Lock;

class ClassF {
    void methodY(Lock varOne, Lock varTwo, Future<String> varThree) throws Exception {
        Lock tmpValue = pick(varOne, varTwo);
        tmpValue.lock();
        try {
            varThree.get();
        } finally {
            tmpValue.unlock();
        }
    }

    Lock pick(Lock varOne, Lock varTwo) {
        return System.nanoTime() > 0 ? varOne : varTwo;
    }
}
```

Expected behavior: the rule may skip reporting if it cannot reliably determine the held-lock identity without sacrificing precision.

## Output
The rule emits one finding for each reported wait call site.

The message must be intuitive and actionable. It should clearly say that the code is waiting on a future while a lock is still held, explain that this can deadlock or block other threads, and tell the user what to do next.

Expected message shape:

`Do not wait on a Future while holding a lock; release the lock before calling get()/join(), or move the wait outside the synchronized or locked section.`

Equivalent wording is acceptable if it preserves the same meaning and remains specific to the wait call site.

## Performance considerations
This rule is intentionally limited to same-method analysis. It should have predictable cost relative to the size of the current method and its local control flow, without requiring whole-program reasoning.

The rule should prefer precision over recall. If proving that a lock is still held would require broad alias reasoning or uncertain path reconstruction, the rule should not report.

## Acceptance criteria
- Reports `Future.get()` inside a `synchronized` method or `synchronized` block when the wait occurs before monitor release.
- Reports timed `Future.get(long, TimeUnit)` under the same conditions.
- Reports `CompletableFuture.join()` when a `Lock`-based lock acquired in the same method is still definitely held.
- Does not report waits that occur only after the relevant monitor or lock is definitely released.
- Does not report non-blocking future APIs.
- Does not require or imply interprocedural reasoning.
- Keeps messages user-facing, specific to the wait site, and actionable.
- Treats `@Suppress`-style suppression as unsupported and allows only JSpecify for annotation-driven semantics.
