# EMPTY_CATCH

## Summary
- Rule ID: `EMPTY_CATCH`
- Name: Empty catch block
- Problem: Catch blocks that do nothing hide failures and make recovery behavior unclear.

## What This Rule Reports
This rule reports `catch` handlers that contain no meaningful logic.
Trivial stack/local operations and immediate returns are treated as effectively empty.

### Java Example (reported)
```java
class ClassA {
    void methodOne() {
        try {
            runTask();
        } catch (Exception varOne) {
            // no handling
        }
    }

    void runTask() {}
}
```

## What This Rule Does Not Report
- Handlers with meaningful work (log, rethrow, convert, recover)
- Kotlin synthetic enum-when mapping handlers (`$WhenMappings` + `NoSuchFieldError`)

### Java Example (not reported)
```java
class ClassA {
    void methodOne() {
        try {
            runTask();
        } catch (Exception varOne) {
            throw new RuntimeException(varOne);
        }
    }

    void runTask() {}
}
```

## Recommended Fix
Handle the exception explicitly: log with context, restore state, rethrow, or convert to a domain error.

## Message Shape
Findings are reported as `Empty catch block in <class>.<method><descriptor>`.
