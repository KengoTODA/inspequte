---
type: 'Static Analysis Rule'
title: 'Coroutine context with Job'
description: 'Coroutine builder and withContext calls that pass a CoroutineContext containing a Job element break structured concurrency'
tags: ['jvm', 'static-analysis', 'kotlin', 'coroutines']
status: 'stable'
rule_id: 'COROUTINE_CONTEXT_WITH_JOB'
---

# COROUTINE_CONTEXT_WITH_JOB

## Summary
- Rule ID: `COROUTINE_CONTEXT_WITH_JOB`
- Name: Coroutine context with Job
- Description: Detects kotlinx.coroutines builder and `withContext` calls that pass a `CoroutineContext` containing a `Job` element created by `Job()` or `SupervisorJob()`, which breaks structured concurrency.
- Annotation policy: `@Suppress`-style suppression is unsupported. Annotation-driven semantics support JSpecify only; this rule has no annotation-driven semantics.

## Motivation
Coroutine builders inherit their parent job from the receiver scope. When the caller passes a context that contains its own `Job` element, for example `scope.launch(Job()) { ... }` or `withContext(SupervisorJob()) { ... }`, the new coroutine is no longer a child of the scope. Cancelling the scope no longer cancels the coroutine, failures no longer propagate to the parent, and `join`/`cancel` semantics silently change.

The code compiles and often appears to work, so this defect survives code review easily. The rule mirrors the IntelliJ inspection `CoroutineContextWithJob` while operating on JVM bytecode from class files and JARs.

## What it detects
The rule reports a builder or `withContext` call site in an analysis target method when all of the following hold:

- kotlinx.coroutines is visible to the scan. Otherwise the rule reports nothing.
- The method contains a call to one of the job factories, in plain or `$default` compiled form:
  - `Job()` on `kotlinx/coroutines/JobKt`
  - `SupervisorJob()` on `kotlinx/coroutines/SupervisorKt`
  The factory is tracked whether or not a parent job argument is given to it.
- Within the same method, the factory result reaches the `CoroutineContext` argument of one of these calls, in plain or `$default` compiled form:
  - `launch` on `kotlinx/coroutines/BuildersKt`
  - `async` on `kotlinx/coroutines/BuildersKt`
  - `withContext` on `kotlinx/coroutines/BuildersKt`
  - `produce` on `kotlinx/coroutines/channels/ProduceKt`
  - `actor` on `kotlinx/coroutines/channels/ActorKt`
- The value may reach the context argument directly, through a local variable, or through `CoroutineContext.plus` combinations in either operand position, including chains such as `Dispatchers.IO + Job() + CoroutineName("x")`.

## What it does NOT detect
- Job values that cross method boundaries. Jobs created in another method, received as a parameter, or read from a field are out of scope.
- Arbitrary `Job`-typed values that were not produced by `Job()` or `SupervisorJob()` in the same method, including jobs obtained from `coroutineContext[Job]` or from an existing scope.
- Contexts that flow into `CoroutineScope(...)` construction. Creating a root scope with its own job is the intended use of the job factories and is never reported.
- The `promise` builder. It exists only on Kotlin/JS and never appears in JVM bytecode.
- `future` from kotlinx-coroutines-jdk8, reactive builders from rxjava or reactor integrations, and `runBlocking`.
- The pattern hidden behind helper functions that wrap the builders.
- Classpath-only classes that contain the pattern but are not part of the analysis target.
- Any suppression behavior via `@Suppress` or `@SuppressWarnings`.

## Examples (TP/TN/Edge)

### TP: Job passed directly to launch (reported)
```kotlin
fun funOne(scope: CoroutineScope) {
    scope.launch(Job()) { }
}
```
Finding reported: the launched coroutine is not a child of `scope`.

### TP: SupervisorJob passed to async (reported)
```kotlin
fun funOne(scope: CoroutineScope) {
    scope.async(SupervisorJob()) { }
}
```
Finding reported: `SupervisorJob()` detaches the coroutine from `scope` just like `Job()`.

### TP: Job passed to withContext (reported)
```kotlin
suspend fun funOne() {
    withContext(Job()) { }
}
```
Finding reported: the block runs under a foreign job instead of the caller's job.

