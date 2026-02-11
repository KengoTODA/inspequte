# INEFFECTIVE_EQUALS_HASHCODE

## Summary
- Rule ID: `INEFFECTIVE_EQUALS_HASHCODE`
- Name: Ineffective equals/hashCode
- Problem: Overriding only `equals` or only `hashCode` breaks collection behavior contracts.

## What This Rule Reports
This rule reports classes that define one of these methods without the other:
- `boolean equals(Object)`
- `int hashCode()`

### Java Example (reported)
```java
class ClassA {
    @Override
    public boolean equals(Object varOne) {
        return true;
    }
}
```

## What This Rule Does Not Report
- Classes that override both methods consistently
- Methods named `equals`/`hashCode` with non-contract signatures

### Java Example (not reported)
```java
class ClassA {
    @Override
    public boolean equals(Object varOne) {
        return true;
    }

    @Override
    public int hashCode() {
        return 1;
    }
}
```

## Recommended Fix
If one method is overridden, override the pair with compatible semantics.

## Message Shape
Findings state either:
- class overrides `equals(Object)` but not `hashCode()`, or
- class overrides `hashCode()` but not `equals(Object)`.
