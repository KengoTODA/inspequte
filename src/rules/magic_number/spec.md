# MAGIC_NUMBER

## Summary

- Rule ID: `MAGIC_NUMBER`
- Name: Magic number
- Description: Numeric literals used directly in method bodies reduce readability and maintainability; extract them into
  named constants.

## Motivation

Unnamed numeric literals ("magic numbers") obscure the intent of code and make it fragile to change. When the same value
appears in multiple locations, a change to one occurrence but not others introduces silent bugs. Extracting values into
named constants makes the purpose explicit, centralizes changes, and improves searchability.

Source-level tools (Checkstyle, PMD) already detect magic numbers, but inspequte operates on bytecode. This brings
detection to environments where a source is unavailable and catches literals that survive compilation. A known
limitation is that `javac` inlines compile-time constants (`static final` primitives with constant initializers) at
usage sites, making them indistinguishable from true magic numbers at the bytecode level.

## What it detects

Numeric literal values loaded in method bodies that are **not** in the built-in allowlist and are **not** in a
known-safe context.

Targeted numeric-constant-loading instructions:

- `bipush` (byte-range integers)
- `sipush` (short-range integers)
- `ldc` / `ldc_w` / `ldc2_w` loading integer, long, float, or double constants

The rule applies to all non-synthetic, non-bridge methods, including `<clinit>` (class initializers).

## What it does NOT detect

- Values loaded via dedicated small-constant opcodes (`iconst_*`, `lconst_*`, `fconst_*`, `dconst_*`) — these encode
  only a few common values and are not `bipush`/`sipush`/`ldc`.
- Values in the built-in allowlist:
    - Integers: -1, 0, 1, 2
    - Longs: 0L, 1L, 2L
    - Floats: 0.0F, 1.0F
    - Doubles: 0.0, 1.0
    - Powers of two up to 1024 (2, 4, 8, 16, 32, 64, 128, 256, 512, 1024)
    - Common bit masks: 0xFF, 0xFFFF, 0xFFFFFFFF
- Values in known-safe instruction contexts:
    - Array creation sizes (immediate predecessor of `newarray`, `anewarray`,
      `multianewarray`)
    - `tableswitch` / `lookupswitch` case values
    - Initial capacity arguments for collection-like types (`StringBuilder`,
      `StringBuffer`, `Collection`, `Map`)
    - Enum constructor arguments in `<clinit>` (constants passed to `invokespecial <init>` on
      enum subclasses of `java/lang/Enum`)
    - Values used in annotation contexts
    - Values used in the body of `hashCode()` methods
- Synthetic or bridge methods.
- String literals (magic strings are a separate concern).
- Cross-class analysis to determine whether a value is defined as a named constant elsewhere.
- Inlined compile-time constants that are indistinguishable from raw literals at the bytecode level (fundamental
  limitation, documented as a known source of false positives).
- `@Suppress`-style annotation suppression is not supported.
- Non-JSpecify annotation semantics are not supported.

## Examples (TP/TN/Edge)

### True Positive — non-allowlisted integer literal

```java
class Timeout {
    void resetIfExpired(int elapsed) {
        if (elapsed > 3600) { // bipush/sipush 3600
            resetSession();
        }
    }

    void resetSession() {
    }
}
```

Reported: the literal `3600` is not in the allowlist and is not in a safe context.

### True Positive — non-allowlisted float literal

```java
class Physics {
    double gravity() {
        return 9.81; // ldc 9.81
    }
}
```

Reported: the literal `9.81` is not in the allowlist.

### True Negative — allowlisted values

```java
class Indexing {
    int next(int index) {
        return index + 1; // iconst_1 or bipush 1 — allowlisted
    }

    int mask(int value) {
        return value & 0xFF; // allowlisted bit mask
    }
}
```

Not reported: `1` and `0xFF` are in the built-in allowlist.

### True Negative — array creation size

