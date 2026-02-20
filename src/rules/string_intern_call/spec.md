# STRING_INTERN_CALL

## Summary
- Rule ID: `STRING_INTERN_CALL`
- Name: String intern call
- Problem: `String.intern()` can increase global string-pool pressure and hurt memory/performance.

## What This Rule Reports
This rule reports direct calls to:
- `java/lang/String.intern()Ljava/lang/String;`

### Examples (reported)
```java
package com.example;
public class ClassA {
    public String methodX(String varOne) {
        return varOne.intern();
    }
}
```

## What This Rule Does Not Report
- Other string APIs such as `toString()`.
- Calls that appear only in classpath/dependency classes outside the analysis target.

### Examples (not reported)
```java
package com.example;
public class ClassB {
    public String methodY(String varOne) {
        return varOne.toString();
    }
}
```

## Recommended Fix
Avoid interning dynamic strings. Prefer bounded caches or targeted canonicalization where needed.

## Message Shape
Findings are reported as `Avoid String.intern() in <class>.<method><descriptor>; use bounded caching or explicit canonicalization instead.`
