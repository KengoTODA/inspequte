# MUTATE_UNMODIFIABLE_COLLECTION

## Summary
- Rule ID: `MUTATE_UNMODIFIABLE_COLLECTION`
- Name: Mutation on unmodifiable collection
- Description: Reports mutation calls on collections that are created by known JDK unmodifiable factories or wrappers in the same method.
- Annotation policy: `@Suppress`-style suppression is unsupported. Annotation-driven semantics support JSpecify only; non-JSpecify annotations are unsupported for this rule.

## Motivation
Unmodifiable collections are convenient for safe sharing, but mutating them throws `UnsupportedOperationException` at runtime. These bugs are easy to miss during code review because the code compiles and fails only when the mutation path executes.

## What it detects
- A collection value is produced by known unmodifiable JDK APIs in the same method, including:
  - `List.of(...)`, `List.copyOf(...)`
  - `Set.of(...)`, `Set.copyOf(...)`
  - `Map.of(...)`, `Map.ofEntries(...)`, `Map.copyOf(...)`
  - `Collections.unmodifiable*`, `Collections.empty*`, `Collections.singleton*`
- A mutator call is made on that value (for example `add`, `remove`, `clear`, `put`, `replace`, `compute`).
- One finding per mutation call site.

## What it does NOT detect
- Cases where unmodifiable provenance is created in a different method and not visible in the current method.
- Exact runtime receiver identity proof under complex aliasing.
- Suppression via `@Suppress` / `@SuppressWarnings`.
- Semantics driven by non-JSpecify annotations.

## Examples (TP/TN/Edge)
### TP (reported)
```java
import java.util.List;

class ClassA {
    void methodX() {
        List<String> varOne = List.of("tmpValue");
        varOne.add("varTwo");
    }
}
```

### TN (not reported)
```java
import java.util.ArrayList;
import java.util.List;

class ClassB {
    void methodY() {
        List<String> varOne = new ArrayList<>(List.of("tmpValue"));
        varOne.add("varTwo");
    }
}
```

### Edge (only unmodifiable mutation reported)
```java
import java.util.ArrayList;
import java.util.List;
import java.util.Set;

class ClassC {
    void methodZ() {
        Set<String> varOne = Set.of("tmpValue");
        varOne.remove("tmpValue");

        List<String> varTwo = new ArrayList<>();
        varTwo.add("varThree");
    }
}
```

## Output
- Message should be actionable and include method context, for example:
  `Unmodifiable collection is mutated in <class>.<method><descriptor>; create a mutable copy before calling mutator methods.`
- Location should point to the mutator call site line when line metadata is available.

## Performance considerations
- Analysis should be linear in method bytecode size.
- No whole-program dataflow is required.
- Finding count and order must be deterministic across repeated runs.

## Acceptance criteria
- Reports mutator calls on values produced by in-scope unmodifiable JDK APIs within the same method.
- Does not report mutations on clearly mutable collections (for example new mutable copy patterns).
- Covers TP, TN, and edge scenarios in tests.
- Produces deterministic finding count and ordering.
- Keeps `@Suppress`-style suppression unsupported and does not add non-JSpecify annotation semantics.
