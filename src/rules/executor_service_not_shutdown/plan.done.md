# Plan: executor_service_not_shutdown

## Objective
Detect executor services that are created inside a method and can reach a method exit without a matching local shutdown step.

## Scope
- Track executor instances created in the same method via:
  - direct constructors of classes that implement `ExecutorService`
  - selected JDK `Executors` factory methods that allocate a new executor service
- Require shutdown inside the same method via:
  - `shutdown()`
  - `shutdownNow()`
  - `close()`
- Report only when the executor lifecycle remains local to the method.

## Non-goals
- Do not report executors whose ownership is transferred out of the local method scope.
- Treat these transfers as out of scope and stop tracking them:
  - storing into fields
  - storing into arrays or other heap-backed containers
  - returning the executor
  - passing the executor as an argument to another call
- Do not infer lifecycle management across helper methods.
- Do not add suppression support via `@Suppress` or `@SuppressWarnings`.
- Do not add non-JSpecify annotation semantics.

## Detection strategy
1. Run one intraprocedural CFG worklist per method.
2. Assign a symbolic ID to each locally created reference.
3. Mark a symbolic ID as a tracked executor when:
   - a constructor call initializes a class assignable to `ExecutorService`, or
   - a whitelisted `Executors.*` factory returns a new executor service
4. Keep tracked IDs alive until one of these happens:
   - `shutdown()`, `shutdownNow()`, or `close()` on that receiver
   - ownership escapes through field store, return, or method argument
5. At each reachable terminal path, report every still-tracked executor creation site.

## Test strategy
- TP: local executor from `Executors.newSingleThreadExecutor()` without shutdown.
- TN: executor shut down in `finally`.
- Edge: branch with early return before shutdown.
- Scope guard: executor stored into a field is not reported.
- Scope guard: executor stored into a local array is not reported.
- Scope guard: executor returned from the method is not reported.
- Scope guard: executor passed to another method is not reported.
- TN: try-with-resources/`close()` counts as shutdown.

## Risks
- [ ] Factory whitelist is intentionally narrow; missing JDK creation APIs can cause false negatives.
- [ ] Ownership transfer heuristics are conservative; helper-method shutdown patterns are intentionally ignored.
- [ ] CFG path exploration must stay deterministic and avoid duplicate reports for the same creation site.

## Post-mortem
- Went well: a single worklist pass was enough to cover constructor-based creation, `Executors` factories, and branch-sensitive exit checks.
- Tricky: try-with-resources introduced synthetic `ifnull` guards, so the analysis needed a small non-null branch filter for locally created executors.
- Follow-up: broadening factory coverage later should be done carefully to avoid reintroducing ownership-transfer false positives.
