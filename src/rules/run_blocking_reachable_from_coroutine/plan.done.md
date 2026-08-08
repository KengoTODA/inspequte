# Plan: run_blocking_reachable_from_coroutine

## Rule idea
Port of the IntelliJ inspection "RunBlocking" (new in 2025.1). Report `runBlocking`
builder calls that can be reached from a coroutine through a chain of plain
function calls. Include an approximate call stack from the coroutine primitive
to the `runBlocking` call in the finding message.

## Problem description
Coroutines share threads for execution. When `runBlocking` is called from code
that runs inside a coroutine, it blocks the underlying thread and prevents other
coroutines from using it. This can cause performance degradation and, in bad
cases, thread starvation or deadlock. The dangerous pattern is often indirect. A
suspend function calls a plain helper function, and the helper calls
`runBlocking`. No single method looks wrong in isolation, so only an
interprocedural check can find it.

IntelliJ example that must be reported:

```kotlin
suspend fun main() { foo() }

fun foo() {
    runBlocking { suspendFunction() }
}
```

## Relation to RUN_BLOCKING_IN_SUSPEND_FUNCTION
A sibling rule `RUN_BLOCKING_IN_SUSPEND_FUNCTION` (open PR #290, not yet in
main) reports `runBlocking` called directly inside a compiled suspend function.
IntelliJ ships both inspections side by side. This rule takes the exact
complement of the sibling's detection surface to avoid double reporting and
gaps:

- The sibling owns `runBlocking` call sites whose enclosing method is a
  compiled suspend function. Detection heuristic for that surface is a trailing
  `Lkotlin/coroutines/Continuation;` parameter with `Ljava/lang/Object;` return
  type. This rule never reports those call sites, even when they are also
  reachable through a longer chain.
- This rule owns `runBlocking` call sites in every other method that is
  reachable from a coroutine primitive. That includes plain functions reached
  transitively and `invokeSuspend` bodies of suspend lambdas (for example
  `launch { runBlocking { } }`), because `invokeSuspend` has descriptor
  `(Ljava/lang/Object;)Ljava/lang/Object;` and does not match the sibling's
  suspend-function heuristic.

This boundary is stable even if PR #290 lands before or after this rule. Both
rules stay pure and independent per `src/rules/AGENTS.md`.

## Detection strategy

Bytecode-level, rule-local interprocedural reachability. All graph construction
happens inside the rule's `run` method from existing IR. No shared analysis
artifact is added.

### Framework gate
Skip the whole rule unless kotlinx.coroutines is present in the input. The
current branch has no `has_kotlinx_coroutines` flag in
`detect_known_frameworks` (`src/engine.rs` detects only slf4j, log4j2, and
koin). Add `has_kotlinx_coroutines`, keyed off method and field descriptors and
`referenced_classes` containing `kotlinx/coroutines`, mirroring the shape used
by PR #290 so the eventual merge is mechanical.

### Sinks
`runBlocking` call sites in analysis target classes:

- `INVOKESTATIC kotlinx/coroutines/BuildersKt.runBlocking(Lkotlin/coroutines/CoroutineContext;Lkotlin/jvm/functions/Function2;)Ljava/lang/Object;`
- the `runBlocking$default` bridge for the no-context overload

Both are found via `Method.calls` (`CallSite` with owner `kotlinx/coroutines/BuildersKt`
and name `runBlocking` or `runBlocking$default`).

### Roots (coroutine primitives)
A method is a root when either:

1. It is a compiled suspend function. Heuristic: trailing parameter
   `Lkotlin/coroutines/Continuation;` and return type `Ljava/lang/Object;`.
   This covers `suspend fun main` and every named suspend function, including
   their `$default` bridges.
2. It is the `invokeSuspend` method of a class whose superclass is
   `kotlin/coroutines/jvm/internal/SuspendLambda`. Suspend lambdas passed to
   `launch`, `async`, `withContext`, `produce`, `actor`, and `runBlocking`
   itself all compile to `SuspendLambda` subclasses holding the body in
   `invokeSuspend` (verified, not invokedynamic). This root model subsumes an
   explicit list of builder call sites, so no builder-name allowlist is needed
   for root detection.

`RestrictedSuspendLambda` subclasses (bodies of `sequence { }` and other
restricted-suspension builders) are deliberately not roots. Those bodies run
synchronously on the caller thread without a dispatcher, so `runBlocking` there
is not a coroutine thread-sharing problem.

Roots are collected from analysis target classes only.

### Call graph and reachability
1. Build a method index from analysis target classes keyed by
   `(owner, name, descriptor)` in a `BTreeMap`.
