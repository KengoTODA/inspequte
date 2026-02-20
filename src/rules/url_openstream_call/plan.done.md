# Rule Plan: url_openstream_call

## Summary
Detect direct calls to `URL.openStream()`.

## Problem framing
`URL.openStream()` hides connection configuration details and often leads to missing timeout settings. This can cause indefinite blocking and weak network control.

## Scope
- Analyze call sites in analysis target classes only.
- Report direct calls to:
  - `java/net/URL.openStream()Ljava/io/InputStream;`
- Do not report `openStream()` when it is directly chained from:
  - `java/lang/Class.getResource(Ljava/lang/String;)Ljava/net/URL;`
  - `java/lang/ClassLoader.getResource(Ljava/lang/String;)Ljava/net/URL;`
- Emit one finding per matching call site with class/method context.

## Non-goals
- Do not prove whether timeout settings are already configured elsewhere.
- Do not inspect downstream stream handling or resource closing behavior.
- Do not add suppression semantics via `@Suppress` / `@SuppressWarnings`.
- Do not add non-JSpecify annotation semantics.

## Detection strategy
1. Iterate analysis target classes, methods, and call sites.
2. Match owner/name/descriptor exactly for `URL.openStream()`.
3. Resolve source line from bytecode offset when available.
4. Emit deterministic findings in traversal order.

## Rule message
- Problem: `URL.openStream()` obscures connection controls and can miss timeouts.
- Fix: use `openConnection()` and set explicit connect/read timeouts with structured resource handling.

## Test strategy
- TP: `URL.openStream()` is reported.
- TN: `URL.openConnection()` is not reported.
- TN: `Class.getResource(...).openStream()` is not reported.
- TN: `ClassLoader.getResource(...).openStream()` is not reported.
- Edge: classpath-only calls are ignored.

## Complexity and determinism
- Linear in number of call sites (`O(C)`).
- Deterministic by stable class/method/call iteration.

## Annotation policy
- `@Suppress`-style suppression remains unsupported.
- Annotation-driven semantics remain JSpecify-only.
- Non-JSpecify annotations do not affect behavior.

## Risks
- [ ] Some code may intentionally rely on default connection behavior and still be reported.
- [ ] Rule does not guarantee safer replacement usage after migration.

## Post-mortem
- What went well: single-signature matching produced a precise and deterministic rule.
- What was tricky: keeping guidance actionable without overpromising automatic safety.
- Follow-up: consider future extension to detect missing timeout calls after `openConnection()` use.
