# Plan: Shared Stack/Locals Abstract Machine

## Objective
Introduce a reusable stack/locals abstraction for JVM symbolic analysis so rules can reuse safe stack operations, normalization, and budget handling.

## Background
Rules that inspect bytecode often need similar operand stack and local variable simulation. Hand-rolled implementations repeatedly introduce edge-case differences and growth bugs.

## Implementation Approach
- Add a generic machine module (implemented as `src/dataflow/stack_machine.rs`):
  - `StackMachine<V>` where `V` is a rule-specific abstract value
  - APIs: `push`, `pop`, `pop_n`, local `load/store`, merge helpers
  - Configurable caps: stack depth, tracked locals, tracked symbolic identities
- Provide built-in normalization hooks:
  - Canonicalization for symbolic identities
  - Pruning for dead local bindings
  - Truncation strategy for deep stack tails
- Keep engine-agnostic:
  - Usable from worklist engine or direct per-method scan
- Migrate one stack-heavy rule and verify behavior parity.

## Test Cases
- Unit tests:
  - Stack push/pop boundaries
  - Local load/store semantics
  - Canonicalization and truncation behavior
  - Deterministic state key generation
- Rule tests:
  - Existing true/false positive tests for migrated rules
  - Loop-heavy bytecode cases that previously risked growth

## Success Criteria
- Common stack/locals behavior is centralized in one module.
- Migrated rules no longer manually implement stack utility functions.
- No regression in existing SARIF rule outputs.

## Dependencies
- Shared analysis engine (optional but complementary).
- Existing opcode and instruction models.

## Complexity Estimate
Medium-High

## Post-mortem
- Went well: `StackMachine<V>` covered both a worklist-based rule (`EXCEPTION_CAUSE_NOT_PRESERVED`) and a direct-scan rule (`ARRAY_EQUALS`) with the same API surface.
- Tricky: symbolic identity canonicalization had to return an explicit mapping so preserved-side sets could be remapped deterministically.
- Follow-up: migrate SLF4J/Log4j stack-simulation rules to `StackMachine<V>` to reduce duplicated opcode handling.
