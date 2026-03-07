# Rule Plan: string_trim_is_empty

## Summary
Detect direct `String.trim().isEmpty()` chains and recommend `String.isBlank()` on Java 11+.

## Problem framing
Using `trim().isEmpty()` as a blank-string check can be ambiguous and may miss some Unicode whitespace expectations. The intent is usually "blank check", and `isBlank()` expresses that intent directly.

## Scope
- Analyze call sites in analysis target classes only.
- Report direct call chains where `String.trim()` is immediately followed by `String.isEmpty()` on the trimmed value.
- Emit one finding per matching chain.

## Non-goals
- Do not report standalone `String.isEmpty()` or `String.trim()` calls.
- Do not report alternative idioms such as `trim().length() == 0`.
- Do not infer semantic intent outside the exact call chain.
- Do not add suppression semantics via `@Suppress` / `@SuppressWarnings`.
- Do not add non-JSpecify annotation semantics.

## Detection strategy
1. Iterate analysis target classes, methods, and call sites.
2. Match adjacent call-site pairs where:
   - first call: `java/lang/String.trim()Ljava/lang/String;`
   - second call: `java/lang/String.isEmpty()Z`
3. Require bytecode adjacency for direct chaining (`first.offset + opcode_length == second.offset`).
4. Emit deterministic findings in traversal order.

## Rule message
- Problem: `trim().isEmpty()` may be unclear for whitespace semantics.
- Fix: replace with `isBlank()` (Java 11+).

## Test strategy
- TP: direct `trim().isEmpty()` is reported.
- TN: `isBlank()` and plain `isEmpty()` are not reported.
- Edge: mixed usage reports only direct chain occurrences.
- Edge: classpath-only occurrences are ignored.

## Complexity and determinism
- Linear in number of call sites (`O(C)`).
- Deterministic by stable class/method/call traversal order.

## Annotation policy
- `@Suppress`-style suppression remains unsupported.
- Annotation-driven semantics remain JSpecify-only.
- Non-JSpecify annotations do not affect behavior.

## Risks
- [ ] Some codebases intentionally rely on `trim().isEmpty()` semantics that differ from `isBlank()` expectations.
- [ ] Restricting to direct call adjacency may miss logically equivalent but non-adjacent forms.
- [ ] Message wording must avoid overstating correctness while still steering toward actionable replacement.

## Post-mortem
- What went well: direct call-chain matching with bytecode adjacency kept detection deterministic and low-noise.
- What was tricky: balancing actionable guidance toward `isBlank()` without over-claiming semantic equivalence for every whitespace policy.
- Follow-up: if needed, consider a future spec revision for non-adjacent equivalents like `trim(); ...; isEmpty()` with receiver tracking.
