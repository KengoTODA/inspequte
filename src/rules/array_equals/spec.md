# ARRAY_EQUALS

## Summary
- Rule ID: `ARRAY_EQUALS`
- Name: Array equals
- Problem: Comparing array values with `==` or `equals()` checks identity, not element equality.

## What This Rule Reports
This rule reports array comparisons when code compares array references directly.

### Java Example (reported)
```java
class ClassA {
    boolean methodOne(String[] varOne, String[] varTwo) {
        return varOne == varTwo;
    }

    boolean methodTwo(String[] varOne, String[] varTwo) {
        return varOne.equals(varTwo);
    }
}
```

## What This Rule Does Not Report
- `java.util.Arrays.equals(varOne, varTwo)`
- Null checks such as `varOne == null`
- Comparisons on non-array references
- Element-level comparisons inside loops

### Java Example (not reported)
```java
import java.util.Arrays;

class ClassA {
    boolean methodOne(String[] varOne, String[] varTwo) {
        return Arrays.equals(varOne, varTwo);
    }
}
```

## Recommended Fix
Use `Arrays.equals(...)` (or `Arrays.deepEquals(...)` for nested arrays) when the intent is value comparison.

## Message Shape
Findings explain that array comparison is done by reference and should use `Arrays.equals`.
