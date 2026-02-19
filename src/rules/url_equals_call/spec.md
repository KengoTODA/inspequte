# URL_EQUALS_CALL

## Summary
- Rule ID: `URL_EQUALS_CALL`
- Name: URL equals call
- Problem: `URL.equals(Object)` can trigger host resolution and may not match intended structural equality.

## What This Rule Reports
This rule reports direct calls to:
- `java/net/URL.equals(Ljava/lang/Object;)Z`

### Examples (reported)
```java
package com.example;
import java.net.URL;
public class ClassA {
    public boolean methodX(URL varOne, URL varTwo) {
        return varOne.equals(varTwo);
    }
}
```

## What This Rule Does Not Report
- Comparisons on normalized `URI` values.
- Calls that appear only in classpath/dependency classes outside the analysis target.

### Examples (not reported)
```java
package com.example;
import java.net.URL;
import java.net.URISyntaxException;
public class ClassB {
    public boolean methodY(URL varOne, URL varTwo) throws URISyntaxException {
        return varOne.toURI().equals(varTwo.toURI());
    }
}
```

## Recommended Fix
Prefer explicit structural comparisons, such as `toURI().equals(...)` on normalized values or component-wise checks.

## Message Shape
Findings are reported as `Avoid URL.equals() in <class>.<method><descriptor>; compare normalized URI values or explicit URL components instead.`
