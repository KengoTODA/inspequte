# Rule Plan: optional_get_call

## Summary
Detect direct calls to `Optional.get()` and primitive `Optional*#getAs*()` accessors.

## Problem framing
Direct getter calls on `Optional` values throw when empty and can hide error handling assumptions. Safer alternatives (`orElse`, `orElseThrow`, `ifPresent`) make empty handling explicit.

## Scope
- Analyze call sites in analysis target classes only.
- Report exact invocations of:
  - `java/util/Optional.get()Ljava/lang/Object;`
  - `java/util/OptionalInt.getAsInt()I`
  - `java/util/OptionalLong.getAsLong()J`
  - `java/util/OptionalDouble.getAsDouble()D`
- Emit one finding per matching call site with class/method context.

## Non-goals
- Do not perform control-flow validation around `isPresent()`/`isEmpty()` guards.
- Do not infer project-specific Optional usage policies.
- Do not add suppression semantics via `@Suppress` / `@SuppressWarnings`.
- Do not add non-JSpecify annotation semantics.

## Detection strategy
1. Iterate analysis target classes, then methods, then call sites.
2. Match owner/name/descriptor exactly for target Optional getter APIs.
3. Resolve source line from bytecode offset when available.
4. Emit deterministic findings in traversal order.

## Rule message
- Problem: direct Optional getter can throw on empty.
- Fix: use `orElse`, `orElseThrow`, or `ifPresent`-style handling.

## Test strategy
- TP: `Optional.empty().get()` is reported.
- TP: `OptionalInt.empty().getAsInt()` is reported.
- TN: `orElse(...)` forms are not reported.
- Edge: classpath-only classes with Optional getters are ignored.

## Complexity and determinism
- Linear in number of call sites (`O(C)`).
- No CFG/dataflow required.
- Deterministic by stable class/method/call iteration.

## Annotation policy
- `@Suppress`-style suppression remains unsupported.
- Annotation-driven semantics remain JSpecify-only.
- Non-JSpecify annotations do not affect behavior.

## Risks
- [ ] Potential false positives when code already guarantees non-empty Optional.
- [ ] Missed detections when Optional getter calls are hidden behind wrappers.
- [ ] Message should remain advisory and actionable.

## Post-Mortem
- Went well: exact owner/name/descriptor matching provided deterministic implementation and straightforward TP/TN coverage.
- Tricky: snapshot and rule count updates were required after registration to keep SARIF metadata tests aligned.
- Follow-up: evaluate whether a future path-sensitive variant should suppress findings under explicit non-empty guards.
