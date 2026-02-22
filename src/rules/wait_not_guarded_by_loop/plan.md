# wait_not_guarded_by_loop plan

## Problem framing
`Object.wait(...)` and `Condition.await...(...)` can wake up spuriously. Calling them outside retry loops can violate invariants and cause flaky behavior.

## Scope
- Detect wait/await calls in analysis-target classes where the call is not inside a detected backward control-flow region.
- Cover:
  - `java/lang/Object.wait()V`
  - `java/lang/Object.wait(J)V`
  - `java/lang/Object.wait(JI)V`
  - `java/util/concurrent/locks/Condition.await()V`
  - `java/util/concurrent/locks/Condition.awaitUninterruptibly()V`
  - `java/util/concurrent/locks/Condition.awaitNanos(J)J`
  - `java/util/concurrent/locks/Condition.awaitUntil(Ljava/util/Date;)Z`
  - `java/util/concurrent/locks/Condition.await(JLjava/util/concurrent/TimeUnit;)Z`

## Detection strategy
1. Scan call sites in each method.
2. Select wait/await targets by owner/name/descriptor.
3. Use method CFG back-edges to infer loop containment around the call offset.
4. Report when no back-edge range encloses the call offset.

## Non-goals
- Proving semantic correctness of loop predicates.
- Supporting `@Suppress` / `@SuppressWarnings` suppression behavior.
- Annotation-driven semantics beyond JSpecify (none are required for this rule).

## Determinism and complexity
- Deterministic iteration over existing IR vectors.
- Per-method complexity O(calls × edges), acceptable for bounded CFG size.
- Emit one finding per matching call site.

## Test strategy
- TP: `wait()` under `if` (no retry loop) should report.
- TN: `wait()` inside `while` should not report.
- Edge:
  - `Condition.await()` under `if` should report.
  - `Condition.awaitNanos(...)` inside `while` should not report.
  - `Object.wait(long)` under `if` should report.
  - Unrelated method calls should not report.

## Risks
- [ ] Bytecode block boundaries could miss rare loop shapes.
- [ ] Back-edge heuristic may under-report if loop lowering is unusual.
- [ ] Expanded signatures for `Condition` methods may miss exotic owners.
