---
type: 'Static Analysis Rule'
title: 'runBlocking reachable from coroutine'
description: 'runBlocking calls reachable from a coroutine through plain function calls block a shared coroutine thread'
tags: ['jvm', 'static-analysis']
status: 'stable'
rule_id: 'RUN_BLOCKING_REACHABLE_FROM_COROUTINE'
---

# RUN_BLOCKING_REACHABLE_FROM_COROUTINE

## Summary
- Rule ID: `RUN_BLOCKING_REACHABLE_FROM_COROUTINE`
- Name: runBlocking reachable from coroutine
- Description: runBlocking calls reachable from a coroutine through plain function calls block a shared coroutine thread
- Annotation policy: `@Suppress`-style suppression is unsupported. Annotation-driven semantics support JSpecify only; this rule has no annotation-driven semantics.

## Motivation
Coroutines share threads for execution. When `runBlocking` runs inside a coroutine, it blocks the underlying thread and prevents other coroutines from using it. This causes performance degradation and, in bad cases, thread starvation or deadlock.

The dangerous pattern is often indirect. A suspend function calls a plain helper function, and the helper calls `runBlocking`. No single method looks wrong in isolation, so only an interprocedural check can find it. This rule is a bytecode-level port of the IntelliJ "RunBlocking" inspection.

A sibling rule `RUN_BLOCKING_IN_SUSPEND_FUNCTION` owns `runBlocking` call sites that appear directly inside a compiled suspend function. This rule owns the exact complement, which is every other `runBlocking` call site reachable from a coroutine. The two rules never report the same call site.

## What it detects
The rule analyzes Kotlin-compiled analysis target classes. It produces no findings when the analysis input does not reference kotlinx.coroutines.

### Sinks
A sink is a call site in an analysis target method where:
- the call owner is `kotlinx/coroutines/BuildersKt`, and
- the call name is `runBlocking` or `runBlocking$default`.

The expected call shapes are the static calls
`runBlocking(Lkotlin/coroutines/CoroutineContext;Lkotlin/jvm/functions/Function2;)Ljava/lang/Object;` and
`runBlocking$default(Lkotlin/coroutines/CoroutineContext;Lkotlin/jvm/functions/Function2;ILjava/lang/Object;)Ljava/lang/Object;`.

### Roots (coroutine entry points)
A method in an analysis target class is a root when either:
1. It is a compiled suspend function. The detection shape is a trailing `Lkotlin/coroutines/Continuation;` parameter combined with a `Ljava/lang/Object;` return type. An array of `Continuation` as the last parameter does not match the shape.
2. It is a method named `invokeSuspend` with descriptor `(Ljava/lang/Object;)Ljava/lang/Object;` declared in a class whose direct superclass is `kotlin/coroutines/jvm/internal/SuspendLambda`. This covers the bodies of suspend lambdas passed to `launch`, `async`, `withContext`, `produce`, `actor`, and `runBlocking` itself.

A named suspend function declared on a restricted-suspension scope such as `SequenceScope` also matches the compiled suspend function shape and is treated as a root. The restriction annotation is not visible in the analyzed bytecode. Findings reachable only through such a function are a known, accepted false positive.

### Reachability
A sink is reported when its enclosing method is reachable from any root through call edges, with one exclusion. Reachability follows these constraints:
- Roots are reachable at distance zero. A sink inside a root method body is reachable.
- A call edge resolves only to the method with the exact called owner, name, and descriptor among analysis target classes. When the named owner declares no such method, the edge resolves to the nearest matching method inherited within the analysis target classes, searching the superclass chain first and then superinterfaces. This covers inherited concrete methods and interface default methods. Edges never expand to overriding implementations in subclasses.
- Edges never follow invokedynamic call sites, plain (non-suspend) lambdas, `Thread` construction, executor submission, or any other callback registration boundary.
- Cycles in the call graph must not prevent termination or duplicate findings.

