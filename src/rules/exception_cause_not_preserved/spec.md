## Summary
- Rule ID: `exception_cause_not_preserved`
- Name: Exception cause not preserved
- Description: Detects catch handlers that throw a new exception without preserving the caught exception as the cause.

## Motivation
Losing the original exception in a catch block removes the root stack trace and makes debugging significantly harder. This is an easy oversight when wrapping errors, but it hides the real failure context and slows incident response.

## What it detects
- A catch handler that throws a newly created exception instance without preserving the caught exception as a cause.
- Cause preservation includes:
  - Passing the caught exception as a constructor argument.
  - Calling `initCause(caught)` on the thrown exception before the `throw`.
  - Calling `addSuppressed(caught)` on the thrown exception before the `throw`.
- Rethrowing the caught exception directly is not reported.

## What it does NOT detect
- Wrapping done via helper methods or builders (inter-procedural inference is out of scope).
- Cause preservation via custom fields or non-standard APIs.
- Throws of exceptions created outside the catch handler.
- Suppression via `@Suppress` / `@SuppressWarnings` (unsupported).
- Annotation-driven semantics from non-JSpecify annotations (unsupported; JSpecify-only when applicable).

## Examples (TP/TN/Edge)
### True positive (reported)
```java
try {
    MethodX();
} catch (Exception varOne) {
    throw new RuntimeException("failed");
}
```

### True negative (not reported)
```java
try {
    MethodX();
} catch (Exception varOne) {
    throw new RuntimeException("failed", varOne);
}
```

### True negative (not reported)
```java
try {
    MethodX();
} catch (Exception varOne) {
    throw varOne;
}
```

### Edge (not reported)
```java
try {
    MethodX();
} catch (Exception varOne) {
    RuntimeException varTwo = new RuntimeException("failed");
    varTwo.initCause(varOne);
    throw varTwo;
}
```

## Output
- SARIF result only.
- Message must be actionable and mention the fix, for example:
  - "Catch handler throws a new exception without preserving the original cause; pass the caught exception as a cause or call initCause/addSuppressed before throwing."

## Performance considerations
- Per catch handler, scanning instructions for throw sites and cause-preserving uses should be linear in handler size.
- No cross-method or cross-class analysis is required.

## Acceptance criteria
- Reports when a catch handler throws a newly constructed exception and the caught exception is not preserved as a cause on the path to the throw.
- Does not report rethrows of the caught exception.
- Does not report when the cause is preserved via constructor, `initCause`, or `addSuppressed` before the throw.
- Emits deterministic, stable findings.
- Explicitly does not support suppression annotations or non-JSpecify annotation semantics.
