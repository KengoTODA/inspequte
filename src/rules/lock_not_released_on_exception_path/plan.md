# Plan: lock_not_released_on_exception_path

## Objective
Detect methods that acquire a `java.util.concurrent.locks.Lock` (or `ReentrantLock`) but allow at least one reachable exit path without a matching `unlock()` after that acquisition.

## Problem framing
A missing `unlock()` on exceptional or early-return paths can permanently hold a lock and block other threads. This is hard to catch in review because the bug depends on control flow and exception edges, not just local syntax.

## Scope
- Analyze method bytecode and CFG for lock/unlock call order.
- Focus on `lock()` / `unlock()` pairs where calls are visible in the same method.
- Report when a path from a `lock()` call reaches method exit without any reachable `unlock()` call after that lock.

## Non-goals
- Inter-procedural tracking (unlock in callee is out of scope).
- Ownership/alias proof that lock and unlock receiver are exactly the same runtime object.
- Semantics driven by non-JSpecify annotations.
- `@Suppress` / `@SuppressWarnings` based suppression behavior.

## Detection strategy
1. Find invocation instructions that match lock acquisition:
   - `name == "lock"`
   - `descriptor == "()V"`
   - owner is `java/util/concurrent/locks/Lock` or `java/util/concurrent/locks/ReentrantLock`.
2. For each acquisition site, perform bounded CFG state exploration from the next program point.
3. Track one bit of state: whether an `unlock()` has been seen since that acquisition.
4. If any terminal path is reachable with `unlock_seen == false`, report one finding for that acquisition site.
5. Deduplicate findings by `(class, method, lock_offset)`.

## Determinism constraints
- Traverse classes/methods/instructions in source order.
- Build successor sets using sorted `BTreeMap` / `BTreeSet`.
- Keep report order stable by iterating lock sites in instruction order.

## Complexity and performance
- Per lock site, exploration is `O(B + E)` over CFG with two-state visitation (`unlock_seen` false/true).
- In worst case: `O(L * (B + E))` per method, where `L` is number of lock sites.
- Early stop when the first unsafe terminal path is found for a lock site.

## Test strategy
- TP: `lock()` followed by possible exceptional exit without `unlock()`.
- TN: `lock()` with `unlock()` in `finally` pattern.
- Edge: multiple lock sites where only one is unsafe.
- Edge: method without `lock()` should not report.

## Risks
- [ ] False positives when unlock occurs in helper methods (inter-procedural non-goal).
- [ ] False negatives when CFG misses certain exceptional edges in uncommon bytecode patterns.
- [ ] Receiver alias mismatch (lock/unlock on different instances) cannot be proven reliably at bytecode-only level.
- [ ] Performance regressions in methods with many lock sites and dense CFGs.
