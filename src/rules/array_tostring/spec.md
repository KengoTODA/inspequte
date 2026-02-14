## Summary
- Rule ID: `ARRAY_TOSTRING`
- Name: Array toString
- Description: Reports calls to `toString()` on array-typed values, which produce unhelpful JVM identity-hash output like `[Ljava.lang.String;@1a2b3c` instead of readable content.
- Annotation policy: `@Suppress`/`@SuppressWarnings` are not supported; only JSpecify annotations are recognized for any annotation-driven semantics, and non-JSpecify annotations do not change behavior.

## Motivation
Calling `toString()` on a Java array — directly or via string concatenation, `String.valueOf()`, or `PrintStream.println()` — produces the JVM default representation (type descriptor + identity hash), which is almost never the intended output. Developers typically mean `Arrays.toString(array)` or `Arrays.deepToString(array)`. This bug is easy to introduce and hard to spot in code review because the code compiles and runs without error, only producing wrong output at runtime.

## What it detects
- Direct `array.toString()` calls where the receiver is known to be an array type.
- `String.valueOf(array)` calls where the argument is known to be an array type (invoked as `String.valueOf(Object)`).
- `StringBuilder.append(array)` calls where the argument is an array type (invoked as `StringBuilder.append(Object)`), which covers string concatenation compiled by javac.
- `PrintStream.print(array)` and `PrintStream.println(array)` calls where the argument is an array type (invoked with `Object` parameter).

## What it does NOT detect
- Array values whose array-ness cannot be determined from intra-method stack simulation (e.g., arrays returned from unknown methods, loaded from fields without local creation).
- Inter-procedural array tracking (array created in one method, passed to another, then `toString()`-ed).
- `toString()` on non-array objects.
- Correctly using `Arrays.toString()` or `Arrays.deepToString()`.
- Multidimensional arrays passed to `Arrays.toString()` (where `deepToString` would be better) — this is a separate concern.
- Suppression via annotations (`@Suppress`, `@SuppressWarnings`) is not supported.
- Non-JSpecify annotations do not affect rule behavior.

## Examples (TP/TN/Edge)
### True Positive — direct toString()
```java
class ClassA {
    String methodX(int[] varOne) {
        return varOne.toString();
    }
}
```

### True Positive — string concatenation
```java
class ClassB {
    String methodX(String[] varOne) {
        return "values: " + varOne;
    }
}
```

### True Positive — String.valueOf
```java
class ClassC {
    String methodX(Object[] varOne) {
        return String.valueOf(varOne);
    }
}
```

### True Positive — System.out.println
```java
class ClassD {
    void methodX(int[] varOne) {
        System.out.println(varOne);
    }
}
```

### True Negative — Arrays.toString
```java
import java.util.Arrays;
class ClassE {
    String methodX(int[] varOne) {
        return Arrays.toString(varOne);
    }
}
```

### True Negative — non-array toString
```java
class ClassF {
    String methodX(Object varOne) {
        return varOne.toString();
    }
}
```

### Edge Case — multidimensional array
```java
class ClassG {
    String methodX(int[][] varOne) {
        return varOne.toString();
    }
}
```
This should be reported because `toString()` on a multidimensional array still produces the identity-hash representation.

## Output
- Message: `"Array toString() produces '[type@hash]' instead of readable content. Use Arrays.toString() or Arrays.deepToString()."` followed by the qualified method location.
- Location: the call site where `toString()` (or equivalent) is invoked on the array value.

## Performance considerations
- Linear in method bytecode size (single-pass stack simulation).
- Reuses existing stack-machine infrastructure; no additional CFG or inter-procedural analysis required.
- No allocation beyond per-method stack/local state.

## Acceptance criteria
- Reports a finding for direct `toString()` calls on array-typed receivers.
- Reports a finding for `String.valueOf(Object)` calls with array-typed arguments.
- Reports a finding for `StringBuilder.append(Object)` calls with array-typed arguments.
- Reports a finding for `PrintStream.print(Object)` and `PrintStream.println(Object)` calls with array-typed arguments.
- Does not report when `Arrays.toString()` or `Arrays.deepToString()` is used.
- Does not report `toString()` on non-array objects.
- Reports on multidimensional arrays.
- Produces actionable, user-facing messages as defined in Output.
- Applies the annotation policy exactly as stated in Summary.
- Examples above correspond to TP, TN, and Edge behavior.
