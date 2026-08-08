---
type: 'Static Analysis Rule'
title: 'runBlocking in suspend function'
description: 'runBlocking inside a suspend function blocks the calling thread and defeats asynchronous execution'
tags: ['jvm', 'static-analysis', 'kotlin', 'coroutines']
status: 'stable'
rule_id: 'RUN_BLOCKING_IN_SUSPEND_FUNCTION'
---

# RUN_BLOCKING_IN_SUSPEND_FUNCTION

## Summary
- Rule ID: `RUN_BLOCKING_IN_SUSPEND_FUNCTION`
- Name: runBlocking in suspend function
- Description: runBlocking inside a suspend function blocks the calling thread and defeats asynchronous execution

The rule reports calls to `kotlinx.coroutines.runBlocking` that occur inside compiled Kotlin `suspend` functions. Analysis operates on JVM bytecode. The rule is modeled after the IntelliJ inspection `RunBlockingInSuspendFunction`.

## Motivation
`runBlocking` blocks the calling thread until its coroutine completes. Inside a `suspend` function the caller is already running in a coroutine. Blocking the thread there defeats asynchronous programming, wastes the thread, and can deadlock when the blocked thread is the one needed to resume the coroutine. The correct pattern is to call the suspending code directly, or to use `withContext(...)` when a specific dispatcher or context is needed.

## What it detects
The rule reports a finding when all of the following hold.

1. The enclosing method has the compiled `suspend` shape. Its final parameter type is `Lkotlin/coroutines/Continuation;` and its return type is `Ljava/lang/Object;`.
2. The method contains a static call to owner `kotlinx/coroutines/BuildersKt` with one of these exact name and descriptor pairs.
   - `runBlocking(Lkotlin/coroutines/CoroutineContext;Lkotlin/jvm/functions/Function2;)Ljava/lang/Object;`
   - `runBlocking$default(Lkotlin/coroutines/CoroutineContext;Lkotlin/jvm/functions/Function2;ILjava/lang/Object;)Ljava/lang/Object;`
3. The enclosing class is part of the analysis target, not a classpath or dependency class.

Both the no-context form `runBlocking { ... }` (compiled to the `runBlocking$default` bridge) and the explicit-context form `runBlocking(context) { ... }` (compiled to the full signature) are reported. Each matching call site produces exactly one finding.

When the analysis input contains no reference to kotlinx.coroutines, the rule produces no findings.

## What it does NOT detect
- `runBlocking` calls in non-suspend methods. Entry points such as `main`, tests, and blocking bridge code are legitimate `runBlocking` call sites.
- `runBlocking` calls inside suspend lambdas. The synthetic `invokeSuspend` bodies of compiler-generated lambda classes are out of scope for this version, so such calls are a documented false negative.
- Other blocking APIs inside suspend functions, such as `Thread.sleep`, `Future.get`, or blocking IO. Those are separate concerns.
- Inter-procedural cases. A suspend function calling a non-suspend helper that itself calls `runBlocking` is not reported.
- Actual deadlock proof or dispatcher analysis. The call site itself is the finding.
- Calls named `runBlocking` on any owner other than `kotlinx/coroutines/BuildersKt`, or with a different descriptor.

Annotation policy is as follows. `@Suppress`-style suppression (including `@Suppress` and `@SuppressWarnings`) is unsupported and never hides a finding. Annotation-driven semantics are JSpecify-only project-wide. This rule uses no annotation-driven semantics, and non-JSpecify annotations must not change its behavior.

## Examples (TP/TN/Edge)

### True positive
`runBlocking` without a context argument inside a suspend function.

```kotlin
suspend fun methodA(): String = runBlocking {
    loadValue()
}
```

`runBlocking` with an explicit context inside a suspend function.

```kotlin
suspend fun methodB(context: CoroutineContext): String = runBlocking(context) {
    loadValue()
}
```

### True negative
`runBlocking` in a non-suspend function, such as an entry point.

```kotlin
fun methodC() {
    runBlocking {
        loadValue()
    }
}
```

A suspend function that uses `withContext` or plain suspending calls.

```kotlin
suspend fun methodD(): String = withContext(context) {
    loadValue()
}
```

### Edge
Two `runBlocking` calls in one suspend function produce two findings in call-site order.

```kotlin
suspend fun methodE() {
    runBlocking { loadValue() }
    runBlocking { loadValue() }
}
```

`runBlocking` inside a suspend lambda passed to a builder is not reported. The lambda body compiles into a synthetic class outside this rule's scope.

```kotlin
fun methodF(scope: CoroutineScope) {
    scope.launch {
        runBlocking { loadValue() }  // not reported (documented non-goal)
    }
}
```

A non-Kotlin method that happens to take a trailing `Continuation` parameter is only treated as suspend when its return type is also `Ljava/lang/Object;`.

## Output
One finding per matching call site with the enclosing class, method, and descriptor, plus the source line when line information is available.

Findings are reported as `runBlocking in suspend function <class>.<method><descriptor> blocks the calling thread and can deadlock. Call the suspending code directly, or use withContext(...) when a specific dispatcher or context is needed.`

Findings are deterministic. Running the same input twice produces identical findings in identical order. Findings are ordered by class, method, descriptor, and call-site position.

## Performance considerations
- The rule performs a single linear pass over analysis target methods. Cost is proportional to the number of methods plus the instructions of methods that match the suspend shape.
- The suspend-shape check is a constant-time descriptor inspection per method, so non-suspend methods are skipped without instruction scanning.
- When the analysis input contains no kotlinx.coroutines reference, the rule does no per-method work and produces no findings.
- No dataflow, control-flow graph, or inter-procedural analysis is required.

## Acceptance criteria
- A suspend function calling `runBlocking { ... }` without a context argument is reported (the `runBlocking$default` bridge shape).
- A suspend function calling `runBlocking(context) { ... }` with an explicit context is reported (the full signature shape).
- A non-suspend function calling `runBlocking { ... }` is not reported.
- A suspend function using `withContext(...)` or plain suspending calls, with no `runBlocking`, is not reported.
- `runBlocking` inside a suspend lambda passed to a coroutine builder is not reported.
- Two `runBlocking` calls in one suspend function produce exactly two findings in call-site order.
- An analysis input without kotlinx.coroutines references produces no findings.
- Only calls whose owner is `kotlinx/coroutines/BuildersKt` with one of the two exact descriptors are reported.
- Findings include class, method, descriptor, and source line when available, and use the message shape defined in `## Output`.
- Repeated runs on the same input produce byte-identical findings and ordering.
- `@Suppress`-style annotations do not suppress findings, and non-JSpecify annotations do not change behavior.
