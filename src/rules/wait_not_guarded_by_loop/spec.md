# WAIT_NOT_GUARDED_BY_LOOP
- id: `WAIT_NOT_GUARDED_BY_LOOP`
- name: `Wait call not guarded by loop`
- description: `wait/await calls outside retry loops risk spurious-wakeup bugs`

## Motivation
`Object.wait(...)` and `Condition.await...(...)` are allowed to wake up without the desired condition becoming true. If the call is not guarded by a retry loop, code can proceed with invalid state and fail intermittently.

## What it detects
- Wait-style blocking calls in analysis-target classes where the call site is not enclosed by a retry loop.
- Covered APIs:
  - `Object.wait()`
  - `Object.wait(long)`
  - `Object.wait(long, int)`
  - `Condition.await()`
  - `Condition.awaitUninterruptibly()`
  - `Condition.awaitNanos(long)`
  - `Condition.awaitUntil(Date)`
  - `Condition.await(long, TimeUnit)`

## What it does NOT detect
- Semantic validation that the loop predicate is correct.
- Wait-like APIs outside the listed methods.
- Findings suppression via `@Suppress`/`@SuppressWarnings` (unsupported).
- Annotation-driven behavior beyond JSpecify (non-JSpecify annotations are unsupported for semantics).

## Examples (TP/TN/Edge)
- TP: `if (!ready) { lock.wait(); }` → report.
- TP: `if (!ready) { condition.await(); }` → report.
- TN: `while (!ready) { lock.wait(); }` → no report.
- TN: `while (!ready) { condition.awaitNanos(timeout); }` → no report.
- Edge: `if (!ready) { lock.wait(100L); }` → report.

## Output
- Message style: actionable and specific, e.g.
  - `Wrap wait/await in a condition-checking loop in <class>.<method><descriptor>; re-check the condition after wakeup to handle spurious wakeups.`
- One finding per detected call site.
- Include method location with source line when available.

## Performance considerations
- Scan only method call sites and CFG edges for each method.
- Keep processing deterministic and linear with method IR size.
- No global state and no cross-rule dependency.

## Acceptance criteria
- Reports wait/await call sites outside detected retry loops.
- Does not report listed wait/await call sites inside retry loops.
- Emits deterministic findings and ordering for identical inputs.
- Findings remain limited to analysis-target classes.
- User-facing message explains both risk and remediation.
