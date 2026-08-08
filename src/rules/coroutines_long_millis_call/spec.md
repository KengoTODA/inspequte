---
type: 'Static Analysis Rule'
title: 'Coroutines call with raw Long milliseconds'
description: 'kotlinx.coroutines time-based calls that pass a raw Long milliseconds value should use the kotlin.time.Duration overload when it is available on the classpath'
tags: ['jvm', 'static-analysis']
status: 'stable'
rule_id: 'COROUTINES_LONG_MILLIS_CALL'
---

# COROUTINES_LONG_MILLIS_CALL

## Summary
- Rule ID: `COROUTINES_LONG_MILLIS_CALL`
- Name: Coroutines call with raw Long milliseconds
- Description: kotlinx.coroutines time-based calls that pass a raw Long milliseconds value should use the kotlin.time.Duration overload when it is available on the classpath
- Annotation policy: `@Suppress`-style suppression is unsupported. Annotation-driven semantics support JSpecify only. This rule has no annotation-driven semantics.

## Motivation
kotlinx.coroutines offers two families of time-based APIs. The older family takes a raw `Long` interpreted as milliseconds. The newer family takes `kotlin.time.Duration`, which carries its unit in the type. Raw `Long` arguments are easy to misread and easy to pass in the wrong unit, for example a value in seconds or nanoseconds obtained from another API. The Duration overloads make the unit explicit at the call site, such as `delay(500.milliseconds)`.

The Duration overloads only exist on sufficiently new kotlinx-coroutines versions. The rule therefore only reports when the Duration counterpart is confirmed to exist on the analysis classpath, so that the suggestion is always actionable.

## What it detects
- Call sites in analysis target classes that exactly match one of these JVM `(owner, name, descriptor)` triples:

  | Owner | Name | Descriptor |
  |-------|------|------------|
  | `kotlinx/coroutines/DelayKt` | `delay` | `(JLkotlin/coroutines/Continuation;)Ljava/lang/Object;` |
  | `kotlinx/coroutines/TimeoutKt` | `withTimeout` | `(JLkotlin/jvm/functions/Function2;Lkotlin/coroutines/Continuation;)Ljava/lang/Object;` |
  | `kotlinx/coroutines/TimeoutKt` | `withTimeoutOrNull` | `(JLkotlin/jvm/functions/Function2;Lkotlin/coroutines/Continuation;)Ljava/lang/Object;` |
  | `kotlinx/coroutines/flow/FlowKt` | `debounce` | `(Lkotlinx/coroutines/flow/Flow;J)Lkotlinx/coroutines/flow/Flow;` |
  | `kotlinx/coroutines/flow/FlowKt` | `sample` | `(Lkotlinx/coroutines/flow/Flow;J)Lkotlinx/coroutines/flow/Flow;` |

- A matching call site is reported only when the Duration counterpart is available. The Duration counterpart is available if and only if the owner class is resolvable from the analysis input and declares at least one method that satisfies both conditions below. `kotlin.time.Duration` is a JVM value class, so its overloads compile to name-mangled methods whose Duration parameter erases to `J`.
  - The method name starts with the plain function name followed by `-` (dash included, for example `delay-`). The dash makes the prefix exact, so `withTimeout-` never matches a mangled `withTimeoutOrNull` method.
  - The method descriptor equals the Long variant's descriptor from the table above.
- Kotlin-compiled and Java-compiled call sites alike. The rule performs no source-language discrimination, so a Java call site that invokes one of the listed static methods is also reported. This is accepted behavior and is expected to be rare.

## What it does NOT detect
- Calls to the Duration-based overloads themselves. Their mangled JVM names never equal the plain names in the table.
- Matching call sites when the owner class cannot be resolved from the analysis input. This keeps the rule silent when kotlinx-coroutines is absent from the classpath.
- Matching call sites when the resolved owner class declares no Duration counterpart, for example on old kotlinx-coroutines versions.
- Other milliseconds-based APIs, including `Thread.sleep`, `Object.wait`, the `kotlinx.coroutines.time` java.time interop functions, `onTimeout` select clauses, `delay` members on `Delay` implementations, and the `debounce`/`sample` overloads that take a selector function.
- Calls whose owner, name, or descriptor differs from the table, including user-defined functions that happen to be named `delay`, `withTimeout`, `debounce`, or `sample`.
- Shaded or relocated kotlinx-coroutines classes. Only the exact owner names in the table are matched.
- Argument values. There is no special-casing of constants such as `0L`, no unit inference, and no analysis of where the Long value came from.
- Call sites inside classpath-only (dependency) classes. Only analysis target classes are scanned.
- Concrete rewrite generation. The message is advisory and does not compute a Duration expression for the given argument.
- Any suppression behavior via `@Suppress` or `@SuppressWarnings`.

## Examples (TP/TN/Edge)
All examples assume a kotlinx-coroutines version on the analysis classpath that declares the Duration overloads, unless stated otherwise.