2. For each indexed method, edges are its `Method.calls` entries resolved
   against the index by exact `(owner, name, descriptor)`. When the exact owner
   has no such method, walk the `super_name` chain within the index and resolve
   to the first inherited concrete method. No expansion to overriding
   implementations in subclasses (see virtual dispatch stance below).
3. Run one BFS from the full root set over these edges, recording a
   deterministic parent pointer for each discovered method (see determinism
   constraints). This yields, for every reachable method, one shortest chain
   back to a root.
4. Report every sink call site whose enclosing method is reachable (roots are
   reachable at distance zero), except call sites whose enclosing method
   matches the sibling rule's suspend-function heuristic.

### Virtual dispatch stance
IntelliJ offers an "explore functions with overrides" option for this
inspection. For a bytecode analyzer the equivalent is CHA-style expansion of
virtual and interface calls to all overriding implementations. This rule
simplifies that option to off, permanently. Call edges resolve only to the
statically named target, with a superclass-chain fallback for inherited
concrete methods. Rationale: precision over recall per `src/rules/AGENTS.md`.
CHA expansion would claim reachability through overrides that are never
dispatched from coroutine context and would be the main false-positive source.
The cost is a known false negative for interface calls with a single
implementation. Document this in spec non-goals.

### Lambda and thread boundaries
Plain SAM lambdas (`Runnable`, executor tasks) compile to invokedynamic in
Kotlin 2.x. The rule does not follow invokedynamic `impl_method` edges and does
not follow `Thread`, executor, or callback boundaries. A `Thread { runBlocking { } }`
created inside a suspend function runs on its own thread where blocking is
acceptable, so following those edges would produce false positives. Suspend
lambda bodies do not need edge modeling at all because they are roots
themselves.

### Finding message and location
Location is the `runBlocking` call site (class, method, line via
`line_for_offset`). One finding per sink call site, not per (root, sink) pair.
The message embeds the approximate call stack from the chosen root to the sink,
for example:

```
runBlocking is reachable from a coroutine and blocks a shared coroutine thread. Potential call stack: com.example.MainKt.main -> com.example.UtilKt.foo -> runBlocking
```

Rendering rules:

- dotted class names, `ClassName.methodName` per frame, no descriptors
- ` -> ` separator, terminated by the literal `runBlocking`
- chain capped at 10 frames, longer chains elide the middle with `...`
- synthetic lambda frames render their JVM name as is (for example
  `MainKt$main$1.invokeSuspend`), which is stable for a fixed kotlinc version

## Scope

**In scope:**
- Kotlin-compiled analysis target classes when kotlinx.coroutines is detected.
- `runBlocking` and `runBlocking$default` call sites on
  `kotlinx/coroutines/BuildersKt`.
- Reachability through static, special, virtual, and interface calls resolved
  to their static target within analysis target classes.
- Roots from compiled suspend functions and `SuspendLambda.invokeSuspend`
  bodies.

**Non-goals:**
- The direct-in-suspend-function case owned by `RUN_BLOCKING_IN_SUSPEND_FUNCTION`.
- CHA or points-to expansion of virtual calls to overriding implementations.
- Edges through plain (non-suspend) lambdas, `Thread`, executors, or other
  callback registrations.
- Traversal through dependency classes. Roots, edges, and sinks all live in
  analysis target classes in v1.
- Dispatcher awareness. A chain guarded by "only runs on Dispatchers.IO"
  runtime logic is still reported when structurally reachable.
- `RestrictedSuspendLambda` bodies as roots.
- Annotation-based suppression. `@Suppress` and `@SuppressWarnings` are not
  supported per project policy.
- Annotation-driven semantics beyond JSpecify. No non-JSpecify annotation may
  change this rule's behavior, and this rule needs no JSpecify semantics.

## Determinism constraints
- All method and edge collections use `BTreeMap`/`BTreeSet` or sorted `Vec`,
  never hash-order iteration.
- BFS visits the root set in sorted `(owner, name, descriptor)` order and each
  adjacency list in sorted order, so the recorded parent pointer (and therefore
  the rendered chain) is the lexicographically smallest shortest path.
- Findings sort by `(class, method, descriptor, call-site offset)` before
  emission. The engine's final `(rule_id, message)` sort stays stable because
  each message embeds a deterministic chain.
- Same input twice produces identical findings, ordering, and messages.

## Complexity
- Method index build is `O(M)` over target-class methods.
- Edge resolution is `O(C log M)` over total call sites with `BTreeMap` lookup,
  plus a bounded superclass-chain walk per unresolved call.
- Single BFS is `O(M + C)`. No per-sink or per-root repeated traversal.
- Memory is one index entry per method and one parent pointer per reachable
  method. No CFG exploration and no dataflow. This satisfies the performance
  stability principle for large inputs.

