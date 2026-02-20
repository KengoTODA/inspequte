# Rule Plan: string_intern_call

## Summary
Detect direct calls to `String.intern()`.

## Problem framing
`String.intern()` can increase memory pressure and introduce global string-pool contention, especially in long-lived or high-throughput systems.

## Scope
- Analyze call sites in analysis target classes only.
- Report direct calls to `java/lang/String.intern()Ljava/lang/String;`.
- Emit one finding per matching call site with class/method context.

## Non-goals
- Do not infer whether call volume is low enough to be harmless.
- Do not evaluate JVM string deduplication settings.
- Do not flag other string operations (`toString`, `valueOf`, concatenation).
- Do not add suppression semantics via `@Suppress` / `@SuppressWarnings`.
- Do not add non-JSpecify annotation semantics.

## Detection strategy
1. Iterate analysis target classes, methods, and call sites.
2. Match owner/name/descriptor exactly for `String.intern()`.
3. Resolve source line from bytecode offset when available.
4. Emit deterministic findings in traversal order.

## Rule message
- Problem: `String.intern()` may create memory/performance bottlenecks.
- Fix: avoid interning dynamic strings; use bounded caches or domain-specific canonicalization instead.

## Test strategy
- TP: `String.intern()` is reported.
- TN: `String.toString()` is not reported.
- Edge: classpath-only calls are ignored.

## Complexity and determinism
- Linear in number of call sites (`O(C)`).
- Deterministic by stable class/method/call iteration.

## Annotation policy
- `@Suppress`-style suppression remains unsupported.
- Annotation-driven semantics remain JSpecify-only.
- Non-JSpecify annotations do not affect behavior.

## Risks
- [ ] Some canonicalization-heavy domains may intentionally use `intern()`.
- [ ] Rule does not model call frequency or memory profile context.

## Post-mortem
- What went well: exact call-site matching for `String.intern()` yielded clear behavior and deterministic output.
- What was tricky: keeping recommendations actionable without over-asserting runtime memory impact.
- Follow-up: if needed, consider optional context filters for generated/string-table bootstrap code via spec revision.