### TP: suspend function passes a Long to delay (reported)
```kotlin
package com.example

import kotlinx.coroutines.delay

suspend fun functionOne() {
    delay(500)
}
```
One finding naming `delay`.

### TP: withTimeout and withTimeoutOrNull with Long timeouts (reported)
```kotlin
package com.example

import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.withTimeoutOrNull

suspend fun functionOne(): String {
    val varOne = withTimeout(1_000) { "a" }
    val varTwo = withTimeoutOrNull(1_000) { "b" } ?: ""
    return varOne + varTwo
}
```
Two findings, one naming `withTimeout` and one naming `withTimeoutOrNull`.

### TP: flow operators with Long timeouts (reported)
```kotlin
package com.example

import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.sample

fun functionOne(varOne: Flow<Int>): Flow<Int> = varOne.debounce(200)

fun functionTwo(varOne: Flow<Int>): Flow<Int> = varOne.sample(200)
```
Two findings, one naming `debounce` and one naming `sample`.

### TN: Duration overload is already used (not reported)
```kotlin
package com.example

import kotlin.time.Duration.Companion.milliseconds
import kotlinx.coroutines.delay

suspend fun functionOne() {
    delay(500.milliseconds)
}
```
No finding. The compiled call targets the mangled Duration method, whose name differs from `delay`.

### TN: old library without Duration overloads (not reported)
The same Long call sites as the TP examples, analyzed against a kotlinx-coroutines version whose `DelayKt`, `TimeoutKt`, and `FlowKt` classes declare no Duration counterparts. No findings, because the suggestion would not be actionable.

### TN: owner class not on the analysis classpath (not reported)
The same Long call sites as the TP examples, analyzed without kotlinx-coroutines on the classpath. No findings, because the owner classes cannot be resolved and availability cannot be confirmed.

### TN: user-defined function with a matching simple name (not reported)
```kotlin
package com.example

fun delay(timeMillis: Long) {}

fun functionOne() {
    delay(500)
}
```
No finding. The call's owner class is not `kotlinx/coroutines/DelayKt`.

### Edge: multiple matching calls in one method (one finding each)
```kotlin
package com.example

import kotlinx.coroutines.delay

suspend fun functionOne() {
    delay(100)
    delay(200)
}
```
Two findings in deterministic order, one per call site.

### Edge: matching call site in a classpath-only class (not reported)
A dependency class on the classpath contains `delay(500)`. No finding, because only analysis target classes are scanned.

### Edge: Java call site (reported, accepted behavior)
A Java class in the analysis target invokes `FlowKt.debounce(varOne, 200L)` directly. The call site matches the `debounce` triple and is reported when the availability condition holds. The Duration overload is awkward to call from Java, and this noise is documented as accepted because such call sites are expected to be rare.

## Output
- Report one finding per matching call site.
- Message must be actionable and must name the called function:
  `Call to <function> in <class>.<method><descriptor> passes a raw Long milliseconds value. Use the kotlin.time.Duration overload available on the classpath, for example <function>(500.milliseconds).`
- `<function>` is the plain function name from the table (`delay`, `withTimeout`, `withTimeoutOrNull`, `debounce`, or `sample`).
- Location is reported at the enclosing method logical location and, where available, the source line of the call site.
- Findings are emitted in deterministic order, sorted by enclosing class name, method name, method descriptor, and call-site position.

## Performance considerations
- The call-site scan is linear in the instruction count of analysis target methods, with a constant-time match per call instruction.
- The availability condition involves at most three owner-class lookups plus a scan of each resolved owner's declared methods, evaluated once per analysis run. When no owner class satisfies the availability condition, the rule reports nothing.
- No CFG traversal, no data-flow analysis, and no inter-procedural analysis.
- Traversal order must be deterministic. Output depends only on the scanned class files, with no environment or timing sensitivity.

## Acceptance criteria
1. Reports one finding per call site in analysis target classes that exactly matches one of the five `(owner, name, descriptor)` triples, when the availability condition holds for that owner.
2. Treats the Duration counterpart as available if and only if the resolved owner class declares a method whose name starts with the plain function name plus `-` and whose descriptor equals the Long variant's descriptor.
3. Does not report any matching call site whose owner class cannot be resolved from the analysis input.
4. Does not report any matching call site when the resolved owner class declares no Duration counterpart.
5. Does not report Duration-overload call sites.
6. Does not report calls whose owner, name, or descriptor differs from the table, including user-defined functions with matching simple names.
7. Does not report call sites inside classpath-only classes.
8. Reports Java-compiled call sites the same way as Kotlin-compiled ones, as documented accepted behavior.
9. Emits the message template from `## Output` with the correct function name per finding.
10. Produces deterministic finding order and count across repeated runs on the same input.
11. Covers TP, TN, and edge cases in tests, including the availability-gate variations.
12. Keeps `@Suppress`-style suppression unsupported and adds no non-JSpecify annotation semantics.