### Exclusion (sibling ownership)
A sink call site is never reported when its enclosing method itself matches the compiled suspend function shape (trailing `Lkotlin/coroutines/Continuation;` parameter and `Ljava/lang/Object;` return type). Those call sites belong to `RUN_BLOCKING_IN_SUSPEND_FUNCTION`, even when the enclosing method is also reachable through a longer chain.

## What it does NOT detect
- `runBlocking` call sites directly inside a compiled suspend function. The sibling rule `RUN_BLOCKING_IN_SUSPEND_FUNCTION` owns those.
- `runBlocking` reachable only from plain non-coroutine code, such as a regular `fun main`. Top-level `runBlocking` is the intended use of the API.
- Reachability through virtual or interface calls dispatched to an overriding implementation in a subclass. Edges resolve to the statically named target only. An interface call with a single implementation is a known false negative, accepted for precision.
- Reachability through plain (non-suspend) lambdas, `Thread` bodies, executor tasks, or callback registrations. Code inside `Thread { ... }` runs on its own thread where blocking is acceptable.
- Chains passing through dependency (classpath-only) classes. Roots, edges, and sinks all live in analysis target classes.
- Dispatcher awareness. A chain that only runs on `Dispatchers.IO` at runtime is still reported when structurally reachable.
- Bodies of restricted-suspension builders such as `sequence { }`. Classes extending `kotlin/coroutines/jvm/internal/RestrictedSuspendLambda` are not roots because they run synchronously on the caller thread without a dispatcher.
- Default-value expressions evaluated inside the `$default` bridge of a suspend function. The bridge descriptor ends with `ILjava/lang/Object;`, so the bridge is not itself a root. It still participates as an ordinary node when called from a reachable method. This is a known, accepted false negative.
- Any suppression via `@Suppress` or `@SuppressWarnings`.

## Examples (TP/TN/Edge)

### TP: suspend function reaches runBlocking through a plain helper (reported)
```kotlin
package com.example

import kotlinx.coroutines.runBlocking

suspend fun main() {
    functionOne()
}

fun functionOne() {
    runBlocking { }
}
```
One finding at the `runBlocking` call in `functionOne`. The message chain names both frames, for example `com.example.MainKt.main -> com.example.MainKt.functionOne -> runBlocking`.

### TP: depth-two chain (reported)
```kotlin
suspend fun functionOne() { functionTwo() }

fun functionTwo() { functionThree() }

fun functionThree() { runBlocking { } }
```
One finding at the `runBlocking` call in `functionThree` with a three-frame chain.

### TP: plain helper called from a builder lambda (reported)
```kotlin
fun functionOne(scope: CoroutineScope) {
    scope.launch { functionTwo() }
}

fun functionTwo() { runBlocking { } }
```
The `launch` lambda body compiles to an `invokeSuspend` method of a `SuspendLambda` subclass, which is a root. One finding at the `runBlocking` call in `functionTwo`.

### TP: runBlocking directly inside a builder lambda (reported by this rule)
```kotlin
fun functionOne(scope: CoroutineScope) {
    scope.launch { runBlocking { } }
}
```
`invokeSuspend` has descriptor `(Ljava/lang/Object;)Ljava/lang/Object;` and does not match the compiled suspend function shape, so this rule owns the call site.

### TN: runBlocking reachable only from a plain entry point (not reported)
```kotlin
fun main() {
    runBlocking { functionOne() }
}

suspend fun functionOne() { }
```
`main` is not a root and no root reaches the call site. Top-level `runBlocking` is the intended bridge from blocking to suspending code.

### TN: runBlocking directly inside a suspend function (not reported by this rule)
```kotlin
suspend fun functionOne() {
    runBlocking { }
}
```
The enclosing method matches the compiled suspend function shape. `RUN_BLOCKING_IN_SUSPEND_FUNCTION` owns this call site.

### TN: thread boundary (not reported)
```kotlin
suspend fun functionOne() {
    Thread { functionTwo() }.start()
}

fun functionTwo() { runBlocking { } }
```
The plain lambda passed to `Thread` is not followed. The new thread does not share coroutine threads.

### TN: input without kotlinx.coroutines (silent)
A Java-only input, or any input that does not reference kotlinx.coroutines, produces no findings from this rule.