```java
class Buffer {
    byte[] allocate() {
        return new byte[4096]; // array creation size context
    }
}
```

Not reported: the literal is an immediate predecessor of a `newarray` instruction.

### True Negative — hashCode method

```java
class Point {
    int x, y;

    @Override
    public int hashCode() {
        return 31 * x + y; // inside hashCode()
    }
}
```

Not reported: numeric literals in `hashCode()` bodies are excluded.

### Edge — static final initializer in clinit

```java
class Config {
    static final int TIMEOUT = 3600;
    // If TIMEOUT is NOT a compile-time constant (e.g., assigned from a method),
    // the literal 3600 appears in <clinit> and is reported.
    // If TIMEOUT IS a compile-time constant, javac inlines it and <clinit>
    // may not contain the literal at all.
}
```

### Edge — negative value via bipush

```java
class Range {
    boolean isValid(int value) {
        return value > -128; // bipush -128 — not in allowlist, reported
    }
}
```

Reported: `-128` is not in the allowlist (only `-1` is allowlisted).

### Edge — tableswitch case values

```java
class Dispatcher {
    void dispatch(int code) {
        switch (code) {
            case 200:
                handle200();
                break;
            case 404:
                handle404();
                break;
            default:
                handleOther();
                break;
        }
    }

    void handle200() {
    }

    void handle404() {
    }

    void handleOther() {
    }
}
```

Not reported: `200` and `404` are case values within a `tableswitch` / `lookupswitch` instruction and are excluded.

### True Negative — enum constructor arguments

```java
enum Duration {
    ONE_HOUR(3600),
    TWO_HOURS(7200);

    private final int seconds;

    Duration(int seconds) {
        this.seconds = seconds;
    }

    int getSeconds() {
        return seconds;
    }
}
```

Not reported: numeric literals passed to `invokespecial <init>` on the enum class in `<clinit>` are recognized as enum
constructor arguments and excluded.

### True Negative — Kotlin const val in companion object

```kotlin
class Config {
    companion object {
        const val TIMEOUT = 3600
    }
}
```

Not reported: `const val` values are compile-time constants. The Kotlin compiler inlines them at usage sites, so the
literal does not appear in a method body that would be scanned.

### True Positive — annotation default values

```java
@interface Retry {
    int maxAttempts() default 3600;
}
```

Reported: the annotation method `maxAttempts` has a numeric default value `3600` that is not in the allowlist.
Annotation defaults are extracted from the `AnnotationDefault` attribute.

### True Positive — Kotlin default argument values

```kotlin
class Service {
    fun connect(timeout: Int = 3600): Boolean {
        return true
    }
}
```

Reported: the Kotlin compiler emits a synthetic `connect$default` method containing the default value `3600`. The rule
scans these synthetic `$default` methods and attributes findings to the enclosing real method.

## Output

Findings are reported as:

```
Magic number <value> in <class>.<method><descriptor>
```

Where `<value>` is the numeric literal, `<class>` is the fully qualified class name, `<method>` is the method name, and
`<descriptor>` is the method descriptor.

## Performance considerations

- Linear scan: O(N × M) where N is the number of methods per class and M is the number of instructions per method.
- No inter-method or inter-class analysis is required; each method is evaluated independently.
- Allowlist lookup is constant-time.
- No additional passes or shared analysis artifacts beyond standard class-file parsing are needed.

## Acceptance criteria

- The rule reports numeric literals not in the built-in allowlist and not in known-safe contexts.
- The rule does not report allowlisted values or values in excluded contexts (array sizes, switch cases, collection
  capacities, annotations, hashCode bodies).
- The rule does not report findings in synthetic or bridge methods.
- Findings are deterministic: identical input produces identical findings in identical order.
- Finding order is stable: sorted by (class name, method name, descriptor, bytecode offset).
- Unit tests cover true positive, true negative, and edge cases as listed above.
