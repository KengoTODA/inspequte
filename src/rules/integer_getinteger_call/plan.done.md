# Rule Plan: integer_getinteger_call

## Summary
Detect direct calls to `Integer.getInteger(...)`.

## Problem framing
`Integer.getInteger(...)` reads JVM system properties, not numeric string values. It is frequently confused with `Integer.parseInt(...)`/`Integer.valueOf(...)`.

## Scope
- Analyze call sites in analysis target classes only.
- Report direct calls to `java/lang/Integer.getInteger` overloads:
  - `(Ljava/lang/String;)Ljava/lang/Integer;`
  - `(Ljava/lang/String;I)Ljava/lang/Integer;`
  - `(Ljava/lang/String;Ljava/lang/Integer;)Ljava/lang/Integer;`
- Emit one finding per matching call site with class/method context.

## Non-goals
- Do not infer user intent from surrounding variable names or comments.
- Do not model whether a system property is intentionally used.
- Do not report `Integer.parseInt(...)` or `Integer.valueOf(...)`.
- Do not add suppression semantics via `@Suppress` / `@SuppressWarnings`.
- Do not add non-JSpecify annotation semantics.

## Detection strategy
1. Iterate analysis target classes, methods, and call sites.
2. Match owner/name/descriptor exactly for `Integer.getInteger(...)` overloads.
3. Resolve source line from bytecode offset when available.
4. Emit deterministic findings in traversal order.

## Rule message
- Problem: `Integer.getInteger(...)` reads system properties and is often a parse mistake.
- Fix: use `Integer.parseInt(...)`/`Integer.valueOf(...)` for numeric parsing.

## Test strategy
- TP: `Integer.getInteger(String)` is reported.
- TP: `Integer.getInteger(String, int)` is reported.
- TN: `Integer.parseInt(String)` is not reported.
- Edge: classpath-only calls are ignored.

## Complexity and determinism
- Linear in number of call sites (`O(C)`).
- Deterministic by stable class/method/call iteration.

## Annotation policy
- `@Suppress`-style suppression remains unsupported.
- Annotation-driven semantics remain JSpecify-only.
- Non-JSpecify annotations do not affect behavior.

## Risks
- [ ] Some code intentionally reads integer-valued system properties and may be reported.
- [ ] Rule cannot infer intent when both parsing and property reads are plausible.

## Post-mortem
- What went well: overload-level descriptor matching kept behavior explicit and low-noise.
- What was tricky: balancing guidance between parse mistakes and legitimate system-property reads.
- Follow-up: if needed, consider optional heuristics for property-key-like string literals via future spec change.
