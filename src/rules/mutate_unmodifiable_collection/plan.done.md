# Plan: mutate_unmodifiable_collection

## Objective
Detect mutation calls on collections that are proven to come from JDK unmodifiable factories or wrappers in the same method.

## Problem framing
JDK factory methods such as `List.of(...)` and wrappers such as `Collections.unmodifiableList(...)` return collections that throw `UnsupportedOperationException` when mutated. These failures often reach production because the code type-checks but fails only at runtime.

## Scope
- Analyze method bytecode to track collection values created from known unmodifiable JDK APIs.
- Report when a known mutator is called on a tracked unmodifiable value in the same method.
- Cover common collection interfaces (`Collection`/`List`/`Set`/`Map`) and their mutator methods.

## Non-goals
- Inter-procedural provenance tracking across method boundaries.
- Receiver identity proof across complex aliasing beyond same-method local/stack tracking.
- Suppression behavior via `@Suppress`/`@SuppressWarnings`.
- Annotation-driven semantics from non-JSpecify annotations.

## Detection strategy
1. Iterate method bytecode in deterministic instruction order.
2. Track abstract reference values on stack and locals (`unknown` vs `known-unmodifiable`).
3. Mark values as `known-unmodifiable` when they come from known JDK APIs:
   - `List.of`, `List.copyOf`
   - `Set.of`, `Set.copyOf`
   - `Map.of`, `Map.ofEntries`, `Map.copyOf`
   - `Collections.unmodifiable*`, `Collections.empty*`, `Collections.singleton*`
4. On each collection mutator call (`add`, `remove`, `clear`, `put`, `replace`, `compute`, etc.), report if the receiver is `known-unmodifiable`.
5. Emit one deterministic finding per mutation call site.

## Determinism constraints
- Preserve class/method/instruction traversal order.
- Use stable offset-based callsite lookup.
- Avoid hash-order-dependent result collection.

## Complexity and performance
- Per method analysis is linear in bytecode length plus descriptor parsing at invoke sites.
- Memory usage is bounded by method local variable count and operand stack depth.

## Test strategy
- TP: mutate a `List.of(...)` result.
- TN: create mutable copy (`new ArrayList<>(List.of(...))`) and mutate the copy.
- Edge: method with both mutable and unmodifiable receivers where only unmodifiable mutation is reported.

## Risks
- [ ] False negatives when provenance is lost through unsupported stack/local operations.
- [ ] False negatives for unmodifiable sources not listed in scope.
- [ ] False positives if future JDK/library APIs with same names have different mutability semantics.
- [ ] Runtime overhead from descriptor parsing in very large methods.

## Post-mortem
- Went well: same-method stack/local provenance was enough to implement TP/TN/edge coverage with deterministic outputs.
- Tricky: accurately keeping mutable-copy TN behavior required conservative handling of constructor and non-factory returns as `unknown`.
- Follow-up: extend provenance support for additional stack ops and branch joins to reduce false negatives in more complex bytecode.
