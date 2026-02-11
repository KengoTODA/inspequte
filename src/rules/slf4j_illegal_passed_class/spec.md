# SLF4J_ILLEGAL_PASSED_CLASS

## Summary
- Rule ID: `SLF4J_ILLEGAL_PASSED_CLASS`
- Name: SLF4J illegal passed class
- Problem: `LoggerFactory.getLogger(...)` should receive the declaring class to keep logger category accurate.

## What This Rule Reports
This rule reports class-literal logger initialization that passes a different class than the current class.

### Java Example (reported)
```java
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

class ClassA {
    private static final Logger LOG = LoggerFactory.getLogger(String.class);
}
```

## What This Rule Does Not Report
- `LoggerFactory.getLogger(ClassA.class)` in `ClassA`
- Kotlin reified-extension helper patterns where class literal is compiler-driven

### Java Example (not reported)
```java
class ClassA {
    private static final Logger LOG = LoggerFactory.getLogger(ClassA.class);
}
```

## Recommended Fix
Pass the current class literal (`ClassA.class`) or use a safe wrapper that preserves the declaring type.

## Message Shape
Findings explain that `LoggerFactory.getLogger` should use the caller class and show both expected and actual class.
