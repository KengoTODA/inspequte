# EXECUTOR_SERVICE_NOT_SHUTDOWN

## Summary
- Rule ID: `EXECUTOR_SERVICE_NOT_SHUTDOWN`
- Name: ExecutorService not shut down
- Description: Detects locally created executor services that can reach a method exit without `shutdown()`, `shutdownNow()`, or `close()` in the same method.
- Annotation policy: `@Suppress`-style suppression is unsupported. Annotation-driven semantics support JSpecify only; non-JSpecify annotations are unsupported for this rule.

## Motivation
Creating an executor service starts worker threads and associated queues that usually need explicit shutdown. When a method creates an executor and forgets to close it on every reachable exit path, threads can leak and tests or services can hang. This is easy to miss in review, especially when early returns or exceptional paths are involved.

## What it detects
- A method locally creates an executor service by:
  - calling a supported JDK `Executors` factory that allocates a new executor service, or
  - invoking a constructor for a class assignable to `java.util.concurrent.ExecutorService`
- The created executor remains locally owned inside the same method.
- At least one reachable exit path from that creation site reaches method exit without a later call to `shutdown()`, `shutdownNow()`, or `close()` on the same locally tracked executor.
- The rule reports the local creation site whose shutdown is not guaranteed.

## What it does NOT detect
- Executors received from parameters, fields, or method returns.
- Cases where ownership is intentionally transferred out of the method, such as:
  - storing the executor into a field
  - storing the executor into an array or other heap-backed container
  - returning the executor
  - passing the executor as an argument to another method
- Proof that shutdown happens in a different helper method after transfer.
- Suppression behavior via `@Suppress` or `@SuppressWarnings`.
- Rules based on non-JSpecify annotations.

## Examples (TP/TN/Edge)
### TP (reported)
```java
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

class ClassA {
    void methodX() {
        ExecutorService varOne = Executors.newSingleThreadExecutor();
        varOne.submit(() -> {});
    }
}
```

### TN (not reported)
```java
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

class ClassA {
    void methodX() {
        ExecutorService varOne = Executors.newFixedThreadPool(1);
        try {
            varOne.submit(() -> {});
        } finally {
            varOne.shutdown();
        }
    }
}
```

### Edge (reported)
```java
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

class ClassA {
    void methodX(boolean varOne) {
        ExecutorService varTwo = Executors.newSingleThreadExecutor();
        if (varOne) {
            return;
        }
        varTwo.shutdown();
    }
}
```

## Output
- Report one finding per locally created executor service whose shutdown is not guaranteed on all reachable exits.
- Message must be actionable and include the method context, for example:
  `ExecutorService created in <class>.<method><descriptor> may exit without shutdown(); call shutdown(), shutdownNow(), or close() before the method returns.`
- Primary fix guidance: place shutdown in a `finally` block or use try-with-resources where appropriate.

## Performance considerations
- Analysis should remain bounded by method CFG size and the number of locally created executors in the method.
- Tracking must remain intraprocedural and deterministic.
- Output order and deduplication must be stable across repeated runs.

## Acceptance criteria
- Reports a locally created executor service when at least one reachable exit path after creation lacks `shutdown()`, `shutdownNow()`, or `close()` in the same method.
- Does not report when all reachable exits after creation run one of the accepted shutdown operations.
- Does not report when ownership leaves the local method scope by field store, return, or argument passing.
- Covers TP, TN, and edge cases in tests.
- Keeps `@Suppress`-style suppression unsupported and does not add non-JSpecify annotation semantics.