## Infrastructure notes
- `AnalysisContext` provides no call graph today. The `call_graph_*` timing
  keys in `ContextTimings` are hardcoded zeros. This rule builds its graph
  locally and does not touch those keys or add shared context state.
- Engine change is limited to adding the `has_kotlinx_coroutines` gate to
  `detect_known_frameworks` plus an accessor, following the existing
  slf4j/log4j2/koin pattern.
- Rule registers via `register_rule!` from
  `src/rules/run_blocking_reachable_from_coroutine/mod.rs` and is
  auto-discovered.

## Test strategy
Tests use `JvmTestHarness` with `Language::Kotlin` (kotlinc 2.1.10 on PATH,
`JAVA_HOME` pointing at Java 21). kotlinx.coroutines is stubbed as facade
sources with `@file:JvmName("BuildersKt")` in package `kotlinx.coroutines`,
declaring `runBlocking` with a default `CoroutineContext` parameter so use
sites produce the real `BuildersKt.runBlocking` / `runBlocking$default`
INVOKESTATIC shapes, plus minimal `CoroutineScope` and `launch` declarations.
Generic class and method names per harness guidelines.

- TP: IntelliJ example. `suspend fun` calls plain `foo()`, `foo` calls
  `runBlocking`. Expect one finding whose message chain names both frames.
- TP: depth-two chain. suspend fun -> `a()` -> `b()` -> `runBlocking`.
- TP: builder lambda root. `launch { helper() }` where plain `helper()` calls
  `runBlocking`.
- TP: direct in builder lambda. `launch { runBlocking { } }` is reported by
  this rule because `invokeSuspend` is not a compiled suspend function.
- TN: `runBlocking` reachable only from a plain non-suspend `main`.
- TN: `runBlocking` directly inside a suspend function body. Excluded here as
  sibling-rule territory.
- TN: plain lambda boundary. Suspend function creates `Thread { runBlocking { } }`
  or a `Runnable`. Not followed, no finding.
- TN: Java-only input without kotlinx.coroutines. Gate keeps the rule silent.
- Edge: two roots reach the same sink. Exactly one finding with the
  deterministic shortest, lexicographically smallest chain.
- Edge: recursive call cycle on the path. BFS terminates, single finding.
- Edge: `runBlocking$default` bridge call shape is detected.
- Determinism: analyze the same compiled input twice and assert identical
  SARIF output.

## Risks
- [ ] Merge overlap with PR #290. Both branches touch `detect_known_frameworks`
  for the kotlinx gate. Mitigation: mirror the sibling's field naming and
  detection keying so conflict resolution is mechanical.
- [ ] Boundary drift with `RUN_BLOCKING_IN_SUSPEND_FUNCTION` if PR #290's final
  detection surface differs from the Continuation-parameter heuristic.
  Mitigation: state the exclusion heuristic explicitly in spec so both specs
  can be checked for complement coverage at merge time.
- [ ] False negatives from the no-override dispatch stance (interface call with
  one implementation). Accepted for precision, documented in spec non-goals.
- [ ] False positives from suspend lambdas that are created but never
  dispatched on a shared-thread dispatcher. Rare in practice. Accepted and
  documented.
- [ ] Synthetic frame names (`MainKt$main$1.invokeSuspend`) in messages may
  vary across kotlinc versions and could churn baselines. Mitigation: keep
  frame rendering minimal and cover it with harness tests pinned to the
  toolchain in CI.
- [ ] Stub fidelity. Facade stubs must keep producing the exact
  `BuildersKt.runBlocking` INVOKESTATIC shape of real kotlinx-coroutines.
  Mitigation: reuse the verified facade pattern and assert call-site shape in
  a harness test.
- [ ] Message-size blowup on very deep chains. Mitigation: 10-frame cap with
  middle elision.
- [ ] Performance on large inputs. Graph build is linear but adds one index
  over all target methods. Mitigation: gate on kotlinx detection so non-Kotlin
  projects pay nothing.

## Post-mortem

- The spec phase caught a factual error in this plan. Suspend function
  `$default` bridges append mask parameters after the Continuation, so the
  trailing-Continuation heuristic does not match them. They are documented as
  an accepted false negative instead of roots.
- The engine builds no call graph despite its `call_graph_*` telemetry keys,
  so the rule builds reachability locally. Verify passed on the first
  iteration with non-blocking findings only.
- Compiling the kotlinx stub facades to a classpath directory instead of the
  analysis target set kept the stubs' own `runBlocking$default` bridge from
  surfacing as a fixture-artifact finding.
