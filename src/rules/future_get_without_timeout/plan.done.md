# Rule Plan: future_get_without_timeout

## Summary
Detect timeout-free blocking calls to `Future.get()`.

## Problem framing
Calling `Future.get()` without a timeout can block indefinitely and cause stalled request handling or thread-pool starvation under failure conditions.

## Scope
- Analyze call sites in analysis target classes only.
- Report direct timeout-free blocking calls to zero-argument `get()` on:
  - `java/util/concurrent/Future`
  - `java/util/concurrent/CompletableFuture`
  - `java/util/concurrent/FutureTask`
  - `java/util/concurrent/ForkJoinTask`
  - additional `java/util/concurrent/*Future` owners with the same signature
- Emit one finding per matching call site with class/method context.

## Non-goals
- Do not infer whether the target `Future` is guaranteed to complete quickly.
- Do not model calling context (for example background worker vs request thread).
- Do not report timeout overloads such as `get(long, TimeUnit)`.
- Do not report non-`get` APIs such as `join()` or `getNow(...)`.
- Do not add suppression semantics via `@Suppress` / `@SuppressWarnings`.
- Do not add non-JSpecify annotation semantics.

## Detection strategy
1. Iterate analysis target classes, methods, and call sites.
2. Match timeout-free `get()` by owner/name/descriptor.
3. Resolve source line from bytecode offset when available.
4. Emit deterministic findings in traversal order.

## Rule message
- Problem: timeout-free `Future.get()` may block indefinitely.
- Fix: use timed waits (`get(timeout, unit)`) or non-blocking composition.

## Test strategy
- TP: `Future.get()` is reported.
- TP: `CompletableFuture.get()` is reported.
- TN: timeout overload `get(long, TimeUnit)` is not reported.
- TN: `getNow(...)` is not reported.
- Edge: classpath-only calls are ignored.

## Complexity and determinism
- Linear in number of call sites (`O(C)`).
- Deterministic by stable class/method/call iteration.

## Annotation policy
- `@Suppress`-style suppression remains unsupported.
- Annotation-driven semantics remain JSpecify-only.
- Non-JSpecify annotations do not affect behavior.

## Risks
- [ ] Some intentionally blocking workflows may accept timeout-free waits.
- [ ] Without runtime context, rule may report valid blocking calls in batch/offline paths.

## Post-mortem
- What went well: owner/name plus zero-arg `get()` matching kept implementation simple and deterministic.
- What was tricky: balancing owner coverage for JDK `Future` implementations without over-matching unrelated `get()` APIs.
- Follow-up: if false positives appear, refine owner matching with class-hierarchy checks for known `Future` subtypes.
