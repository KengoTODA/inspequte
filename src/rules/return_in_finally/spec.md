## Summary
- Rule ID: `return_in_finally`
- Name: Return in finally
- Description: Reports `return` statements inside `finally` blocks because they can override exceptions or earlier returns and hide failures.
- Annotation policy: `@Suppress`/`@SuppressWarnings` are not supported; only JSpecify annotations are recognized for any annotation-driven semantics, and non-JSpecify annotations do not change behavior.

## Motivation
Returning from a `finally` block changes control flow in a way that can discard an exception or prior return value. This makes failures disappear and results unpredictable, especially in large try/catch/finally blocks. The rule aims to prevent hidden failures and make cleanup code safer.

## What it detects
- Any explicit `return` statement located inside a `finally` block.
- `return` in `finally` that would override:
  - An exception thrown in the `try` or `catch` block.
  - A prior return value from the `try` or `catch` block.
- Both value returns (e.g., `return 1;`) and `return;` in `void` methods.

## What it does NOT detect
- `finally` blocks that do not contain a `return`.
- Returns that occur after the `try`/`finally` has completed (for example, returning a local assigned in `finally`).
- Suppression via annotations (`@Suppress`, `@SuppressWarnings`) is not supported.
- Non-JSpecify annotations do not affect rule behavior.

## Examples (TP/TN/Edge)
### True Positive
```java
class ClassA {
    int MethodX() {
        try {
            throw new RuntimeException("fail");
        } finally {
            return 1;
        }
    }
}
```

### True Negative
```java
class ClassB {
    int MethodY() {
        int varOne;
        try {
            varOne = 1;
        } finally {
            varOne = 2;
        }
        return varOne;
    }
}
```

### Edge Case
```java
class ClassC {
    int MethodZ() {
        try {
            return 1;
        } finally {
            return 2;
        }
    }
}
```

## Output
- Message: "Return in finally overrides exceptions or prior returns. Move the return outside the finally block or return after the try/finally."
- Location: the `return` statement within the `finally` block.

## Performance considerations
- Expected to be linear in method size.
- Should reuse existing control-flow and exception-table analysis where available.

## Acceptance criteria
- Reports a finding for any `return` inside a `finally` block that would override a thrown exception or earlier return.
- Does not report when `finally` has no `return` and the method returns after the `try`/`finally` completes.
- Produces actionable, user-facing messages as defined in Output.
- Applies the annotation policy exactly as stated in Summary.
- Examples above correspond to TP, TN, and Edge behavior.
