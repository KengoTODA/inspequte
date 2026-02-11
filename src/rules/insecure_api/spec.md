# INSECURE_API

## Summary
- Rule ID: `INSECURE_API`
- Name: Insecure API usage
- Problem: Certain process execution and reflection APIs are high risk and should be avoided or tightly controlled.

## What This Rule Reports
This rule reports direct calls to known insecure APIs, including:
- `java/lang/Runtime.exec(...)`
- `java/lang/ProcessBuilder.<init>(...)`
- `java/lang/Class.forName(...)`

### Java Example (reported)
```java
class ClassA {
    void methodOne() throws Exception {
        Runtime.getRuntime().exec("sh -c whoami");
    }
}
```

## What This Rule Does Not Report
- Safe/regular APIs not on the insecure list
- Classes outside the analysis target scope

### Java Example (not reported)
```java
class ClassA {
    int methodOne(String varOne) {
        return varOne.length();
    }
}
```

## Recommended Fix
Prefer safer alternatives, validate/whitelist inputs, and avoid dynamic command/reflection paths when possible.

## Message Shape
Findings are reported as `Insecure API call to <owner>.<method> in <class>.<method><descriptor>`.
