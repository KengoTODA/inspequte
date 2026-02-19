# Rule Plan: bigdecimal_divide_without_rounding

## Summary
Detect calls to `BigDecimal.divide(BigDecimal)` without explicit rounding configuration.

## Problem framing
`BigDecimal.divide(BigDecimal)` can throw `ArithmeticException` for non-terminating decimal results. Many call sites should make rounding behavior explicit.

## Scope
- Analyze call sites in analysis target classes only.
- Report exact invocations of `java/math/BigDecimal.divide(Ljava/math/BigDecimal;)Ljava/math/BigDecimal;`.
- Emit one finding per matching call site with class/method context.

## Non-goals
- Do not infer whether operands always produce terminating decimal results.
- Do not add suppression semantics via `@Suppress` / `@SuppressWarnings`.
- Do not add non-JSpecify annotation semantics.

## Detection strategy
1. Iterate analysis target classes, methods, and call sites.
2. Match owner/name/descriptor exactly for the one-argument `divide` overload.
3. Resolve source line from bytecode offset when available.
4. Emit deterministic findings in traversal order.

## Rule message
- Problem: divide without rounding can fail at runtime.
- Fix: use overloads that specify `RoundingMode` or `MathContext`.

## Test strategy
- TP: one-argument `divide` call is reported.
- TN: `divide(..., RoundingMode)` is not reported.
- TN: `divide(..., MathContext)` is not reported.
- Edge: classpath-only classes are ignored.

## Complexity and determinism
- Linear in number of call sites (`O(C)`).
- Deterministic by stable class/method/call iteration.

## Annotation policy
- `@Suppress`-style suppression remains unsupported.
- Annotation-driven semantics remain JSpecify-only.
- Non-JSpecify annotations do not affect behavior.

## Risks
- [ ] Some call sites guarantee terminating decimals and are safe.
- [ ] Rule favors explicitness over proving arithmetic properties.

## Post-Mortem
- Went well: overload-specific descriptor matching made the rule deterministic and easy to validate.
- Tricky: this rule intentionally trades some precision for explicitness because proving terminating decimals statically is non-trivial.
- Follow-up: evaluate optional precision improvements for constant operands in a future iteration.
