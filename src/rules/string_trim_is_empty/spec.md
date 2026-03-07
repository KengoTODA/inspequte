# STRING_TRIM_IS_EMPTY

## Summary
- Rule ID: `STRING_TRIM_IS_EMPTY`
- Name: String trim followed by isEmpty
- Description: Reports direct `String.trim().isEmpty()` chains because blank-check intent is ambiguous and `String.isBlank()` (Java 11+) is clearer.
- Annotation policy: `@Suppress`/`@SuppressWarnings` are not supported; only JSpecify annotations are recognized for annotation-driven semantics, and non-JSpecify annotations do not change behavior.

## Motivation
`trim().isEmpty()` is often used to express blank-string checks, but its behavior can be misunderstood for Unicode whitespace and the intent is less explicit. `isBlank()` communicates intent directly and aligns better with modern Java APIs.

## What it detects
- Direct call chains where `java.lang.String.trim()` is immediately followed by `java.lang.String.isEmpty()` on the returned string value.
- One finding per matching chain.

## What it does NOT detect
- Standalone `String.isEmpty()` calls.
- Standalone `String.trim()` calls.
- Non-adjacent or alternative forms such as `trim().length() == 0`.
- Suppression via annotations (`@Suppress`, `@SuppressWarnings`).
- Behavior changes based on non-JSpecify annotations.

## Examples (TP/TN/Edge)
### TP (reported)
```java
class ClassA {
    boolean methodX(String varOne) {
        return varOne.trim().isEmpty();
    }
}
```

### TN (not reported)
```java
class ClassB {
    boolean methodY(String varOne) {
        return varOne.isBlank();
    }
}
```

### Edge (only direct chain is reported)
```java
class ClassC {
    boolean methodZ(String varOne, String varTwo) {
        boolean varThree = varOne.trim().isEmpty();
        boolean varFour = varTwo.isEmpty();
        return varThree || varFour;
    }
}
```

## Output
- Message should be actionable and include method context, for example:
  `String blank check in <class>.<method><descriptor> uses trim().isEmpty(); replace with isBlank() (Java 11+) for clearer Unicode-aware whitespace handling.`
- Location should point to the call site line when line metadata is available.

## Performance considerations
- Analysis should be linear in the number of discovered call sites.
- No whole-program dataflow is required.
- Result order should be deterministic across runs.

## Acceptance criteria
- Reports each direct `String.trim().isEmpty()` chain.
- Does not report `String.isBlank()` calls.
- Does not report standalone `String.isEmpty()` calls without a preceding direct `trim()` chain.
- Covers TP, TN, and edge scenarios in tests.
- Produces deterministic finding count and ordering.
- Keeps `@Suppress`-style suppression unsupported and does not add non-JSpecify annotation semantics.
