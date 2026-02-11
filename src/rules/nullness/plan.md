# Plan: Type-Use Nullness Flow Analysis for Generic Calls

## Objective
Extend nullness flow analysis so generic method returns are specialized by receiver type-use, then remove the ignore from `nullness_rule_reports_type_use_flow_from_generic_call`.

## Background
Type-use metadata is already parsed and used for override checks, but flow analysis still computes call return nullness from declaration-level method nullness only (`lookup_return_nullness`).

This misses cases like:
- receiver type: `ClassB<@Nullable String>`
- called method return: `T`
- expected call result nullness: `@Nullable`

Current behavior keeps the result as non-null/unknown, so no `possible null receiver` is reported for chained calls.

## Scope
- In scope:
  - Nullness flow refinement for generic call returns in `src/rules/nullness/mod.rs`
  - Minimal IR/parser additions required to resolve class type parameters
  - Test coverage for positive/negative generic-call flow outcomes
- Out of scope:
  - Full method type-argument inference from invocation arguments
  - Interprocedural flow summaries
  - External API nullness database integration (covered by plan 02)

## Implementation Approach
1. Add class generic metadata to IR
   - Parse class `Signature` in `src/scan.rs`.
   - Extend `Class` IR with class type parameters (names + bounds) or equivalent mapping metadata.
   - Preserve compatibility when signature is missing (raw/erased types).

2. Carry type-use through intra-method flow state
   - Extend `State` / `StackValue` in `src/rules/nullness/mod.rs` to optionally track `TypeUse` alongside nullness.
   - Seed parameter local slots from `method.type_use.parameters`.
   - Propagate type-use through `ALOAD`/`ASTORE` and stack operations needed by current flow checks.

3. Specialize return type-use at call sites
   - Replace `lookup_return_nullness`-only behavior with a resolver that:
   - finds callee method/type-use
   - builds type variable substitutions from receiver class type arguments
   - applies substitutions to callee return `TypeUse`
   - derives effective top-level return nullness from substituted return type
   - falls back to existing declaration nullness when specialization is not possible

4. Keep diagnostics intuitive and stable
   - Reuse current user-facing null receiver wording unless a clearer fix hint is necessary.
   - Ensure behavior is conservative on unresolved generics (`Unknown`, not forced `Nullable`).

## Test Plan
1. Unignore and pass existing test:
   - `src/rules/nullness/mod.rs`: `nullness_rule_reports_type_use_flow_from_generic_call`

2. Add focused harness tests:
   - No report for `ClassB<@NonNull String>` chained call.
   - No report when generic return cannot be specialized (raw type usage).
   - Report still triggered when nullable value is stored in a local and used later as receiver (propagation check).

3. Add small unit tests around substitution helper(s):
   - `T -> @Nullable String` resolves return nullness to `Nullable`.
   - Missing mapping keeps unresolved type variable conservative.

## Verification
- `cargo fmt`
- `cargo build`
- `JAVA_HOME=$(/usr/libexec/java_home -v 21) cargo test nullness_rule_reports_type_use_flow_from_generic_call -- --nocapture`
- `JAVA_HOME=$(/usr/libexec/java_home -v 21) cargo test`
- `cargo audit --format sarif`

## Success Criteria
- Ignored test is enabled and passes.
- Generic-call flow reports `possible null receiver` for nullable-specialized returns.
- Existing nullness flow and override tests remain green.
- No new unstable ordering in SARIF output.

## Risks and Mitigations
- Risk: Incomplete type variable mapping causes false negatives.
  - Mitigation: Explicit fallback path + unit tests for unresolved substitutions.
- Risk: Over-aggressive substitution causes false positives.
  - Mitigation: Restrict first implementation to receiver-class type parameters and top-level return nullness.

## Dependencies
- Existing type-use parser and IR (`src/scan.rs`, `src/ir.rs`)
- Nullness flow engine (`src/rules/nullness/mod.rs`)
- Java 21 harness environment (`JAVA_HOME`)

## Estimated Complexity
**Medium-High** - touches IR, parser, and flow transfer logic, but within current architecture.

## Post-mortem
- Flow state extension with optional `TypeUse` on locals/stack integrated cleanly with existing block join logic.
- The tricky part was preserving useful nullness when substituting `TypeVar` returns without over-trusting unresolved generics.
- Follow-up: add harness coverage for raw-type generic calls and non-null type arguments to guard against regressions.
