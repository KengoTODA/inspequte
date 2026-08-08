# Plan: coroutines_long_millis_call

## Rule idea
Report calls to kotlinx.coroutines time-based functions that take a `Long` milliseconds/timeout parameter (`delay`, `withTimeout`, `withTimeoutOrNull`, and the flow operators `debounce`, `sample`) when Duration-based overloads are available on the classpath, and suggest using the `kotlin.time.Duration` overloads instead. Inspired by the IntelliJ IDEA 2025.3 Kotlin inspection "ConvertLongToDuration".

## Problem description
kotlinx.coroutines offers two families of time-based APIs. The older family takes a raw `Long` interpreted as milliseconds. The newer family takes `kotlin.time.Duration`, which carries its unit in the type. Raw `Long` arguments are easy to misread and easy to pass in the wrong unit, for example seconds or nanoseconds obtained from another API. The Duration overloads make the unit explicit at the call site, such as `delay(500.milliseconds)`.

The Duration overloads only exist on sufficiently new kotlinx-coroutines versions. The rule must therefore confirm the overload actually exists on the analysis classpath before suggesting it, otherwise the suggestion is not actionable.

## Detection strategy

Bytecode-level exact match on call sites, plus a classpath availability gate.

### Call-site matching
Scan all call sites in analysis target classes and report a finding for each call that exactly matches one of these `(owner, name, descriptor)` triples:

| Owner | Name | Descriptor |
|-------|------|------------|
| `kotlinx/coroutines/DelayKt` | `delay` | `(JLkotlin/coroutines/Continuation;)Ljava/lang/Object;` |
| `kotlinx/coroutines/TimeoutKt` | `withTimeout` | `(JLkotlin/jvm/functions/Function2;Lkotlin/coroutines/Continuation;)Ljava/lang/Object;` |
| `kotlinx/coroutines/TimeoutKt` | `withTimeoutOrNull` | `(JLkotlin/jvm/functions/Function2;Lkotlin/coroutines/Continuation;)Ljava/lang/Object;` |
| `kotlinx/coroutines/flow/FlowKt` | `debounce` | `(Lkotlinx/coroutines/flow/Flow;J)Lkotlinx/coroutines/flow/Flow;` |
| `kotlinx/coroutines/flow/FlowKt` | `sample` | `(Lkotlinx/coroutines/flow/Flow;J)Lkotlinx/coroutines/flow/Flow;` |

These descriptors were verified against the kotlinx-coroutines JVM API. `kotlin.time.Duration` is a value class whose JVM representation is `J`, so the Duration overloads compile to name-mangled methods (for example `delay-VtjQ1oo` on `kotlinx/coroutines/DelayKt`) with descriptors identical to the Long variants. Matching the plain (unmangled) name exactly therefore never matches a Duration call site; the mangled dash suffix is the only distinguishing feature, and exact plain-name matching excludes it by construction.

`kotlinx/coroutines/flow/FlowKt` is a `@JvmMultifileClass` facade in the real library. Kotlin-compiled call sites reference the facade, and the facade class file declares delegating static methods, so both the call-site match and the availability check below work against `FlowKt`.

### Availability gate
Only report a finding when the Duration-based overload is confirmed to exist on the classpath:

1. Resolve the owner class (`DelayKt`, `TimeoutKt`, or `FlowKt`) via `AnalysisContext::all_classes()`, which exposes analysis target classes plus dependency classes scanned from `--classpath` artifacts. Dependency classes carry full `Method` lists with `name` and `descriptor` (precedent: `koin_autocloseable_not_closed` builds a `BTreeMap<&str, &Class>` index over `all_classes()` and resolves classpath supertypes from it).
2. Check that the resolved owner class declares a mangled counterpart of the called function: a method whose name starts with the plain name plus a dash (for example starts with `delay-`, `withTimeout-`, `withTimeoutOrNull-`, `debounce-`, `sample-`). Match on the `name-` prefix, never on an exact mangled name, because the mangling suffix is a compiler-computed hash that can drift across Kotlin versions and differs for test stubs. Note the prefix includes the dash, so `withTimeout-` does not accidentally match `withTimeoutOrNull-...`.
3. If the owner class cannot be resolved from the classpath at all, do NOT report. This is conservative and avoids false positives on old kotlinx-coroutines versions that lack Duration overloads, and on builds where coroutines classes were not passed on the classpath.

Recommended tightening for the spec phase: additionally require the mangled counterpart's descriptor to equal the Long variant's descriptor. Because `Duration` erases to `J`, this equality holds for all five targets in the real library and costs nothing, while excluding hypothetical unrelated mangled overloads. The prefix check remains the primary condition either way.

No new engine-level framework flag (like `has_koin`) is needed. The availability gate itself is the cheap early exit: if none of the three owner classes resolve with a mangled counterpart, the rule reports nothing, and the per-call matching is a constant-time triple comparison.

### Finding shape
- One finding per matching call site.
- Location: enclosing class/method plus source line resolved from the call-site offset via `method_location_with_line` (same as existing rules).
- Message: name the called function and suggest the `kotlin.time.Duration` overload, for example "delay(timeMillis) takes a raw Long in milliseconds; the kotlin.time.Duration overload is available, use delay(500.milliseconds) style instead". Exact wording is fixed in `spec.md`.

## Scope

**In scope:**
- Exact-match call sites of the five triples above, in analysis target classes only.
- Availability gate against the analysis classpath as described.
- Kotlin- and Java-compiled call sites alike; the rule does not attempt source-language discrimination (see Non-goals and Risks).

