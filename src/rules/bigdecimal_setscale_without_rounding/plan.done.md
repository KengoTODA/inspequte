# Rule Plan: bigdecimal_setscale_without_rounding

## Summary
Detect calls to `BigDecimal.setScale(int)` without explicit rounding mode.

## Problem framing
`BigDecimal.setScale(int)` can throw `ArithmeticException` when rounding is required. Many call sites should explicitly define rounding behavior.

## Scope
- Analyze call sites in analysis target classes only.
- Report exact invocations of `java/math/BigDecimal.setScale(I)Ljava/math/BigDecimal;`.
- Emit one finding per matching call site with class/method context.

## Non-goals
- Do not infer whether values always fit the target scale exactly.
- Do not add suppression semantics via `@Suppress` / `@SuppressWarnings`.
- Do not add non-JSpecify annotation semantics.

## Detection strategy
1. Iterate analysis target classes, methods, and call sites.
2. Match owner/name/descriptor exactly for one-argument `setScale`.
3. Resolve source line from bytecode offset when available.
4. Emit deterministic findings in traversal order.

## Rule message
- Problem: setScale without rounding can fail at runtime.
- Fix: use overload with `RoundingMode` for explicit behavior.

## Test strategy
- TP: one-argument `setScale` is reported.
- TN: two-argument `setScale(..., RoundingMode)` is not reported.
- Edge: classpath-only classes are ignored.

## Complexity and determinism
- Linear in number of call sites (`O(C)`).
- Deterministic by stable class/method/call iteration.

## Annotation policy
- `@Suppress`-style suppression remains unsupported.
- Annotation-driven semantics remain JSpecify-only.
- Non-JSpecify annotations do not affect behavior.

## Risks
- [ ] Some call sites may guarantee exact scaling and be safe.
- [ ] Rule favors explicitness over proving numeric properties.

## Post-Mortem
- Went well: one-overload signature matching provided deterministic behavior with minimal implementation complexity.
- Tricky: the rule intentionally reports potential risk even when exact scale may be guaranteed, favoring explicit rounding semantics.
- Follow-up: consider optional precision enhancements for constant-scale proven-safe paths.
