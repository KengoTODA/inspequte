# UNUSED_LAMBDA_PARAMETERS

## Summary

- Rule ID: `UNUSED_LAMBDA_PARAMETERS`
- Name: Unused lambda parameter
- Description: Reports lambda parameters that are never referenced in the lambda body. Unused parameters obscure data
  dependencies and can mask bugs where a wrong variable is used instead.
- Annotation policy: `@Suppress`/`@SuppressWarnings` are not supported; only JSpecify annotations are recognized for
  annotation-driven semantics, and non-JSpecify annotations do not change behavior.

## Motivation

Lambda parameters that are declared but never read add noise and can hide real bugs. A common pattern is passing the
wrong outer variable when the lambda parameter should have been used:

```kotlin
folder.children.forEach {
    // "it" should be used here, but "folder" is passed by mistake
    DoSomethingWithFolder(folder)
}
```

In Kotlin, the idiomatic fix is replacing the unused parameter with `_`. In Java 22+ (JEP 456), unnamed variables serve
the same purpose. For earlier Java versions the parameter cannot be omitted, but the finding still signals unnecessary
coupling to an API surface.

## What it detects

- A lambda parameter that is never referenced within the lambda body. One finding per unused parameter.
- Applies to:
    - Java lambda expressions.
    - Kotlin lambda expressions (non-inline), including implicit single-parameter `it`.
    - Kotlin inline lambda expressions (e.g. `forEach`, `map`, `let`). If debug metadata is absent, inline lambda
      analysis is silently skipped (no false positives).
    - SAM conversions in both Java and Kotlin.

## What it does NOT detect

- Parameters that are loaded but whose value is not meaningfully consumed (no data-flow analysis beyond reference
  presence).
- Parameters in hand-written anonymous inner classes (non-SAM, non-lambda).
- Parameters in method references (`Class::method`) — no new parameter scope is introduced.
- Lambda parameters named `_` (intentionally unused by convention in Kotlin; unnamed variables in Java 22+ / JEP 456).
- Kotlin suspend lambda continuation parameters (synthetic, not user-declared).
- Suppression via annotations (`@Suppress`, `@SuppressWarnings`).
- Behavior changes based on non-JSpecify annotations.

## Examples (TP/TN/Edge)

### TP: Java lambda with unused parameter (reported)

```java
class ClassA {
    void methodX() {
        java.util.List.of("varOne").forEach(varTwo -> {
            System.out.println("hello");
        });
    }
}
```

### TP: Java lambda with multiple params, one unused (reported for varThree)

```java
class ClassB {
    void methodX() {
        java.util.Map.of("varOne", "varTwo").forEach((varThree, varFour) -> {
            System.out.println(varFour);
        });
    }
}
```

### TP: Kotlin inline lambda with implicit `it` never referenced (reported)

```kotlin
class ClassC {
    fun methodX() {
        listOf("varOne").forEach {
            println("hello")
        }
    }
}
```

### TP: Kotlin non-inline lambda with unused parameter (reported for varTwo)

```kotlin
class ClassD {
    fun methodX(varOne: java.util.function.Consumer<String>) {
        varOne.accept("hello")
    }
    fun methodY() {
        methodX(java.util.function.Consumer { varTwo -> println("hello") })
    }
}
```

### TP: Kotlin non-inline lambda with implicit `it` never referenced (reported)

```kotlin
class ClassE {
    fun methodX(varOne: java.util.function.Consumer<String>) {
        varOne.accept("hello")
    }
    fun methodY() {
        methodX(java.util.function.Consumer { println("hello") })
    }
}
```

### TP: Kotlin inline lambda with named parameter unused (reported for varTwo)

```kotlin
class ClassF {
    fun methodX() {
        listOf("varOne").forEach { varTwo ->
            println("hello")
        }
    }
}
```

### TN: Java lambda where parameter is used (not reported)

```java
class ClassG {
    void methodX() {
        java.util.List.of("varOne").forEach(varTwo -> {
            System.out.println(varTwo);
        });
    }
}
```

### TN: Kotlin lambda with `_` for unused parameter (not reported)

```kotlin
class ClassH {
    fun methodX() {
        mapOf("varOne" to "varTwo").forEach { (_, varThree) ->
            println(varThree)
        }
    }
}
```

### TN: Regular method with unused parameter (not reported)

```java
class ClassI {
    void methodX(String varOne) {
        System.out.println("hello");
    }
}
```

### TN: Kotlin inline lambda where parameter is used (not reported)

```kotlin
class ClassJ {
    fun methodX() {
        listOf("varOne").forEach { varTwo ->
            println(varTwo)
        }
    }
}
```

### TN: Kotlin inline lambda with `_` (not reported)

```kotlin
class ClassK {
    fun methodX() {
        listOf("varOne").forEach { _ ->
            println("hello")
        }
    }
}
```

### TN: Method reference (not reported)

```java
class ClassL {
    void methodX() {
        java.util.List.of("varOne").forEach(System.out::println);
    }
}
```

### Edge: Lambda capturing outer variable — only lambda params are checked (reported for varThree)

```java
class ClassM {
    void methodX(String varOne) {
        java.util.List.of("varTwo").forEach(varThree -> {
            System.out.println(varOne);
        });
    }
}
```

### Edge: Two-argument lambda, only second used (reported for varTwo)

```java
class ClassN {
    void methodX() {
        java.util.Map.of("varOne", 1).forEach((varTwo, varThree) -> {
            System.out.println(varThree);
        });
    }
}
```

### Edge: Kotlin SAM conversion with unused parameter (reported for varThree)

```kotlin
class ClassO {
    fun interface FuncA {
        fun invoke(varOne: String)
    }
    fun methodX() {
        val varTwo = FuncA { varThree -> println("hello") }
    }
}
```

### Edge: Nested inline lambdas (reported for varThree only; varTwo is used as receiver of `.also`)

```kotlin
class ClassP {
    fun methodX() {
        listOf(listOf("varOne")).forEach { varTwo ->
            varTwo.also { varThree ->
                println("hello")
            }
        }
    }
}
```

## Output

- Message should be actionable and include method context, for example:
  `Unused lambda parameter in <class>.<method><descriptor>: parameter at index <N> is never referenced.`
- Location should point to the lambda declaration line when line metadata is available.

## Performance considerations

- Analysis should be linear in total bytecode size.
- No whole-program data-flow or CFG construction is required.
- Inline lambda detection depends on debug metadata; absence of metadata means no findings for that path (safe
  degradation, not an error).
- Result order should be deterministic across runs.

## Acceptance criteria

- Reports each lambda parameter that is never referenced in the lambda body.
- Does not report parameters named `_` (Kotlin convention or Java 22+ unnamed variables).
- Does not report synthetic continuation parameters in Kotlin suspend lambdas.
- Does not report captured outer variables as unused lambda parameters.
- Does not report parameters in hand-written anonymous inner classes or method references.
- Silently skips inline lambda analysis when debug metadata is absent (no false positives, no errors).
- Covers TP, TN, and edge scenarios in tests.
- Produces deterministic finding count and ordering.
- Keeps `@Suppress`-style suppression unsupported and does not add non-JSpecify annotation semantics.
