# UNMANAGED_AUTOCLOSEABLE

## Summary

- Rule ID: `UNMANAGED_AUTOCLOSEABLE`
- Name: AutoCloseable not closed
- Description: Detects locally created `AutoCloseable` instances that can reach a method exit without `close()` being
  called on every reachable path in the same method.
- Annotation policy: `@Suppress`-style suppression is unsupported. Annotation-driven semantics support JSpecify only;
  non-JSpecify annotations are unsupported for this rule.

## Motivation

Objects implementing `java.lang.AutoCloseable` hold external resources — file handles, network sockets, database
connections — that must be released explicitly. When a method creates such an object locally and fails to call `close()`
on every reachable exit path, the resource leaks. This is easy to miss during review, especially when early returns or
exceptional paths are involved. Java's try-with-resources and Kotlin's `.use {}` exist precisely to prevent this, but
forgetting to use them is a common mistake.

## What it detects

- A method locally creates an `AutoCloseable` instance by:
    - invoking a constructor for a class assignable to `java.lang.AutoCloseable`, or
    - calling any method whose declared return type is assignable to `java.lang.AutoCloseable`
- The created resource remains locally owned inside the same method.
- At least one reachable exit path from that creation site reaches method exit without a later call to `close()` on the
  same locally tracked resource.
- The rule reports the local creation site whose `close()` is not guaranteed.

The following types are excluded from detection because their `close()` is known to be a no-op:

- `java.io.ByteArrayOutputStream`
- `java.io.ByteArrayInputStream`
- `java.io.StringBufferInputStream`
- `java.io.CharArrayWriter`
- `java.io.CharArrayReader`
- `java.io.StringWriter`
- `java.io.StringReader`

The following types are excluded from detection because their `close()` is a no-op in the vast majority of use cases
(created from a collection), and reporting them would produce excessive false positives:

- `java.util.stream.Stream`
- `java.util.stream.IntStream`
- `java.util.stream.LongStream`
- `java.util.stream.DoubleStream`

## What it does NOT detect

- AutoCloseables received from parameters, fields, or method return values.
- Cases where ownership is intentionally transferred out of the method, such as:
    - storing the resource into a field
    - storing the resource into an array or other heap-backed container
    - returning the resource
    - passing the resource as an argument to another method
- Wrapper delegation: when a locally created AutoCloseable is passed as a constructor argument to another
  AutoCloseable (e.g., `new BufferedReader(new FileReader(...))`), the inner resource is considered delegated to the
  outer and is not reported.
- Proof that `close()` happens in a different helper method after ownership transfer.
- Custom close methods (`release()`, `dispose()`, etc.) that are not `close()`.
- Suppression behavior via `@Suppress` or `@SuppressWarnings`.
- Rules based on non-JSpecify annotations.

## Examples (TP/TN/Edge)

### TP (reported)

```java
import java.io.FileInputStream;
import java.io.InputStream;

class ClassA {
    void methodX() {
        InputStream varOne = new FileInputStream("f.txt");
        varOne.read();
    }
}
```

### TN — try-with-resources (not reported)

```java
import java.io.FileInputStream;
import java.io.InputStream;

class ClassA {
    void methodX() {
        try (InputStream varOne = new FileInputStream("f.txt")) {
            varOne.read();
        }
    }
}
```

### TN — finally block (not reported)

```java
import java.io.FileInputStream;
import java.io.InputStream;

class ClassA {
    void methodX() {
        InputStream varOne = new FileInputStream("f.txt");
        try {
            varOne.read();
        } finally {
            varOne.close();
        }
    }
}
```

### TN — escape via field store (not reported)

```java
import java.io.FileInputStream;
import java.io.InputStream;

class ClassA {
    InputStream fieldOne;

    void methodX() {
        fieldOne = new FileInputStream("f.txt");
    }
}
```

### TN — escape via return (not reported)

```java
import java.io.FileInputStream;
import java.io.InputStream;

class ClassA {
    InputStream methodX() {
        return new FileInputStream("f.txt");
    }
}
```

### TN — escape via argument (not reported)

```java
import java.io.FileInputStream;
import java.io.InputStream;

class ClassA {
    void methodX(ClassB varTwo) {
        InputStream varOne = new FileInputStream("f.txt");
        varTwo.takeOwnership(varOne);
    }
}
```

### TN — wrapper pattern (not reported)

```java
import java.io.BufferedReader;
import java.io.FileReader;

class ClassA {
    void methodX() {
        BufferedReader varOne = new BufferedReader(new FileReader("f.txt"));
        try {
            varOne.readLine();
        } finally {
            varOne.close();
        }
    }
}
```

### TN — Kotlin .use {} (not reported)

```kotlin
import java.io.FileInputStream

class ClassA {
    fun methodX() {
        FileInputStream("f.txt").use { varOne ->
            varOne.read()
        }
    }
}
```

### TN — excluded no-op type (not reported)

```java
import java.io.ByteArrayOutputStream;

class ClassA {
    byte[] methodX() {
        ByteArrayOutputStream varOne = new ByteArrayOutputStream();
        varOne.write(42);
        return varOne.toByteArray();
    }
}
```

### Edge — early return (reported)

```java
import java.io.FileInputStream;
import java.io.InputStream;

class ClassA {
    void methodX(boolean varOne) {
        InputStream varTwo = new FileInputStream("f.txt");
        if (varOne) {
            return;
        }
        varTwo.close();
    }
}
```

### Edge — multiple resources, partial close (reported for varTwo only)

```java
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;

class ClassA {
    void methodX() {
        InputStream varOne = new FileInputStream("in.txt");
        OutputStream varTwo = new FileOutputStream("out.txt");
        try {
            varOne.read();
            varTwo.write(42);
        } finally {
            varOne.close();
        }
    }
}
```

### Edge — factory method returning AutoCloseable (reported)

```java
import java.nio.file.Files;
import java.nio.file.Path;
import java.io.InputStream;

class ClassA {
    void methodX() {
        InputStream varOne = Files.newInputStream(Path.of("f.txt"));
        varOne.read();
    }
}
```

## Output

- Report one finding per locally created AutoCloseable whose `close()` is not guaranteed on all reachable exits.
- Message must be actionable and include the method context, for example:
  `AutoCloseable created in <class>.<method><descriptor> may not be closed on all paths; use try-with-resources or call close() in a finally block.`
- Primary fix guidance: wrap the resource in a try-with-resources statement, or call `close()` in a `finally` block.

## Performance considerations

- Analysis should remain bounded by method CFG size and the number of locally created AutoCloseables in the method.
- Tracking must remain intraprocedural and deterministic.
- Output order and deduplication must be stable across repeated runs.

## Acceptance criteria

- Reports a locally created AutoCloseable when at least one reachable exit path after creation lacks `close()` in the
  same method.
- Does not report when all reachable exits after creation call `close()`.
- Does not report when ownership leaves the local method scope by field store, array store, return, or argument passing.
- Does not report when the resource is passed as a constructor argument to another AutoCloseable (wrapper delegation).
- Does not report excluded no-op types.
- Covers TP, TN, and edge cases in tests.
- Keeps `@Suppress`-style suppression unsupported and does not add non-JSpecify annotation semantics.
