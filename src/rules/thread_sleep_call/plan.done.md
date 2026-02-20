# Rule Plan: thread_sleep_call

## Summary
Detect direct calls to `Thread.sleep(...)`.

## Problem framing
`Thread.sleep(...)` introduces blocking behavior and timing-coupled coordination. It is often brittle and can hide race conditions or latency issues.

## Scope
- Analyze call sites in analysis target classes only.
- Report direct calls to:
  - `java/lang/Thread.sleep(J)V`
  - `java/lang/Thread.sleep(JI)V`
- Emit one finding per matching call site with class/method context.

## Non-goals
- Do not infer whether blocking is intentional or required by the runtime contract.
- Do not report waiting APIs outside `Thread.sleep(...)`.
- Do not add suppression semantics via `@Suppress` / `@SuppressWarnings`.
- Do not add non-JSpecify annotation semantics.

## Detection strategy
1. Iterate analysis target classes, methods, and call sites.
2. Match owner/name/descriptor exactly for `Thread.sleep` overloads.
3. Resolve source line from bytecode offset when available.
4. Emit deterministic findings in traversal order.

## Rule message
- Problem: blocking sleep is timing-based and brittle.
- Fix: prefer explicit coordination primitives or scheduler/time-based abstractions.

## Test strategy
- TP: `Thread.sleep(long)` is reported.
- TP: `Thread.sleep(long, int)` is reported.
- TN: `Thread.currentThread()` is not reported.
- Edge: classpath-only calls are ignored.

## Complexity and determinism
- Linear in number of call sites (`O(C)`).
- Deterministic by stable class/method/call iteration.

## Annotation policy
- `@Suppress`-style suppression remains unsupported.
- Annotation-driven semantics remain JSpecify-only.
- Non-JSpecify annotations do not affect behavior.

## Risks
- [ ] Some low-level code intentionally uses sleeps for throttling/backoff and may be reported.
- [ ] Rule does not infer safer alternatives from surrounding framework context.

## Post-mortem
- What went well: signature-level matching across both overloads kept the implementation simple and deterministic.
- What was tricky: balancing actionable wording without assuming one universal replacement for every sleep usage.
- Follow-up: if noise appears, consider a spec change that scopes reporting by package or execution context.
