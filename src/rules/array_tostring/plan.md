# Plan: ARRAY_TOSTRING

## Problem

Calling `toString()` on a Java array produces output like `[Ljava.lang.String;@1a2b3c` — the JVM default identity-hash representation — instead of human-readable content. This is almost always a bug:

- `array.toString()` → unhelpful hash string
- String concatenation with an array (`"prefix" + array`) compiles to `StringBuilder.append(Object)` which calls `toString()` internally
- Logging frameworks receiving an array reference directly

Developers intend `Arrays.toString(array)` or `Arrays.deepToString(array)` in these cases.

## Detection Strategy

Reuse the same stack-simulation approach as `ARRAY_EQUALS`:

1. Track `ValueKind` on the operand stack to distinguish array-typed values from non-array values.
2. Detect the following patterns:
   - **Direct `toString()` call**: `INVOKEVIRTUAL` where receiver is an array and method is `toString()Ljava/lang/String;` (inherited from `Object`).
   - **`String.valueOf(Object)` call**: `INVOKESTATIC java/lang/String.valueOf(Ljava/lang/Object;)Ljava/lang/String;` where the argument is an array.
   - **`StringBuilder.append(Object)` call**: `INVOKEVIRTUAL java/lang/StringBuilder.append(Ljava/lang/Object;)Ljava/lang/StringBuilder;` where the argument is an array. This covers string concatenation compiled by javac.
   - **`PrintStream.println(Object)` / `PrintStream.print(Object)` call**: where the argument is an array.

3. Use the existing `StackMachine<ValueKind>`, `ArrayValueDomain`, and `SemanticsHooks` infrastructure from `array_equals` (or factor out shared parts).

## Scope

- Detect `toString()` on values known to be arrays at the bytecode level.
- Detect `String.valueOf(Object)` and `StringBuilder.append(Object)` with array arguments.
- Detect `PrintStream.print/println(Object)` with array arguments.
- Report a SARIF finding with the method location and line number.

## Non-goals

- Detecting array toString via reflection or other indirect paths.
- Tracking array-ness across method boundaries (inter-procedural).
- Supporting `@Suppress`/`@SuppressWarnings` suppression.
- Supporting annotations beyond JSpecify (per annotation policy).
- Detecting `Arrays.toString()` misuse (e.g., on multidimensional arrays where `deepToString` would be better) — that could be a separate rule.

## Determinism

- Stack simulation is deterministic (sequential bytecode walk).
- Findings keyed by (class, method, offset), producing stable SARIF output.
- No hash-map iteration order dependency.

## Test Strategy

- **TP**: Direct `array.toString()` call.
- **TP**: String concatenation with an array (`"prefix" + array`).
- **TP**: `String.valueOf(array)` call.
- **TP**: `System.out.println(array)` call.
- **TN**: `Arrays.toString(array)` — correct usage, no finding.
- **TN**: `toString()` on a non-array object.
- **TN**: `String.valueOf()` with a non-array argument.
- **Edge**: Multidimensional array `toString()` — should still flag.
- **Edge**: Array loaded from a field (unknown provenance) — should not flag (precision over recall).

## Complexity

- O(N) per method in bytecode length — same as `ARRAY_EQUALS`.
- No inter-procedural analysis required.
- Reuses existing stack-machine infrastructure with minimal new code.

## Risks

- [ ] Stack desync on complex control flow may cause false negatives (acceptable: precision over recall).
- [ ] `StringBuilder.append(Object)` pattern may vary across Java compiler versions — verify with Java 21 javac output.
- [ ] Shared `ValueKind` enum extraction from `array_equals` may require refactoring — evaluate whether to duplicate or extract.
- [ ] Kotlin or other JVM languages may compile string templates differently — out of scope for initial implementation.
