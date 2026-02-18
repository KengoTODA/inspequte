# Rule Plan: explicit_gc_call

## Summary
Detect explicit garbage collection calls (`System.gc()` and `Runtime.gc()`).

## Problem framing
Explicit GC calls often hurt performance predictability and usually indicate an attempt to solve memory pressure in application code rather than via JVM/runtime tuning.

## Scope
- Analyze call sites in analysis target classes only.
- Report exact invocations of:
  - `java/lang/System.gc()V`
  - `java/lang/Runtime.gc()V`
- Emit one finding per matching call site with class/method context.

## Non-goals
- Do not infer whether a call is justified for specific benchmarking or test harness scenarios.
- Do not model transitive helper wrappers around GC APIs.
- Do not add suppression semantics via `@Suppress` / `@SuppressWarnings`.
- Do not add non-JSpecify annotation semantics.

## Detection strategy
1. Iterate analysis target classes, then methods, then call sites.
2. Match owner/name/descriptor exactly against supported explicit GC APIs.
3. Resolve source line from bytecode offset when available.
4. Emit deterministic findings in traversal order.

## Rule message
- Problem: explicit GC invocation in application/library code.
- Fix: remove explicit GC call and rely on JVM GC heuristics/configuration.

## Test strategy
- TP: `System.gc()` is reported.
- TP: `Runtime.getRuntime().gc()` is reported.
- TN: non-GC `System` calls are not reported.
- Edge: classpath-only classes using GC APIs are ignored.

## Complexity and determinism
- Linear in number of call sites (`O(C)`).
- No CFG/dataflow required.
- Deterministic by stable class/method/call iteration.

## Annotation policy
- `@Suppress`-style suppression remains unsupported.
- Annotation-driven semantics remain JSpecify-only.
- Non-JSpecify annotations do not affect behavior.

## Risks
- [ ] Potential noise in benchmark/test code where explicit GC may be intentional.
- [ ] Missed detections if explicit GC is wrapped behind helper APIs.
- [ ] Message wording should stay actionable and avoid overclaiming correctness impact.

## Post-Mortem
- Went well: exact owner/name/descriptor matching made this rule compact and deterministic with straightforward TP/TN coverage.
- Tricky: adding a new rule required updating the SARIF callgraph snapshot because tool rule metadata is part of the snapshot contract.
- Follow-up: consider an allowlist strategy for benchmark-only packages if explicit-GC noise appears in real projects.
