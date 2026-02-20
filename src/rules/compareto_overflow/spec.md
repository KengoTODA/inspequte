# COMPARETO_OVERFLOW

## Summary
- Rule ID: `COMPARETO_OVERFLOW`
- Name: compareTo integer subtraction overflow
- Description: Detects `compareTo` implementations that use integer subtraction to compute the return value, which can produce incorrect ordering for extreme integer values due to arithmetic overflow.
- Annotation policy: `@Suppress`-style suppression is unsupported. Annotation-driven semantics support JSpecify only; non-JSpecify annotations are unsupported for this rule.

## Motivation
A common Java pitfall is implementing `compareTo` with integer subtraction:

```java
public int compareTo(MyObj other) {
    return this.value - other.value;
}
```

This appears correct but is broken for extreme values. When `this.value = Integer.MAX_VALUE` and `other.value = -1`, the subtraction `Integer.MAX_VALUE - (-1)` overflows to `Integer.MIN_VALUE`, which is negative. The comparator incorrectly reports that `MAX_VALUE` is less than `-1`, violating the `compareTo` contract and corrupting sort order in collections such as `TreeSet`, `TreeMap`, and `PriorityQueue`.

This bug is subtle and easy to miss during code review because it only manifests for values near `Integer.MAX_VALUE` or `Integer.MIN_VALUE`. The safe replacement is `Integer.compare(this.value, other.value)`.

## What it detects
- A method named `compareTo` with an `int` return type (descriptor ending in `)I`) that contains the `isub` (integer subtract, JVM opcode 0x64) bytecode instruction.
- The rule additionally requires that the method does **not** call `java/lang/Integer.compare` or `java/lang/Long.compare`, which are the overflow-safe comparison utilities.

## What it does NOT detect
- `compareTo` methods that use `Integer.compare`, `Long.compare`, or other overflow-safe comparison utilities. These are correct and are excluded.
- Long arithmetic subtraction (`lsub`) narrowed to `int`.
- Overflow in comparator lambda bodies defined in separate synthetic methods.
- Methods outside analysis target classes.
- Findings based on non-JSpecify annotation semantics.
- Any suppression behavior via `@Suppress` or `@SuppressWarnings`.

## Examples (TP/TN/Edge)

### TP: direct integer subtraction return (reported)
```java
package com.example;
public class ClassA implements Comparable<ClassA> {
    int varOne;
    @Override
    public int compareTo(ClassA other) {
        return this.varOne - other.varOne;
    }
}
```

### TP: multi-field comparison using subtraction (reported)
```java
package com.example;
public class ClassB implements Comparable<ClassB> {
    int varOne;
    int varTwo;
    @Override
    public int compareTo(ClassB other) {
        int diff = this.varOne - other.varOne;
        if (diff != 0) return diff;
        return this.varTwo - other.varTwo;
    }
}
```

### TN: Integer.compare used (not reported)
```java
package com.example;
public class ClassC implements Comparable<ClassC> {
    int varOne;
    @Override
    public int compareTo(ClassC other) {
        return Integer.compare(this.varOne, other.varOne);
    }
}
```

### TN: String compareTo delegation, no integer subtraction (not reported)
```java
package com.example;
public class ClassD implements Comparable<ClassD> {
    String varOne;
    @Override
    public int compareTo(ClassD other) {
        return this.varOne.compareTo(other.varOne);
    }
}
```

### Edge: subtraction for intermediate value but Integer.compare for return (not reported)
```java
package com.example;
public class ClassE implements Comparable<ClassE> {
    int varOne;
    int varTwo;
    @Override
    public int compareTo(ClassE other) {
        int adjustment = this.varTwo - 1;  // isub, but excluded because Integer.compare is called
        return Integer.compare(this.varOne + adjustment, other.varOne + adjustment);
    }
}
```

### Edge: classpath-only class (not reported)
A class compiled as a classpath dependency and not included in the analysis target is not reported, even if it contains a subtraction-based `compareTo`.

## Output
- Report one finding per `compareTo` method that contains integer subtraction without an overflow-safe comparison call.
- Message must be actionable and include the method context:
  `Avoid integer subtraction in compareTo in <class>.<method><descriptor>; use Integer.compare() to prevent overflow.`
- Location is reported at the method level using method logical location and, where available, the source line of the first `isub` instruction.

## Performance considerations
- Analysis is bounded by the number of basic block instructions per `compareTo` method.
- Only methods named `compareTo` with int return descriptor are scanned; all other methods are skipped.
- No inter-method or inter-class analysis is performed.
- Traversal order over blocks and instructions must be deterministic (use the natural iteration order of `method.cfg.blocks`).

## Acceptance criteria
1. Reports when a `compareTo` method (returning `int`) contains `isub` and does not call `Integer.compare` or `Long.compare`.
2. Does not report when `Integer.compare` or `Long.compare` is present in the same method.
3. Does not report methods not named `compareTo`.
4. Does not report `compareTo` methods that return non-int types.
5. Does not report classpath-only classes.
6. Covers TP, TN, and edge cases in tests.
7. Produces deterministic finding order and count across repeated runs.
8. Keeps `@Suppress`-style suppression unsupported and does not add non-JSpecify annotation semantics.
