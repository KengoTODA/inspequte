# Plan: exception_cause_not_preserved

## Objective
Detect catch blocks that throw a new exception without preserving the original exception as a cause.

## Problem framing
When a catch block replaces the original exception with a new one but drops the cause, debugging becomes harder because the root stack trace is lost. This is easy to miss in review because the control flow is localized to catch handlers and the missing cause is a single constructor argument or `initCause` call.

## Scope
- Analyze each catch handler in method bytecode.
- Flag `athrow` of a newly constructed exception when the caught exception is not used as a cause.
- Accept cause preservation via constructor argument, `initCause(Throwable)`, or `addSuppressed(Throwable)` before the throw.

## Non-goals
- Inter-procedural inference (e.g., helper methods that wrap exceptions).
- Inferring cause preservation via custom fields or builder APIs.
- Tracking across stored fields or escapes beyond the catch handler.
- Semantics driven by non-JSpecify annotations.
- `@Suppress` / `@SuppressWarnings` based suppression behavior.

## Detection strategy
1. Identify catch handlers and their caught exception local index.
2. Within each handler, collect:
   - `athrow` sites and the value being thrown.
   - Uses of the caught exception as an argument to:
     - Exception constructors (`<init>` with a `Throwable` parameter)
     - `Throwable.initCause(Throwable)`
     - `Throwable.addSuppressed(Throwable)`
3. For each `athrow` that throws a newly constructed exception instance:
   - Report if no cause-preserving use of the caught exception is reachable on all paths leading to that `athrow`.
4. Do not report when the handler rethrows the caught exception directly.
5. Deduplicate findings by `(class, method, handler_start, throw_offset)`.

## Determinism constraints
- Iterate classes/methods/handlers in source order.
- Use sorted collections for handler traversal and use-site lists.
- Emit findings in instruction offset order.

## Complexity and performance
- Per handler, a single pass to index uses and throw sites: `O(I)` where `I` is handler instructions.
- Optional lightweight CFG to confirm reachability of cause-preserving calls to each `athrow`.
- Overall bounded by total bytecode size; no cross-method traversal.

## Test strategy
- TP: `catch (Exception e) { throw new RuntimeException("x"); }`.
- TN: `catch (Exception e) { throw new RuntimeException("x", e); }`.
- TN: `catch (Exception e) { throw e; }`.
- Edge: cause preserved via `initCause(e)` before `throw`.
- Edge: multi-catch where one of the caught types is rethrown without wrapping.

## Risks
- [ ] False positives when cause is preserved via helper methods (inter-procedural non-goal).
- [ ] False negatives when cause preservation is done via uncommon APIs or custom wrappers.
- [ ] CFG imprecision may mis-handle complex handler control flow.
- [ ] Over-reporting if the thrown exception is created outside the handler and rethrown.

## Post-mortem
- Went well: Implemented a lightweight stack tracker to associate thrown exceptions with cause-preserving calls.
- Tricky: Bytecode stack effects are partial, so the analysis stays conservative to avoid false positives.
- Follow-up: Consider a more path-sensitive check for initCause/addSuppressed in complex handlers.
