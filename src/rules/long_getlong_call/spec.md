---
type: 'Static Analysis Rule'
title: 'Long.getLong call'
description: 'Long.getLong reads system properties, not numeric input strings'
tags: ['jvm', 'static-analysis']
status: 'stable'
rule_id: 'LONG_GETLONG_CALL'
---

# LONG_GETLONG_CALL

## Summary
- Rule ID: `LONG_GETLONG_CALL`
- Name: Long.getLong call
- Problem: `Long.getLong(...)` reads system properties and is often mistakenly used for numeric parsing.

## What This Rule Reports
This rule reports direct calls to:
- `java/lang/Long.getLong(Ljava/lang/String;)Ljava/lang/Long;`
- `java/lang/Long.getLong(Ljava/lang/String;J)Ljava/lang/Long;`
- `java/lang/Long.getLong(Ljava/lang/String;Ljava/lang/Long;)Ljava/lang/Long;`

### Examples (reported)
```java
package com.example;
public class ClassA {
    public Long methodX(String varOne) {
        return Long.getLong(varOne);
    }
}
```

```java
package com.example;
public class ClassB {
    public Long methodY(String varOne) {
        return Long.getLong(varOne, 10L);
    }
}
```

## What This Rule Does Not Report
- Numeric parsing APIs such as `Long.parseLong(...)` and `Long.valueOf(...)`.
- Calls that appear only in classpath/dependency classes outside the analysis target.

### Examples (not reported)
```java
package com.example;
public class ClassC {
    public long methodZ(String varOne) {
        return Long.parseLong(varOne);
    }
}
```

## Recommended Fix
Use `Long.parseLong(...)` or `Long.valueOf(...)` when converting numeric strings.

## Message Shape
Findings are reported as `Avoid Long.getLong() in <class>.<method><descriptor>; use Long.parseLong()/valueOf() for numeric parsing or keep it only for system property reads.`
