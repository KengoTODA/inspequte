---
type: 'Static Analysis Rule'
title: 'String case conversion without explicit locale'
description: 'String.toLowerCase()/toUpperCase() calls without Locale argument'
tags: ['jvm', 'static-analysis']
status: 'stable'
rule_id: 'STRING_CASE_WITHOUT_LOCALE'
---

# STRING_CASE_WITHOUT_LOCALE

## Summary
- Rule ID: `STRING_CASE_WITHOUT_LOCALE`
- Name: String case conversion without explicit locale
- Description: Reports `String.toLowerCase()` and `String.toUpperCase()` calls that do not pass a `Locale`, because results depend on the default locale.
- Annotation policy: `@Suppress`/`@SuppressWarnings` are not supported; only JSpecify annotations are recognized for annotation-driven semantics, and non-JSpecify annotations do not change behavior.

## Motivation
Default-locale case conversion can behave differently across runtime environments. This causes subtle bugs that are hard to reproduce locally and often appear only in specific locales. Requiring an explicit locale makes behavior predictable.

## What it detects
- Calls to `java.lang.String.toLowerCase()` with descriptor `()Ljava/lang/String;`.
- Calls to `java.lang.String.toUpperCase()` with descriptor `()Ljava/lang/String;`.
- One finding per matching call site.

## What it does NOT detect
- Locale-aware overloads that already pass a `java.util.Locale` argument.
- Non-String APIs that perform case conversion.
- Suppression via annotations (`@Suppress`, `@SuppressWarnings`).
- Behavior changes based on non-JSpecify annotations.

## Examples (TP/TN/Edge)
### TP (reported)
```java
import java.util.Locale;

class ClassA {
    String methodX(String varOne) {
        return varOne.toLowerCase();
    }
}
```

### TN (not reported)
```java
import java.util.Locale;

class ClassB {
    String methodY(String varOne) {
        return varOne.toUpperCase(Locale.ROOT);
    }
}
```

### Edge (only locale-less call reported)
```java
import java.util.Locale;

class ClassC {
    String methodZ(String varOne) {
        String varTwo = varOne.toLowerCase();
        String varThree = varOne.toUpperCase(Locale.ROOT);
        return varTwo + varThree;
    }
}
```

## Output
- Message should be actionable and include method context, for example:
  `String case conversion in <class>.<method><descriptor> uses default locale; pass Locale.ROOT (or an explicit Locale) to make behavior deterministic.`
- Location should point to the call site line when line metadata is available.

## Performance considerations
- Analysis should be linear in the number of discovered call sites.
- No whole-program dataflow is required.
- Result order should be deterministic across runs.

## Acceptance criteria
- Reports each `String.toLowerCase()`/`String.toUpperCase()` call that omits `Locale`.
- Does not report locale-aware overloads with `Locale` argument.
- Covers TP, TN, and edge scenarios in tests.
- Produces deterministic finding count and ordering.
- Keeps `@Suppress`-style suppression unsupported and does not add non-JSpecify annotation semantics.