### TP: Job combined into a context with plus (reported)
```kotlin
fun funOne(scope: CoroutineScope) {
    scope.launch(Dispatchers.IO + Job()) { }
}
```
Finding reported: the combined context still contains the `Job` element.

### TP: Job stored in a local variable first (reported)
```kotlin
fun funOne(scope: CoroutineScope) {
    val varOne = Job()
    scope.launch(varOne) { }
}
```
Finding reported: the flow through the local variable is tracked within the method.

### TN: context without a Job element (not reported)
```kotlin
fun funOne(scope: CoroutineScope) {
    scope.launch(Dispatchers.IO) { }
}
```
No finding: the context contains no tracked `Job` element.

### TN: builder call with default context (not reported)
```kotlin
fun funOne(scope: CoroutineScope) {
    scope.launch { }
}
```
No finding: the compiled `$default` form receives `EmptyCoroutineContext`.

### TN: root scope construction (not reported)
```kotlin
fun funOne(): CoroutineScope {
    return CoroutineScope(Job() + Dispatchers.IO)
}
```
No finding: giving a root scope its own job is the intended use of the factory.

### TN: job used only for lifecycle control (not reported)
```kotlin
suspend fun funOne() {
    val varOne = Job()
    varOne.cancel()
    varOne.join()
}
```
No finding: the job never reaches a builder or `withContext` context argument.

### Edge: only the offending builder call is reported
```kotlin
fun funOne(scope: CoroutineScope) {
    scope.launch(Dispatchers.IO) { }
    scope.launch(Job()) { }
}
```
Exactly one finding is reported, at the second `launch` call.

### Edge: channel builders (reported)
```kotlin
fun funOne(scope: CoroutineScope) {
    scope.produce<Int>(Job()) { }
    scope.actor<Int>(SupervisorJob()) { }
}
```
Both the `produce` and the `actor` call are reported.

### Edge: kotlinx-coroutines absent from the scan
If kotlinx.coroutines is not visible to the scan, the rule performs no analysis and reports nothing.

## Output
- Report one finding per offending builder or `withContext` call site.
- Message must be actionable:
  `<builder> call in <class>.<method><descriptor> passes a coroutine context containing a Job element created by <factory>; the coroutine will not be a child of the calling scope, so cancellation and failure propagation break. Remove the Job element from the context, or create an explicit CoroutineScope if an independent lifecycle is intended.`
  where `<builder>` is one of `launch`, `async`, `withContext`, `produce`, `actor` and `<factory>` is `Job()` or `SupervisorJob()`.
- Location is reported at the enclosing method logical location and, where available, the source line of the builder call.

## Performance considerations
- When kotlinx.coroutines is not visible to the scan, the rule is a cheap no-op.
- Analysis is bounded by a single linear pass over the instructions of each analysis target method.
- No inter-procedural analysis and no exploration beyond the enclosing method.
- Traversal order must be deterministic: classes in analysis-target order, methods in declaration order, instructions in bytecode offset order. Findings are ordered by `(class, method, descriptor, builder call offset)`.

## Acceptance criteria
1. Reports `launch`, `async`, and `withContext` calls whose context argument receives a value produced by `Job()` or `SupervisorJob()` in the same method, passed directly.
2. Reports the same flows when the value passes through a local variable or through `CoroutineContext.plus` combinations, including chained combinations.
3. Reports `produce` and `actor` calls under the same conditions.
4. Handles both plain and `$default` compiled forms of the factories and builders.
5. Does not report builder calls whose context contains no tracked job, including the default `EmptyCoroutineContext` form.
6. Does not report `CoroutineScope(...)` construction from a context containing a job.
7. Does not report jobs that are created but never reach a builder or `withContext` context argument.
8. Reports nothing when kotlinx.coroutines is absent from the scan.
9. Emits at most one finding per offending call site, and in a method with multiple builder calls reports only the offending ones.
10. Covers TP, TN, and edge cases in tests.
11. Produces deterministic finding order and count across repeated runs.
12. Keeps `@Suppress`-style suppression unsupported and does not add non-JSpecify annotation semantics.
