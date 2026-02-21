# Plan: explicit_finalize_call

## Rule idea
Detect direct virtual calls to `finalize()` on object instances, which bypass GC lifecycle management and are always a resource management mistake.

## Scope
- Detect any bytecode-level `invokevirtual` call where method name is `finalize` and descriptor is `()V`, regardless of the owner class.
- Report one finding per such call site.
- Only analyze classes that are analysis targets (not classpath-only dependencies).

## Non-goals
- Do not flag `super.finalize()` inside `finalize()` overrides — those use `invokespecial` (`CallKind::Special`) and are legitimate.
- Do not flag overriding `finalize()` declarations themselves (only call sites).
- Do not support `@Suppress`-based suppression.
- Do not implement annotation-driven semantics beyond JSpecify scope (this rule has no annotation-driven semantics).
- Do not perform inter-method or inter-class data flow analysis.

## Detection strategy
1. Iterate over analysis target classes.
2. For each method, iterate over call sites.
3. Flag any call where:
   - `call.kind == CallKind::Virtual` (invokevirtual)
   - `call.name == "finalize"`
   - `call.descriptor == "()V"`
4. Emit one finding per call site, reporting the method context.

## Rationale for scope
Explicit `obj.finalize()` is always `invokevirtual` in bytecode; `super.finalize()` is always `invokespecial`. The two call kinds are mutually exclusive in practice. This makes the detection precise and avoids false positives on legitimate finalize override chains.

## Test strategy
- TP: class with a method that explicitly calls `obj.finalize()` on a local variable → finding reported.
- TP: class with a method that calls `this.finalize()` explicitly → finding reported.
- TN: class that overrides `finalize()` and calls `super.finalize()` → no finding (invokespecial).
- TN: class that overrides `finalize()` without calling super → no finding on the declaration itself.
- TN: class that calls any other `void` method named differently → no finding.
- TN: classpath-only class with an explicit `obj.finalize()` call → not in scope, no finding.

## Determinism constraints
- Results are emitted in class iteration order, then method iteration order, then call site offset order.
- No hash maps or unordered sets are used to generate findings.

## Risks
- [ ] False negatives: synthetic or generated code might call finalize() differently — acceptable, out of scope.
- [ ] Suppression gap: no suppression mechanism exists (by policy).
- [ ] Noisy in codebases with heavy legacy finalizer use: acceptable, each finding is a real bug.
- [ ] `invokespecial finalize` called outside finalize methods (e.g., some obfuscated code) — acceptable false negative.

## Annotation policy
- No `@Suppress`-style suppression is supported.
- No JSpecify annotation semantics are involved in this rule.
