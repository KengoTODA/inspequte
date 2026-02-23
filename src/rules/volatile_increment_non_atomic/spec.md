## Summary
- Rule ID: `VOLATILE_INCREMENT_NON_ATOMIC`
- Name: Non-atomic update on volatile field
- Description: Detects read-modify-write updates (for example `++`, `--`, `+=`, `-=`) on `volatile` fields that can lose updates when multiple threads execute concurrently.
- Annotation policy: `@Suppress`-style suppression is unsupported. Annotation-driven semantics support JSpecify only; non-JSpecify annotations are unsupported for this rule.

## Motivation
`volatile` ensures visibility of reads and writes, but it does not make compound updates atomic. Patterns like increment and compound assignment can overwrite concurrent updates and produce incorrect counters, totals, and shared state.

## What it detects
- A `volatile` field is read and then written back as part of one compound update in the same method.
- The update semantics depend on the previous field value (for example increment, decrement, or arithmetic compound assignment).
- Each unsafe update site is reported as a potential lost-update concurrency bug.

## What it does NOT detect
- Plain assignments that do not depend on a prior read of the same field value.
- Similar compound updates on non-`volatile` fields.
- Proof that external synchronization makes the update effectively safe.
- Behavior changes from non-JSpecify annotations.
- Any suppression behavior via `@Suppress` or `@SuppressWarnings`.

## Examples (TP/TN/Edge)
### TP (reported)
```java
class ClassA {
    private volatile int varOne = 0;

    void methodOne() {
        varOne++;
    }
}
```

### TN (not reported)
```java
class ClassA {
    private volatile int varOne = 0;

    void methodOne(int varTwo) {
        varOne = varTwo;
    }
}
```

### Edge (reported once per unsafe update site)
```java
class ClassA {
    private volatile int varOne = 0;

    int methodOne() {
        int tmpValue = varOne++;
        return tmpValue;
    }
}
```

## Output
- Report one finding per non-atomic volatile update site.
- Message must be intuitive and actionable, for example:
  `Non-atomic update on volatile field '<field>' in <class>.<method>; replace with an atomic type or synchronize the update.`
- Primary fix guidance: use atomic primitives/classes (for example `AtomicInteger`) or protect the full read-modify-write with synchronization.

## Performance considerations
- Analysis should scale with scanned bytecode size and remain stable for large methods.
- Matching should avoid expensive global reasoning and keep per-method work bounded.
- Finding order must be deterministic across repeated runs on identical input.

## Acceptance criteria
- Reports increment/decrement and arithmetic compound updates on `volatile` fields when they use read-modify-write semantics.
- Does not report plain assignments to `volatile` fields.
- Does not report equivalent compound updates on non-`volatile` fields.
- Includes TP, TN, and edge coverage where post-increment expression usage is still reported once at the update site.
- Keeps output deterministic and preserves the annotation policy (`@Suppress` unsupported; JSpecify-only semantics for annotation-driven behavior).
