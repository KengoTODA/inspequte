# Plan: koin_autocloseable_not_closed

## Rule idea
Detect Koin module definitions that create an `AutoCloseable`/`Closeable` instance in a `single { ... }` definition lambda but do not register an `onClose { ... }` callback that closes it.

## Problem description
Koin can retain singleton instances for the lifetime of the container or scope. When a definition lambda constructs a resource-owning object such as a database handle, socket wrapper, or file-backed client, Koin will not automatically call `close()` unless the definition is paired with an `onClose` callback.

This is easy to miss in review because the definition and the cleanup callback are usually declared in a compact DSL chain. The code looks complete at a glance even when the resource lifecycle is incomplete.

## Detection strategy

Bytecode-level detection for an intentionally narrow Koin classic DSL pattern:

1. Scan Kotlin-compiled analysis target methods for calls to `org/koin/core/module/Module.single(...)` or `single$default(...)`.
2. Resolve the synthetic lambda implementation method referenced by the `invokedynamic` instruction that provides the definition body.
3. Confirm that the definition lambda:
   - returns a reference type,
   - constructs that returned type via a constructor call in the same lambda, and
   - the returned type implements `java/lang/AutoCloseable` or `java/io/Closeable`.
4. Search the surrounding outer method bytecode for a matching Koin `onClose(...)` callback registration that belongs to the same definition chain.
5. Resolve the synthetic lambda implementation for the `onClose` callback and verify that it contains a no-argument `close()` call.
6. Report when the definition lambda creates an `AutoCloseable` instance but:
   - no `onClose` callback is registered for that chain, or
   - the registered callback does not call `close()`.

## Scope

**In scope:**
- Kotlin-compiled Koin module methods in analysis target classes.
- `single(...)` / `single$default(...)` definition calls on `org/koin/core/module/Module`.
- `onClose(...)` registrations that appear in the same enclosing method after the definition call.
- Resource types assignable to `AutoCloseable` or `Closeable`.

**Non-goals:**
- `factory`, `scoped`, `singleOf`, `factoryOf`, `scopedOf`, or annotation/plugin-generated Koin DSL variants.
- `withOptions { onClose { ... } }` or other callback wiring patterns that do not appear as the direct `single ... onClose ...` chain in the same outer method.
- Inter-procedural cleanup tracking (for example helper methods that eventually close the resource).
- Resources managed via APIs other than `close()`.
- Definition lambdas that only retrieve an existing resource via `get()` instead of constructing one.
- Annotation-based suppression: `@Suppress`-style annotations are not supported.
- Non-JSpecify annotation semantics are not supported.

## Determinism constraints
- Iterate classes, methods, and instructions in stable bytecode order.
- Match definition and callback lambdas using deterministic nearest-call heuristics in instruction order.
- Sort findings by `(class, method, descriptor, definition offset)` before emitting.

## Test strategy
- TP: `single { ClassA() }` where `ClassA : AutoCloseable` and no `onClose` is registered.
- TP: `single { ClassA() } onClose { }` where the callback exists but does not call `close()`.
- TN: `single { ClassA() } onClose { it?.close() }`.
- TN: `single { ClassB() }` where `ClassB` does not implement `AutoCloseable`.
- Edge: two singleton definitions in the same module method, where only one is missing cleanup.
- Edge: resource class provided from classpath dependency still resolves as `AutoCloseable`.

## Complexity
- Per method, scanning instructions and matching nearby lambda registrations is linear in bytecode size: `O(I)`.
- Interface/supertype resolution for candidate resource classes is bounded by reachable type hierarchy size.
- No CFG exploration or inter-procedural dataflow is required in v1.

## Risks
- [ ] False negatives for real Koin callback styles that compile differently from the direct `single ... onClose ...` chain. Mitigation: keep scope explicit in `spec.md`.
- [ ] False positives if an `onClose` callback calls an unrelated `close()` method. Mitigation: tie callback search to the same definition chain and require `close()V`.
- [ ] Missed detections when the definition lambda creates the resource indirectly through helper/factory calls. Mitigation: constructor-based creation only in v1.
- [ ] Koin API bytecode shapes may vary across versions. Mitigation: support both `single` and `single$default`, and accept both static DSL and instance-style `onClose` owners when practical.

## Post-mortem
- Went well: the Kotlin bytecode shape for direct `single { ... } onClose { ... }` chains was stable enough to support a lightweight instruction-order matcher instead of a heavier CFG/dataflow approach.
- Tricky: Koin's broader DSL surface is wider than the directly detectable bytecode pattern, so the spec had to narrow scope aggressively to avoid claiming support for `withOptions` or plugin-generated variants.
- Follow-up: extend matching to additional Koin registration forms such as `scoped`, `singleOf`, and callback wiring that compiles through option lambdas rather than direct `onClose` chaining.
