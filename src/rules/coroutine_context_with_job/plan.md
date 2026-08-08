# Plan: coroutine_context_with_job

## Rule idea
Detect calls to kotlinx.coroutines coroutine builders (`launch`, `async`, `produce`, `actor`) and `withContext` that pass a `CoroutineContext` argument containing a `Job` element created by `Job()` or `SupervisorJob()`. Passing such a job into a builder context breaks the parent-child relationship of structured concurrency. Modeled after the IntelliJ inspection `CoroutineContextWithJob`.

## Problem description
Coroutine builders inherit the parent job from the receiver scope. When the caller passes a context that contains its own `Job` element, for example `scope.launch(Job()) { ... }` or `withContext(SupervisorJob()) { ... }`, the new coroutine is no longer a child of the scope's job. Cancellation of the scope will not cancel the coroutine, failures will not propagate to the parent, and `join`/`cancel` semantics silently change. The code compiles and often appears to work, so the defect survives review easily.

Analysis operates on JVM bytecode from class files and JARs. Detection must therefore be based on observable bytecode patterns, not Kotlin source syntax.

## Detection strategy

Intra-procedural bytecode dataflow within a single method, reusing the shared `src/dataflow` stack machine:

1. Gate the rule on kotlinx-coroutines being visible to the scan, similar to the `has_koin` gate used by `koin_autocloseable_not_closed`, so the rule is a cheap no-op for non-coroutines codebases.
2. Scan each analysis target method and mark values produced by the job factory calls as job-tainted:
   - `kotlinx/coroutines/JobKt.Job(...)` and `JobKt.Job$default(...)` (returns `CompletableJob`)
   - `kotlinx/coroutines/SupervisorKt.SupervisorJob(...)` and `SupervisorJob$default(...)`
   Taint applies regardless of whether a parent job argument is given to the factory.
3. Propagate taint through:
   - local variable stores and loads (stack machine locals)
   - `kotlin/coroutines/CoroutineContext.plus(CoroutineContext)` calls, in both directions (tainted receiver or tainted argument taints the result), including chained `plus` calls such as `Dispatchers.IO + Job() + CoroutineName("x")`
4. Report when a job-tainted value reaches the `CoroutineContext` parameter of a known builder call:
   - `kotlinx/coroutines/BuildersKt.launch` / `launch$default` (context is the argument after the `CoroutineScope` receiver)
   - `kotlinx/coroutines/BuildersKt.async` / `async$default`
   - `kotlinx/coroutines/BuildersKt.withContext` (context is the first argument)
   - `kotlinx/coroutines/channels/ProduceKt.produce` / `produce$default`
   - `kotlinx/coroutines/channels/ActorKt.actor` / `actor$default`
   The context parameter index is derived from each builder's known descriptor shape, including the `$default` variants with trailing mask arguments.
5. Do NOT flag job-tainted values that flow into `kotlinx/coroutines/CoroutineScopeKt.CoroutineScope(...)`. Creating a root scope with its own job is the intended use of these factories and must stay a true negative.
6. Emit one finding per offending builder call site with a message naming the builder and explaining that the context contains a Job element that breaks structured concurrency.

## Scope

**In scope:**
- Kotlin-compiled JVM bytecode calling the kotlinx.coroutines builder facades listed above.
- Job elements created by `Job()` or `SupervisorJob()` in the same method as the builder call, passed directly, via a local variable, or combined through `CoroutineContext.plus`.
- Both plain and `$default` compiled forms of the factories and builders.

**Non-goals:**
- Inter-procedural tracking. Jobs created in another method, received as a parameter, or read from a field are not tracked in v1.
- Type-based flagging of arbitrary `Job`-typed values that were not produced by the tracked factories in the same method.
- The `promise` builder. It exists only on Kotlin/JS and never appears in JVM bytecode.
- `future` (kotlinx-coroutines-jdk8), reactive builders (rxjava, reactor), and `runBlocking`.
- Jobs obtained from `coroutineContext[Job]` or from a scope's existing job.
- Detecting the pattern through helper functions that wrap the builders.
- Annotation-based suppression. `@Suppress` and `@SuppressWarnings` have no effect on this rule.
- Annotation-driven semantics beyond JSpecify. Only JSpecify is in scope for annotation-driven behavior, and this rule needs none of it.

## Determinism constraints
- Iterate classes, methods, and instructions in stable bytecode order.
- Use the shared stack machine with its symbolic-identity canonicalization so taint identifiers do not depend on hash order.
- Sort findings by `(class, method, descriptor, builder call instruction offset)` before emitting.
- Emit at most one finding per builder call site.

## Test strategy
Tests compile Kotlin sources through the existing test harness (`Language::Kotlin` in `src/test_harness.rs`) together with Kotlin stub sources for kotlinx.coroutines, following the stub pattern used by `koin_autocloseable_not_closed`. Stubs must reproduce the real binary names, so the `Job` and `SupervisorJob` stubs need `@file:JvmName("JobKt")` and `@file:JvmName("SupervisorKt")` file annotations, and builder stubs need `@file:JvmName("BuildersKt")` and the channels equivalents.

- TP: `scope.launch(Job()) { }`.
- TP: `scope.async(SupervisorJob()) { }`.
- TP: `withContext(Job()) { }` inside a suspend function.
- TP: `scope.launch(Dispatchers.IO + Job()) { }` through `CoroutineContext.plus`.
- TP: factory result stored in a local first, `val job = Job()` then `scope.launch(job) { }`.
- TN: `scope.launch(Dispatchers.IO) { }` with no job element.
- TN: `scope.launch { }` compiling to the `$default` variant with `EmptyCoroutineContext`.
- TN: `CoroutineScope(Job() + Dispatchers.IO)` root scope construction.
- TN: a job created and used only for `cancel()` or `join()`, never passed to a builder.
- Edge: two builder calls in one method where only one receives a job.
- Edge: `produce` and `actor` variants, at least one TP each.
- Gate: rule is skipped when kotlinx-coroutines is absent from the scan.

## Complexity
- Per method, a single linear pass with the stack machine is `O(I)` in instruction count.
- Local slot count and stack depth are bounded by the method itself, and the shared stack machine caps symbolic identities.
- No CFG exploration beyond what the shared dataflow helpers already provide, and no inter-procedural analysis.

## Risks
- [ ] Stub bytecode must produce the exact owner, name, and descriptor of real kotlinx-coroutines call sites, including multifile facade names such as `JobKt` and suspend function `Continuation` parameters. Mitigation: verify stub-compiled call sites against real kotlinx-coroutines compiled output during implementation, and drive matching from descriptors observed there.
- [ ] `kotlinc` availability in CI and locally (KOTLIN_HOME or PATH) is required for these tests. The harness already errors clearly when missing, but confirm CI provisions kotlinc before relying on it.
- [ ] Kotlin compiler versions may change how `plus` chains and `$default` bridges compile, causing false negatives. Mitigation: keep the matcher descriptor-driven and cover `$default` forms in tests.
- [ ] False negatives from the intra-procedural limit (jobs from fields, parameters, or helper methods). Accepted for v1 and documented as non-goals in `spec.md`.
- [ ] False positives if a tracked factory result is combined into a context that is only used for legitimate scope construction. Mitigation: taint flows are checked at the builder call argument, and `CoroutineScope(...)` sinks are explicitly not reported.
- [ ] Builder facades differ across kotlinx-coroutines versions (for example channels package history for `actor`). Mitigation: pin the supported owner names in `spec.md` and add version notes if e2e targets reveal drift.
