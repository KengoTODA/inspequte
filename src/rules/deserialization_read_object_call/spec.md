---
type: 'Static Analysis Rule'
title: 'ObjectInputStream deserialization call'
description: 'readObject/readUnshared are high-risk Java deserialization entry points'
tags: ['jvm', 'static-analysis']
status: 'stable'
rule_id: 'DESERIALIZATION_READ_OBJECT_CALL'
---

# DESERIALIZATION_READ_OBJECT_CALL

## Summary
- Rule ID: `DESERIALIZATION_READ_OBJECT_CALL`
- Name: ObjectInputStream deserialization call
- Problem: direct Java deserialization entry points (`readObject`/`readUnshared`) are high-risk when data origin is not strictly trusted.

## What This Rule Reports
This rule reports direct calls to:
- `java/io/ObjectInputStream.readObject()Ljava/lang/Object;`
- `java/io/ObjectInputStream.readUnshared()Ljava/lang/Object;`

### Examples (reported)
```java
package com.example;
import java.io.ObjectInputStream;
public class ClassA {
    public Object methodX(ObjectInputStream varOne) throws Exception {
        return varOne.readObject();
    }
}
```

```java
package com.example;
import java.io.ObjectInputStream;
public class ClassB {
    public Object methodY(ObjectInputStream varOne) throws Exception {
        return varOne.readUnshared();
    }
}
```

## What This Rule Does Not Report
- Non-deserialization stream APIs (for example primitive `readInt`).
- Calls that appear only in classpath/dependency classes outside the analysis target.

### Examples (not reported)
```java
package com.example;
import java.io.DataInputStream;
public class ClassC {
    public int methodZ(DataInputStream varOne) throws Exception {
        return varOne.readInt();
    }
}
```

## Recommended Fix
Prefer safer serialization formats, or enforce strict deserialization controls and input filtering when Java serialization cannot be avoided.

## Message Shape
Findings are reported as `Avoid ObjectInputStream deserialization call in <class>.<method><descriptor>; use safer formats or strict deserialization controls.`
