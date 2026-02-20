# Rule Plan: object_wait_without_timeout

## Summary
Detect direct calls to `Object.wait()` without timeout.

## Problem framing
`Object.wait()` without a timeout can block indefinitely and lead to hangs when notifications are missed or synchronization assumptions break.

## Scope
- Analyze call sites in analysis target classes only.
- Report direct calls to `java/lang/Object.wait()V`.
- Emit one finding per matching call site with class/method context.

## Non-goals
- Do not infer whether notify/notifyAll is guaranteed by surrounding code.
- Do not model lock ownership correctness beyond JVM bytecode validity.
- Do not report timed waits (`wait(long)` / `wait(long, int)`).
- Do not add suppression semantics via `@Suppress` / `@SuppressWarnings`.
- Do not add non-JSpecify annotation semantics.

## Detection strategy
1. Iterate analysis target classes, methods, and call sites.
2. Match owner/name/descriptor exactly for `Object.wait()`.
3. Resolve source line from bytecode offset when available.
4. Emit deterministic findings in traversal order.

## Rule message
- Problem: timeout-free `wait()` can block forever.
- Fix: use timed waits and explicit condition/retry handling.

## Test strategy
- TP: `Object.wait()` is reported.
- TN: `Object.wait(long)` is not reported.
- TN: `Object.wait(long, int)` is not reported.
- Edge: classpath-only calls are ignored.

## Complexity and determinism
- Linear in number of call sites (`O(C)`).
- Deterministic by stable class/method/call iteration.

## Annotation policy
- `@Suppress`-style suppression remains unsupported.
- Annotation-driven semantics remain JSpecify-only.
- Non-JSpecify annotations do not affect behavior.

## Risks
- [ ] Some concurrency protocols intentionally rely on indefinite waits.
- [ ] Rule does not distinguish benign monitor usage from problematic production paths.

## Post-mortem
- What went well: exact owner/name/descriptor matching kept detection predictable and low-cost.
- What was tricky: avoiding noise required explicit non-report tests for timed wait overloads.
- Follow-up: if false positives emerge, evaluate optional flow-sensitive heuristics in a future spec update.
