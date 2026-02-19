# Rule Plan: runtime_halt_call

## Summary
Detect direct calls to `Runtime.halt(int)`.

## Problem framing
`Runtime.halt(int)` forcibly terminates the JVM without running shutdown hooks or orderly cleanup, which can corrupt state and skip resource release.

## Scope
- Analyze call sites in analysis target classes only.
- Report exact invocations of `java/lang/Runtime.halt(I)V`.
- Emit one finding per matching call site with class/method context.

## Non-goals
- Do not infer operational policy for emergency termination.
- Do not add suppression semantics via `@Suppress` / `@SuppressWarnings`.
- Do not add non-JSpecify annotation semantics.

## Detection strategy
1. Iterate analysis target classes, methods, and call sites.
2. Match owner/name/descriptor exactly for `Runtime.halt(int)`.
3. Resolve source line from bytecode offset when available.
4. Emit deterministic findings in traversal order.

## Rule message
- Problem: `Runtime.halt` bypasses graceful shutdown.
- Fix: prefer orderly termination and explicit error propagation where possible.

## Test strategy
- TP: `Runtime.getRuntime().halt(1)` is reported.
- TN: `System.exit(1)` is not reported by this rule.
- Edge: classpath-only classes are ignored.

## Complexity and determinism
- Linear in number of call sites (`O(C)`).
- Deterministic by stable class/method/call iteration.

## Annotation policy
- `@Suppress`-style suppression remains unsupported.
- Annotation-driven semantics remain JSpecify-only.
- Non-JSpecify annotations do not affect behavior.

## Risks
- [ ] Some low-level runtime wrappers may intentionally call `halt`.
- [ ] Rule cannot distinguish emergency-only policies from accidental usage.

## Post-Mortem
- Went well: exact signature matching for `Runtime.halt(int)` gave a deterministic implementation with low false-positive risk.
- Tricky: the recommendation text needed to stay actionable without prescribing one shutdown framework.
- Follow-up: review whether future policy options should distinguish test-only usage from production paths.
