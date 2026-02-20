# INTEGER_GETINTEGER_CALL

## Summary
- Rule ID: `INTEGER_GETINTEGER_CALL`
- Name: Integer.getInteger call
- Problem: `Integer.getInteger(...)` reads system properties and is often mistakenly used for string-to-int parsing.

## What This Rule Reports
This rule reports direct calls to:
- `java/lang/Integer.getInteger(Ljava/lang/String;)Ljava/lang/Integer;`
- `java/lang/Integer.getInteger(Ljava/lang/String;I)Ljava/lang/Integer;`
- `java/lang/Integer.getInteger(Ljava/lang/String;Ljava/lang/Integer;)Ljava/lang/Integer;`

### Examples (reported)
```java
package com.example;
public class ClassA {
    public Integer methodX(String varOne) {
        return Integer.getInteger(varOne);
    }
}
```

```java
package com.example;
public class ClassB {
    public Integer methodY(String varOne) {
        return Integer.getInteger(varOne, 10);
    }
}
```

## What This Rule Does Not Report
- Numeric parsing APIs such as `Integer.parseInt(...)` and `Integer.valueOf(...)`.
- Calls that appear only in classpath/dependency classes outside the analysis target.

### Examples (not reported)
```java
package com.example;
public class ClassC {
    public int methodZ(String varOne) {
        return Integer.parseInt(varOne);
    }
}
```

## Recommended Fix
Use `Integer.parseInt(...)` or `Integer.valueOf(...)` when converting numeric strings.

## Message Shape
Findings are reported as `Avoid Integer.getInteger() in <class>.<method><descriptor>; use Integer.parseInt()/valueOf() for numeric parsing or keep it only for system property reads.`
