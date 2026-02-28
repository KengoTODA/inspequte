# CODEX_LOCAL_COMPLEXITY_GUARD

## Summary

- Rule ID: `codex_local_complexity_guard`
- Name: Local cyclomatic complexity guard
- Description: Reports concrete methods whose local cyclomatic complexity is above a strict fixed threshold (`> 10`) and emits one deterministic finding per method.
- User guidance: The finding message includes the measured complexity and threshold, and tells users to split or simplify method control flow.
- Annotation policy: `@Suppress`-style suppression is unsupported. Annotation-driven semantics are JSpecify-only; non-JSpecify annotations are unsupported and do not change rule behavior.

## Motivation

Methods with too many independent control-flow paths are harder to understand, test, and safely modify. A strict local complexity guard catches risky methods early and gives maintainers a clear signal to refactor before complexity spreads.

This rule is intentionally narrow and deterministic so teams can trust it in CI. It focuses on method-local complexity only and avoids broad heuristics that increase noise.

## What it detects

The rule evaluates concrete method bodies and computes method-local cyclomatic complexity.

A method is reported when all of the following are true:

- The method has executable bytecode (for example, not abstract and not native).
- The method is not compiler-generated bridge or synthetic-only noise.
- The method-local cyclomatic complexity is greater than `10`.

Complexity is defined as:

- Base complexity `1`.
- Plus one for each local control-flow decision point, including conditional branches, each non-default switch branch, and each catch handler.

Reporting behavior:

- Emit at most one finding per method identity (`class`, `method`, `descriptor`).
- Findings are deterministic for identical inputs.

## What it does NOT detect

- Methods with complexity less than or equal to `10` (including the exact boundary value `10`).
- Methods without executable bytecode (for example, abstract or native declarations).
- Inter-procedural complexity (callee complexity does not affect caller findings).
- Cognitive complexity or style-weighted scoring models.
- Suppression via `@Suppress` or `@SuppressWarnings` annotations.
- Annotation-driven behavior from non-JSpecify annotation sets.

## Examples (TP/TN/Edge)

### True Positive (reported): complexity above threshold

```java
class ClassA {
    void methodX(int varOne, int varTwo, int varThree) {
        if (varOne > 0) { }
        if (varTwo > 0) { }
        if (varThree > 0) { }

        for (int i = 0; i < 3; i++) {
            if ((i & 1) == 0) { }
        }

        while (varOne < varTwo) {
            break;
        }

        switch (varThree) {
            case 1: break;
            case 2: break;
            case 3: break;
            default: break;
        }

        try {
            methodY();
        } catch (RuntimeException tmpValue) {
        }
    }

    void methodY() {
    }
}
```

Reported: local complexity is above `10`.

### True Negative (not reported): below threshold

```java
class ClassB {
    int methodX(int varOne, int varTwo) {
        if (varOne > varTwo) {
            return varOne;
        }
        return varTwo;
    }
}
```

Not reported: local complexity is not above `10`.

### Edge (not reported): exact boundary

```java
class ClassC {
    void methodX(int varOne) {
        if (varOne > 0) { }
        if (varOne > 1) { }
        if (varOne > 2) { }
        if (varOne > 3) { }
        if (varOne > 4) { }
        if (varOne > 5) { }
        if (varOne > 6) { }
        if (varOne > 7) { }
        if (varOne > 8) { }
    }
}
```

Not reported: complexity equals `10` (threshold is strict `> 10`).

### Edge (reported): catch handlers increase complexity

```java
class ClassD {
    void methodX() {
        try {
            methodY();
        } catch (IllegalArgumentException varOne) {
        } catch (IllegalStateException varTwo) {
        }

        if (tmpValue()) { }
        if (tmpValue()) { }
        if (tmpValue()) { }
        if (tmpValue()) { }
        if (tmpValue()) { }
        if (tmpValue()) { }
        if (tmpValue()) { }
        if (tmpValue()) { }
        if (tmpValue()) { }
    }

    boolean tmpValue() {
        return true;
    }

    void methodY() {
    }
}
```

Reported: catch handlers count as decision points and push the method above the strict threshold.

## Output

Each finding must be deterministic and method-scoped.

Message shape:

```text
Method complexity <measured> exceeds local threshold <threshold> in <class>.<method><descriptor>; simplify control flow or split this method.
```

Output requirements:

- Rule ID in SARIF: `codex_local_complexity_guard`.
- One finding per violating method identity (`class`, `method`, `descriptor`).
- Stable finding order for identical input, sorted by (`class`, `method`, `descriptor`).

## Performance considerations

- Analysis is local to each method body and scales linearly with method instruction count.
- No inter-procedural graph traversal or cross-method dependency is required.
- Memory use is bounded to per-method metrics plus collected findings.

## Acceptance criteria

- Methods with local cyclomatic complexity `> 10` are reported.
- Methods with complexity `<= 10` are not reported.
- Decision counting includes local conditional branches, non-default switch branches, and catch handlers.
- Methods without executable bodies and synthetic/bridge-only noise methods are not reported.
- Exactly one finding is emitted per violating method identity.
- Finding messages include measured complexity, threshold, and actionable remediation guidance.
- `@Suppress`-style suppression is unsupported and does not alter behavior.
- Non-JSpecify annotations do not alter behavior.
- Re-running on identical input produces identical findings in identical order.