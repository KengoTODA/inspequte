# Rule Plan: run_blocking_in_suspend_function

## Summary
Detect `kotlinx.coroutines.runBlocking` calls made inside Kotlin `suspend` functions.

## Problem framing
`runBlocking` blocks the calling thread until its coroutine completes. Inside a `suspend` function the caller is already in a coroutine, so blocking the thread defeats structured concurrency, wastes the thread, and can deadlock when the blocked thread is needed to resume the coroutine. The fix is to call the code directly, use `run`, or use `withContext` when a specific `CoroutineContext` is needed. This mirrors the IntelliJ inspection `RunBlockingInSuspendFunction` (new in 2025.1).

## Bytecode facts grounding the design
- A Kotlin `suspend` function compiles to a JVM method whose descriptor has a final parameter of type `Lkotlin/coroutines/Continuation;` and a return type of `Ljava/lang/Object;`.
- `runBlocking { ... }` compiles to a static call to `kotlinx/coroutines/BuildersKt.runBlocking(Lkotlin/coroutines/CoroutineContext;Lkotlin/jvm/functions/Function2;)Ljava/lang/Object;`. When the context argument is omitted, the compiler emits the synthetic bridge `runBlocking$default(Lkotlin/coroutines/CoroutineContext;Lkotlin/jvm/functions/Function2;ILjava/lang/Object;)Ljava/lang/Object;`.
- The body of a named `suspend` function is compiled in place into that method (its state machine uses a synthetic `ContinuationImpl` subclass for state, but call sites in the function body stay in the named method). Suspend lambdas instead compile to `invokeSuspend` methods of synthetic classes extending `kotlin/coroutines/jvm/internal/SuspendLambda` or `ContinuationImpl`.

## Scope
- Analyze call sites in analysis target classes only.
- Match static calls where owner is `kotlinx/coroutines/BuildersKt` and name/descriptor is:
  - `runBlocking(Lkotlin/coroutines/CoroutineContext;Lkotlin/jvm/functions/Function2;)Ljava/lang/Object;`
  - `runBlocking$default(Lkotlin/coroutines/CoroutineContext;Lkotlin/jvm/functions/Function2;ILjava/lang/Object;)Ljava/lang/Object;`
- Report only when the enclosing method looks like a compiled `suspend` function:
  - final parameter type is `Lkotlin/coroutines/Continuation;`, and
  - return type is `Ljava/lang/Object;`.
- Emit one finding per matching call site with class/method context and source line when available.

## Suspend-lambda scope decision (explicit)
`invokeSuspend` bodies of synthetic `SuspendLambda` / `ContinuationImpl` subclasses are **out of scope for v1**. Rationale: a suspend lambda may be the argument of any builder, including `runBlocking` itself at a legitimate blocking entry point (`main`, tests). Attributing a synthetic lambda class back to a suspend context would need extra class-hierarchy and enclosing-method reasoning with real false-positive risk. v1 stays precise by matching named suspend methods only. Revisit as a follow-up once we can attribute lambda classes to their creating context deterministically.

## Non-goals
- `runBlocking` calls in non-suspend methods (legitimate bridge points such as `main`, tests, servlet handlers).
- `runBlocking` inside suspend lambdas / `invokeSuspend` state-machine bodies of synthetic classes (see decision above).
- Other blocking APIs inside suspend functions (`Thread.sleep`, `Future.get`, blocking IO); those are covered by other rules or future work.
- Inter-procedural reasoning (a suspend function calling a non-suspend helper that calls `runBlocking`).
- Deadlock proof or dispatcher analysis; the call itself is the finding.
- Suppression via `@Suppress` / `@SuppressWarnings` is not supported.
- Non-JSpecify annotation semantics are not supported; no annotation-driven semantics are needed by this rule.

## Framework gate (engine change required on this branch)
This branch has `has_slf4j`, `has_log4j2`, and `has_koin` gates in `src/engine.rs` (`detect_known_frameworks`) but no kotlinx.coroutines gate. Add `has_kotlinx_coroutines` mirroring `has_koin`:
- descriptor check: field/method descriptor contains `Lkotlinx/coroutines/`;
- reference check: referenced class, super name, or interface starts with `kotlinx/coroutines/`;
- telemetry attribute `inspequte.kotlinx_coroutines.present`;
- expose it via the analysis context and skip this rule entirely when the framework is absent.
This keeps the rule zero-cost for non-coroutines codebases and cuts noise from unrelated `runBlocking`-named methods (exact owner matching already prevents that, but the gate also avoids scanning).

