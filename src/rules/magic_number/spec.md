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
    - Values used in annotation contexts
    - Values used in the body of `hashCode()` methods
- Synthetic or bridge methods.
- String literals (magic strings are a separate concern).
- Cross-class analysis to determine whether a value is defined as a named constant elsewhere.
- Inlined compile-time constants that are indistinguishable from raw literals at the bytecode level (fundamental
  limitation, documented as a known source of false positives).
- Annotation element default values — stored in `AnnotationDefault` attributes, never in `Code` attributes.
- Kotlin `companion object { const val NAME = <value> }` — `const val` compiles to a JVM `static final` field
  carrying a `ConstantValue` attribute; no `bipush`/`sipush`/`ldc` instruction appears in `<clinit>`.
- Enum constructor arguments whose values are in the allowlist — reported only for non-allowlisted values
  (known false positive; see edge cases below).
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

Not reported: `200` and `404` are case values within a `tableswitch` /
`lookupswitch` instruction and are excluded.

### Edge — enum constructor arguments (known false positives for non-allowlisted values)

```java
// NOT reported — 8 and 32 are in the allowlist (powers of two)
enum EnumA {
    ITEM_ONE(8), ITEM_TWO(32);
    private final int valOne;
    EnumA(int valOne) { this.valOne = valOne; }
}

// REPORTED (false positive) — 9 and 200 are not in the allowlist
enum EnumB {
    ITEM_ONE(9), ITEM_TWO(200);
    private final int valOne;
    EnumB(int valOne) { this.valOne = valOne; }
}
```

Enum constant declarations pass their constructor arguments through the compiler-generated `<clinit>`. The JVM
instruction stream in `<clinit>` is indistinguishable from user-written code at the bytecode level: for
`ITEM_TWO(200)`, `javac` emits `sipush 200` followed by `invokespecial <init>`, exactly as it would for a
hand-written magic number.

Allowlisted values such as `8` and `32` are excluded by the normal allowlist check. Non-allowlisted values such
as `9` and `200` are reported as magic numbers — a **known false positive** for enum classes.

### Edge — Kotlin companion object `const val`

```kotlin
class ClassA {
    companion object {
        const val CONST_VAL = 3600
    }
}
```

Not reported: `const val` compiles to a JVM `static final` field with a `ConstantValue` attribute. The JVM
initialises the field directly from that attribute; no push instruction appears in `<clinit>`. This is the Kotlin
equivalent of a Java compile-time constant. By contrast, a non-`const` companion object property (`val`) generates
`<clinit>` code and its initialiser value **is** reported.

### Edge — annotation element default values

```java
public @interface MaxRetryA {
    int maxAttempts() default 3;
    long timeoutMs() default 1000;
}
```

Not reported: annotation element default values are stored in the `AnnotationDefault` attribute, not in a `Code`
attribute. Annotation element methods carry no bytecode, so no push instruction is ever scanned.

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
