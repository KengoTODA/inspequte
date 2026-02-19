# Rule Plan: url_equals_call

## Summary
Detect direct calls to `java.net.URL.equals(Object)`.

## Problem framing
`URL.equals` may trigger host resolution and can have surprising behavior/performance. Many code paths intend structural equality and should compare normalized URI or explicit components.

## Scope
- Analyze call sites in analysis target classes only.
- Report exact invocations of `java/net/URL.equals(Ljava/lang/Object;)Z`.
- Emit one finding per matching call site with class/method context.

## Non-goals
- Do not infer protocol-specific equivalence requirements.
- Do not add suppression semantics via `@Suppress` / `@SuppressWarnings`.
- Do not add non-JSpecify annotation semantics.

## Detection strategy
1. Iterate analysis target classes, methods, and call sites.
2. Match owner/name/descriptor exactly for `URL.equals(Object)`.
3. Resolve line number from bytecode offset when available.
4. Emit deterministic findings in traversal order.

## Rule message
- Problem: `URL.equals` can be expensive and semantically surprising.
- Fix: compare normalized `URI` values or explicit URL components depending on intent.

## Test strategy
- TP: direct `urlA.equals(urlB)` is reported.
- TN: `urlA.toURI().equals(urlB.toURI())` is not reported.
- Edge: classpath-only classes are ignored.

## Complexity and determinism
- Linear in number of call sites (`O(C)`).
- Deterministic by stable class/method/call iteration.

## Annotation policy
- `@Suppress`-style suppression remains unsupported.
- Annotation-driven semantics remain JSpecify-only.
- Non-JSpecify annotations do not affect behavior.

## Risks
- [ ] Some projects may intentionally rely on URL-level equality semantics.
- [ ] Rule cannot determine the exact domain intent of equality checks.

## Post-Mortem
- Went well: matching a single JDK API signature kept detection simple and deterministic.
- Tricky: message wording needed to stay actionable without assuming one universal URL equality policy.
- Follow-up: evaluate whether a future rule should suggest project-specific normalization utilities when available.
