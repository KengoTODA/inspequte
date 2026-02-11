# NULLNESS

## Summary
- Rule ID: `NULLNESS`
- Name: Nullness checks
- Problem: Nullable values used as non-null, unsafe null returns, and override nullness contract violations can cause runtime errors.

## Scope
This rule evaluates nullness behavior guided by JSpecify-style nullness information and inferred flow.
It covers:
- Method override compatibility
- Intra-method nullness flow
- Type-use nullness for generic return/parameter positions

## What This Rule Reports
### 1. Unsafe override contracts
- Return becomes more nullable than the overridden method
- Parameter becomes less nullable than the overridden method
- Nested type-use nullness conflicts in generic signatures

### 2. Nullness flow misuse
- Possible null receiver dereference
- Returning `null` from non-null return contract

### Java Example (reported)
```java
import org.jspecify.annotations.NonNull;
import org.jspecify.annotations.Nullable;

class ClassA {
    @NonNull String methodOne() {
        return null;
    }

    void methodTwo(@Nullable String varOne) {
        varOne.length();
    }
}
```

## What This Rule Does Not Report
- Flows proven non-null by checks before use
- Cases where nullness is unresolved and treated conservatively
- Override combinations allowed by nullness variance rules

### Java Example (not reported)
```java
import org.jspecify.annotations.Nullable;

class ClassA {
    void methodOne(@Nullable String varOne) {
        if (varOne != null) {
            varOne.length();
        }
    }
}
```

## Generic Type-Use Flow
The rule propagates type-use nullness through generic call returns when receiver type arguments make nullness resolvable.
For example, a call on `ClassB<@Nullable String>` that returns `T` is treated as possibly nullable.

## Recommended Fix
- Align override signatures with base nullness contracts
- Add null checks before dereference
- Avoid returning null from non-null methods
- Adjust type-use annotations to match intended API contract

## Message Shape
Findings are emitted in actionable forms such as:
- `Nullness override: ...`
- `Nullness flow: possible null receiver ...`
- `Nullness flow: returning null from @NonNull method ...`