## Detection strategy
1. If `has_kotlinx_coroutines` is false, return no findings.
2. Iterate analysis target classes and methods in stable order.
3. Skip methods that do not match the compiled-suspend shape (final param `Lkotlin/coroutines/Continuation;`, return `Ljava/lang/Object;`).
4. Within matching methods, scan instructions for static calls to the two `BuildersKt.runBlocking` signatures above.
5. Resolve the source line from the bytecode offset when available.
6. Emit findings in traversal order, keyed by `(class, method, descriptor, call-site offset)`.

No dataflow, CFG, or inter-procedural analysis is needed; this is a call-site plus enclosing-method-shape match, structurally close to `thread_sleep_call`.

## Rule message
- Problem: `runBlocking` inside a suspend function blocks the calling thread and defeats asynchronous execution; it can deadlock.
- Fix: call the suspending code directly, use `run { ... }`, or use `withContext(context) { ... }` when a specific `CoroutineContext` is needed.

## Test strategy
Use the Kotlin test harness (`Language::Kotlin`) with kotlinx stubs via `@file:JvmName` facades, following the pattern in `src/rules/koin_autocloseable_not_closed`: a stub file with `@file:JvmName("BuildersKt")` in package `kotlinx.coroutines` declaring `runBlocking` (context parameter with default value so both `runBlocking` and `runBlocking$default` shapes are produced), plus minimal `CoroutineScope` and `withContext` stubs as needed. Generic class names per harness guidelines.
- TP: `suspend fun` calling `runBlocking { ... }` without a context argument (matches `runBlocking$default`).
- TP: `suspend fun` calling `runBlocking(context) { ... }` with an explicit context (matches the full signature).
- TN: non-suspend function (e.g. an entry-point-style method) calling `runBlocking { ... }`.
- TN: `suspend fun` using `withContext(...) { ... }` or plain suspending calls, no `runBlocking`.
- TN (documented non-goal): `runBlocking` inside a suspend lambda passed to a builder; the synthetic `invokeSuspend` body is not reported.
- Edge: two `runBlocking` calls in one suspend function produce two ordered findings.
- Gate: input without kotlinx.coroutines references produces no findings and no scanning work.

## Complexity and determinism
- Linear in the number of methods plus matched-method instructions, `O(M + I)`; the suspend-shape check is a constant-time descriptor inspection per method.
- Deterministic by stable class/method/instruction iteration and sorted emission by `(class, method, descriptor, offset)`.
- No environment- or timing-dependent behavior.

## Annotation policy
- No `@Suppress` / `@SuppressWarnings` suppression semantics.
- Annotation-driven semantics are JSpecify-only project-wide; this rule uses none.
- Non-JSpecify annotations must not change behavior.

## Risks
- [ ] Non-Kotlin code (or hand-written Java) with a trailing `Continuation` parameter could be misclassified as a suspend function. Mitigation: also require return type `Ljava/lang/Object;`; residual risk is tiny and accepted.
- [ ] False negatives for `runBlocking` inside suspend lambdas (documented v1 non-goal). Mitigation: state clearly in `spec.md`; plan follow-up.
- [ ] kotlinx.coroutines version drift could change the `runBlocking$default` bridge shape. Mitigation: match both listed signatures exactly and cover both in tests.
- [ ] Kotlin stubs in the harness must compile to the exact `BuildersKt` facade signatures. Mitigation: assert the expected call shape in fixtures; reuse the proven `@file:JvmName` facade pattern.
- [ ] Engine gate change touches shared `detect_known_frameworks` (tuple return grows). Mitigation: mirror the `has_koin` wiring exactly and extend existing engine tests.

## Post-mortem
- What went well: the no-dataflow call-site plus method-shape design kept the rule small, and verify returned Go on the first iteration with all acceptance criteria backed by tests.
- What was tricky: the plan initially recorded the runBlocking context parameter as kotlinx/coroutines/CoroutineContext; the spec phase caught that the correct descriptor is kotlin/coroutines/CoroutineContext, and stub fixtures were validated against real kotlinc output.
- Follow-ups: consider attributing invokeSuspend bodies of SuspendLambda subclasses to their creating context so runBlocking inside suspend lambdas can be reported, and unify the has_kotlinx_coroutines gate when PR #289 merges.
