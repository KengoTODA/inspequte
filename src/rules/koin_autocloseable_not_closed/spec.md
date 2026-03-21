# KOIN_AUTOCLOSEABLE_NOT_CLOSED

## Summary
- Rule ID: `KOIN_AUTOCLOSEABLE_NOT_CLOSED`
- Name: Koin AutoCloseable not closed
- Description: Detects Koin singleton definitions that construct an `AutoCloseable` resource but do not close it via `onClose`.
- Annotation policy: `@Suppress`-style suppression is unsupported. Annotation-driven semantics support JSpecify only; this rule has no annotation-driven semantics.

## Motivation
Koin can keep singleton definitions alive until the Koin application stops. If a definition lambda constructs an `AutoCloseable` resource, that resource can leak file descriptors, sockets, threads, or native handles unless the definition also registers cleanup logic.

In Koin's DSL, the intended cleanup hook is `onClose`. Missing or ineffective `onClose` logic is easy to overlook because the construction and cleanup code are often declared in one short chain.

## What it detects
- Kotlin-compiled Koin module code in analysis target classes where:
  - a `single(...)` or `single$default(...)` call on `org/koin/core/module/Module` is present,
  - the referenced definition lambda constructs and returns a type that implements `java/lang/AutoCloseable` or `java/io/Closeable`, and
  - the same definition chain does not register `onClose(...)`, or its `onClose` callback does not call `close()`.
- The rule matches the direct classic DSL shape where the singleton definition and the `onClose` registration appear in the same enclosing method.

## What it does NOT detect
- `factory`, `scoped`, `singleOf`, `factoryOf`, `scopedOf`, or compiler-plugin DSL variants.
- Callback wiring styles that do not appear as the direct definition-chain pattern in the same enclosing method.
- Definitions that return a pre-existing resource obtained elsewhere instead of constructing one in the definition lambda.
- Cleanup performed indirectly through helper methods or APIs other than `close()`.
- Classpath-only classes that contain the DSL misuse but are not part of the analysis target.
- Any suppression behavior via `@Suppress` or `@SuppressWarnings`.

## Examples (TP/TN/Edge)

### TP: singleton creates an AutoCloseable and registers no cleanup (reported)
```kotlin
package com.example

import org.koin.dsl.module

class ClassA : AutoCloseable {
    override fun close() {}
}

val varOne = module {
    single { ClassA() }
}
```
Finding reported: the Koin singleton constructs `ClassA` but does not close it with `onClose`.

### TP: singleton has `onClose` but does not call `close()` (reported)
```kotlin
package com.example

import org.koin.core.module.dsl.onClose
import org.koin.dsl.module

class ClassA : AutoCloseable {
    override fun close() {}
}

val varOne = module {
    single { ClassA() } onClose { _ -> println("ignored") }
}
```
Finding reported: the cleanup callback exists but does not close the resource.

### TN: singleton closes the resource in `onClose` (not reported)
```kotlin
package com.example

import org.koin.core.module.dsl.onClose
import org.koin.dsl.module

class ClassA : AutoCloseable {
    override fun close() {}
}

val varOne = module {
    single { ClassA() } onClose { it?.close() }
}
```
No finding: the singleton definition registers cleanup that closes the resource.

### TN: singleton returns a non-AutoCloseable type (not reported)
```kotlin
package com.example

import org.koin.dsl.module

class ClassB

val varOne = module {
    single { ClassB() }
}
```
No finding: the created type does not implement `AutoCloseable` or `Closeable`.

### Edge: multiple definitions in one module method (report only the leaking one)
```kotlin
package com.example

import org.koin.core.module.dsl.onClose
import org.koin.dsl.module

class ClassA : AutoCloseable {
    override fun close() {}
}

class ClassB : AutoCloseable {
    override fun close() {}
}

val varOne = module {
    single { ClassA() }
    single { ClassB() } onClose { it?.close() }
}
```
Only the `ClassA` definition is reported.

### Edge: classpath-provided AutoCloseable type created in target module (reported)
If the target module constructs a dependency class that implements `AutoCloseable`, the rule reports it when the definition omits `onClose`, even if the resource class itself comes from the classpath.

## Output
- Report one finding per leaking Koin singleton definition chain.
- Message must be actionable:
  `Koin singleton in <class>.<method><descriptor> creates AutoCloseable resource <resource-type> but does not close it in onClose; add onClose { it?.close() } or manage the resource lifecycle outside Koin.`
- Location is reported at the enclosing method logical location and, where available, the source line of the singleton definition call.

## Performance considerations
- Analysis is bounded by the instruction count of Kotlin-compiled analysis target methods plus bounded type-hierarchy checks for candidate resource classes.
- No CFG traversal or inter-procedural analysis is required.
- Traversal order must be deterministic: classes in analysis-target order, methods in declaration order, instructions in bytecode offset order.

## Acceptance criteria
1. Reports when an analysis target method contains a Koin `single(...)` or `single$default(...)` definition whose lambda constructs and returns an `AutoCloseable`/`Closeable` type without a matching `onClose` callback.
2. Reports when the `onClose` callback exists for that definition chain but does not call `close()`.
3. Does not report when the callback calls `close()` on the resource.
4. Does not report when the definition constructs a non-`AutoCloseable` type.
5. Does not report classpath-only module code.
6. Supports resource types defined in analysis targets or classpath dependencies, as long as their type hierarchy shows `AutoCloseable`/`Closeable`.
7. Covers TP, TN, and edge cases in tests.
8. Produces deterministic finding order and count across repeated runs.
9. Keeps `@Suppress`-style suppression unsupported and does not add non-JSpecify annotation semantics.
