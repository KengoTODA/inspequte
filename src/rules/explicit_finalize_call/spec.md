# EXPLICIT_FINALIZE_CALL

## Summary
- Rule ID: `EXPLICIT_FINALIZE_CALL`
- Name: Explicit finalize call
- Description: Detects direct virtual calls to `finalize()` on object instances, which bypass GC lifecycle management and indicate broken resource cleanup.
- Annotation policy: `@Suppress`-style suppression is unsupported. Annotation-driven semantics support JSpecify only; this rule has no annotation-driven semantics.

## Motivation
Java's `Object.finalize()` method was designed to be called by the GC during object collection, not by application code. Explicitly calling `obj.finalize()` is almost always a mistake:

- It does not trigger GC collection of `obj` or its resources.
- It can run the finalizer twice (once explicitly, once by GC), which is unsafe for resources like file handles or sockets.
- `Object.finalize()` is deprecated since Java 9 and the entire finalizer mechanism is being removed; explicit calls make migration harder.
- The correct resource management approach is `AutoCloseable` with try-with-resources, or `java.lang.ref.Cleaner`.

This mistake is hard to notice in code review because `obj.finalize()` is syntactically valid and compiles without warnings in older Java versions.

## What it detects
- Any bytecode-level `invokevirtual` call where method name is `finalize` and descriptor is `()V`, in any analysis target class.
- This includes `this.finalize()`, `field.finalize()`, and `localVar.finalize()`.

## What it does NOT detect
- `super.finalize()` calls inside `finalize()` overrides — those use `invokespecial` and are a legitimate pattern for finalize override chains.
- Overriding the `finalize()` method itself — only call sites are flagged.
- `invokespecial` calls to `finalize()` outside a `finalize` override (a rare edge case not worth the complexity to detect).
- Classes that are classpath-only (not analysis targets).
- Any suppression behavior via `@Suppress` or `@SuppressWarnings`.

## Examples (TP/TN/Edge)

### TP: explicit call on a local variable (reported)
```java
package com.example;
public class ClassA {
    public void methodOne() throws Throwable {
        Object varOne = new Object();
        varOne.finalize();
    }
}
```
Finding reported: explicit finalize call in `ClassA.methodOne`.

### TP: explicit call on `this` (reported)
```java
package com.example;
public class ClassB {
    public void methodTwo() throws Throwable {
        this.finalize();
    }
}
```
Finding reported: explicit finalize call in `ClassB.methodTwo`.

### TN: `super.finalize()` inside a `finalize` override (not reported)
```java
package com.example;
public class ClassC {
    @Override
    protected void finalize() throws Throwable {
        super.finalize();
    }
}
```
No finding: `super.finalize()` uses `invokespecial`, not `invokevirtual`.

### TN: `finalize()` override with no super call (not reported)
```java
package com.example;
public class ClassD {
    @Override
    protected void finalize() throws Throwable {
        // cleanup
    }
}
```
No finding: the method declaration itself is not flagged, only call sites.

### TN: unrelated method calls (not reported)
```java
package com.example;
public class ClassE {
    public void methodThree() {
        System.gc();
    }
}
```
No finding: no `finalize()` call present.

### Edge: classpath-only class with explicit finalize call (not reported)
A class compiled as a classpath dependency and not included in the analysis target is not reported, even if it contains an explicit `finalize()` call.

## Output
- Report one finding per `invokevirtual finalize ()V` call site.
- Message must be actionable:
  `Explicit call to finalize() in <class>.<method><descriptor>; use AutoCloseable with try-with-resources or java.lang.ref.Cleaner for deterministic resource cleanup.`
- Location is reported at the method level using method logical location and, where available, the source line of the call instruction.

## Performance considerations
- Analysis is bounded by the total number of call sites across all analysis target methods.
- No inter-method or inter-class analysis is performed.
- No CFG traversal is required; a simple scan of `method.calls` suffices.
- Traversal order must be deterministic: process classes in iteration order, methods in iteration order, calls in offset order (as provided by the IR).

## Acceptance criteria
1. Reports when any analysis target method contains an `invokevirtual finalize ()V` call.
2. Does not report `invokespecial finalize ()V` calls (i.e., `super.finalize()` patterns).
3. Does not report the `finalize()` method declaration itself.
4. Does not report classpath-only classes.
5. Covers TP, TN, and edge cases in tests.
6. Produces deterministic finding order and count across repeated runs.
7. Keeps `@Suppress`-style suppression unsupported and does not add non-JSpecify annotation semantics.