**Non-goals:**
- Other millis-based APIs: `Thread.sleep`, `Object.wait`, `kotlinx.coroutines.time.*` (java.time interop), `onTimeout` select clauses, `delay` on `Delay` implementations, `debounce`/`sample` selector-function overloads, or any API not in the table.
- Argument-value analysis. No special-casing of constants such as `0L`, no unit inference, no data-flow on where the Long came from.
- Suggesting a concrete Duration expression rewrite. The message is advisory; no fix generation.
- Reporting call sites inside dependency (classpath-only) classes.
- Version detection beyond the mangled-counterpart existence check. No artifact-name or manifest parsing.
- Annotation-based suppression: `@Suppress` / `@SuppressWarnings` semantics are not supported.
- Annotation-driven semantics beyond JSpecify are out of scope; this rule uses no annotation semantics at all.

## Determinism constraints
- Iterate analysis target classes, methods, and call sites in stable scan order.
- Build the owner-class lookup from `all_classes()` into a `BTreeMap` (or perform a single deterministic pass); never depend on hash-map iteration order.
- Sort findings by `(class name, method name, method descriptor, call-site offset)` before emitting.
- Output depends only on the scanned class files; no environment or timing sensitivity.

## Test strategy
Use the existing kotlinc harness (`src/test_harness.rs`, `Language::Kotlin`) with stub Kotlin sources for the kotlinx.coroutines API compiled in-test, following the `koin_autocloseable_not_closed` `koin_stub_sources` precedent. Stubs pin JVM class names with `@file:JvmName` (one stub file per owner class, since `@JvmName` cannot be shared across files without multifile-class plumbing):

- `DelayKt` stub: `suspend fun delay(timeMillis: Long)` plus `suspend fun delay(duration: kotlin.time.Duration)`.
- `TimeoutKt` stub: `suspend fun <T> withTimeout(timeMillis: Long, block: suspend CoroutineScope.() -> T): T` and the `OrNull` variant, each with a Duration counterpart; plus a minimal `CoroutineScope` stub.
- `FlowKt` stub: `fun <T> Flow<T>.debounce(timeoutMillis: Long): Flow<T>` and `sample`, each with a Duration counterpart; plus a minimal `Flow` interface stub.

`kotlin.time.Duration` comes from the stdlib that kotlinc provides. The mangling suffix of stub-compiled Duration overloads may differ from the real library's; this is fine because the availability check matches on the `name-` prefix, not an exact mangled name.

Compile stubs into a separate classes dir and pass it as the harness classpath so they are dependency classes, mirroring real usage. Where the availability gate must be varied between compile and analyze, call `harness.compile(...)` and `harness.analyze(...)` separately with different classpaths.

Planned cases:
- TP: suspend function calling `delay(500)`; expect one finding naming `delay`.
- TP: `withTimeout(1_000) { ... }` and `withTimeoutOrNull(1_000) { ... }`; one finding each.
- TP: `flow.debounce(200)` and `flow.sample(200)`; one finding each.
- TN: Duration-overload call sites (`delay(500.milliseconds)`, `debounce(200.milliseconds)`, ...) produce no findings (mangled names do not match).
- TN (availability, old library): stubs compiled WITHOUT Duration overloads; Long call sites present, expect no findings.
- TN (availability, unresolvable owner): compile target against stubs, then analyze with an empty classpath; expect no findings.
- TN: an unrelated user-defined `delay(Long)` in a different owner class is not reported.
- Edge: multiple matching calls in one method report deterministically ordered findings, one per call site.
- Edge: call site in a classpath-only (dependency) class is not reported.

## Complexity
- Call-site scan is linear in total instructions/call sites of analysis target classes: `O(I)` with a constant-time set membership per call.
- Availability gate is at most three class lookups plus a linear scan of each owner's declared methods: bounded by owner method count, computed once per run.
- No CFG traversal, no data-flow, no inter-procedural analysis.

## Risks
- [ ] Old kotlinx-coroutines versions without Duration overloads. Mitigation: availability gate requires a mangled counterpart on the resolved owner class; unresolvable owner means no findings.
- [ ] Mangled suffixes drift across Kotlin/library versions and differ in test stubs. Mitigation: prefix match on `name-`, never exact mangled names.
- [ ] Prefix check could match an unrelated future mangled overload and mis-report availability. Mitigation: low likelihood; spec phase may adopt the descriptor-equality tightening (Duration erases to `J`, so the counterpart descriptor equals the Long variant's).
- [ ] `FlowKt` facade shape could differ in exotic builds (shading, relocation). Mitigation: exact owner match is the documented contract; relocated coroutines are out of scope and silently not reported due to the availability gate.
- [ ] Java call sites (raw `Continuation` interop) would be reported although the Duration overload is awkward from Java. Accepted noise: such call sites are vanishingly rare and the rule performs no source-language discrimination; document in `spec.md`.
- [ ] Intentional Long-API usage (for example values already held as millis from config). Accepted: the rule is advisory; message must stay actionable and name the exact function.
- [ ] Determinism regressions from unordered iteration. Mitigation: BTreeMap index and explicit `(class, method, descriptor, offset)` sort, matching the koin rule precedent.

## Post-mortem
- Went well: value-class name mangling made the Long/Duration distinction unambiguous at bytecode level, so exact (owner, name, descriptor) matching plus a mangled-counterpart availability gate needed no CFG or dataflow.
- Tricky: the kotlinc test-harness stubs get compiler-generated mangled suffixes that differ from the real library, and the classpath TempDir was dropped too early in one test iteration; both were resolved by prefix-based matching and keeping the CompileOutput alive.
- Follow-up: consider covering kotlinx.coroutines.time (java.time based) variants and selector-function debounce/sample overloads in a future rule revision.
