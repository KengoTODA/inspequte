# URL_OPENSTREAM_CALL

## Summary
- Rule ID: `URL_OPENSTREAM_CALL`
- Name: URL.openStream call
- Problem: `URL.openStream()` often bypasses explicit connection timeout configuration and can cause blocking network behavior.

## What This Rule Reports
This rule reports direct calls to:
- `java/net/URL.openStream()Ljava/io/InputStream;`

### Examples (reported)
```java
package com.example;
import java.io.InputStream;
import java.net.URL;
public class ClassA {
    public InputStream methodX(URL varOne) throws Exception {
        return varOne.openStream();
    }
}
```

## What This Rule Does Not Report
- `URL.openConnection()` calls.
- `Class.getResource(...).openStream()` calls.
- `ClassLoader.getResource(...).openStream()` calls.
- Calls that appear only in classpath/dependency classes outside the analysis target.

### Examples (not reported)
```java
package com.example;
import java.net.URL;
import java.net.URLConnection;
public class ClassB {
    public URLConnection methodY(URL varOne) throws Exception {
        return varOne.openConnection();
    }
}
```

```java
package com.example;
import java.io.InputStream;
public class ClassC {
    public InputStream methodZ() throws Exception {
        return ClassC.class.getResource("/tmp.txt").openStream();
    }
}
```

## Recommended Fix
Use `openConnection()` with explicit connect/read timeout configuration and explicit resource management.

## Message Shape
Findings are reported as `Avoid URL.openStream() in <class>.<method><descriptor>; use openConnection() with explicit timeouts and structured resource handling.`