### Edge: two roots reach the same sink (one finding)
When two different roots reach the same `runBlocking` call site, exactly one finding is emitted. The rendered chain is the deterministic shortest chain. Among equally short chains the lexicographically smallest frame sequence is chosen.

### Edge: recursive cycle on the path (one finding)
A call cycle between methods on the path must not prevent termination. The sink is reported once.

### Edge: runBlocking$default bridge shape (reported)
A `runBlocking { }` call without an explicit context compiles to `runBlocking$default`. This shape is detected exactly like the two-argument `runBlocking` shape.

## Output
- One finding per reachable sink call site, not per (root, sink) pair.
- Location is the `runBlocking` call site, reported at the enclosing class and method with the source line of the call site when line information is available.
- Message template:
  `runBlocking is reachable from a coroutine and blocks a shared coroutine thread. Potential call stack: <chain> -> runBlocking. Call the suspend code directly instead of wrapping it in runBlocking.`
- Chain rendering rules:
  - Each frame renders as `<dotted-class-name>.<method-name>` with package separators as dots and no descriptors, for example `com.example.MainKt.functionOne`.
  - Frames are joined with ` -> `. The chain starts at a root frame, ends at the frame enclosing the sink, and is terminated by the literal `runBlocking`.
  - The chain is the shortest chain from any root to the enclosing method. Among equally short chains, the lexicographically smallest frame sequence is rendered.
  - The chain renders at most 10 frames. Longer chains render the first 5 frames, a single `...` element, and the last 5 frames.
  - Synthetic lambda frames render their JVM class name as compiled, for example `com.example.MainKt$functionOne$1.invokeSuspend`.
- The rule sorts its findings by enclosing class, method name, method descriptor, and call site offset before emission. The engine then orders the final SARIF results by rule id and message text.
- The same input analyzed twice produces identical findings, ordering, and messages.

## Performance considerations
- The rule performs one linear pass to index analysis target methods, one linear pass to resolve call edges with logarithmic lookups, and a single traversal from the root set. Overall cost is proportional to the number of target methods plus call sites.
- No CFG exploration and no dataflow analysis are used. Memory is bounded by one index entry per target method plus one predecessor record per reachable method.
- The kotlinx.coroutines gate keeps the rule at zero cost for inputs that do not reference the framework.
- Inheritance fallback during edge resolution is bounded by hierarchy size within analysis target classes.

## Acceptance criteria
1. Reports `runBlocking` and `runBlocking$default` call sites on `kotlinx/coroutines/BuildersKt` whose enclosing analysis target method is reachable from a coroutine root.
2. Treats compiled suspend functions (trailing `Lkotlin/coroutines/Continuation;` parameter with `Ljava/lang/Object;` return) as roots.
3. Treats `invokeSuspend(Ljava/lang/Object;)Ljava/lang/Object;` methods of classes whose direct superclass is `kotlin/coroutines/jvm/internal/SuspendLambda` as roots, and does not treat `RestrictedSuspendLambda` subclasses as roots.
4. Never reports a call site whose enclosing method matches the compiled suspend function shape, leaving it to `RUN_BLOCKING_IN_SUSPEND_FUNCTION`.
5. Resolves call edges only to the exact named target with a superclass-chain and superinterface fallback, without expansion to overriding implementations, and without following invokedynamic, `Thread`, executor, or callback boundaries.
6. Confines roots, edges, and sinks to analysis target classes.
7. Emits exactly one finding per reachable sink call site, with the message template above and a chain that is the deterministic shortest, lexicographically smallest path, capped at 10 frames with middle elision.
8. Produces no findings when the input does not reference kotlinx.coroutines.
9. Terminates on call graph cycles without duplicate findings.
10. Produces deterministic finding order, count, and messages across repeated runs of the same input.
11. Covers TP, TN, and edge cases from this spec in tests, including the `runBlocking$default` shape, the builder lambda root, the thread boundary, and the sibling exclusion.
12. Keeps `@Suppress`-style suppression unsupported and adds no non-JSpecify annotation semantics.
