# Rule Plan: return_in_finally

## Summary
Detect `return` (and implicit value returns) inside `finally` blocks that can override a thrown exception or prior return, causing control-flow to hide failures.

## Problem Framing
Returning from `finally` changes the method's control flow and can suppress exceptions or earlier return values. This is easy to miss in review, especially in larger try/catch/finally blocks, and leads to lost errors or unexpected results.

## Scope
- Bytecode patterns that represent `return` opcodes executed within a `finally` handler.
- Also detect `athrow` within `finally` if it overrides a pending exception (optional follow-up; keep initial scope to `return`).
- Applies to methods compiled with explicit try/finally or try/catch/finally.

## Non-Goals
- Do not attempt source-level reconstruction of `finally` syntax beyond bytecode handler ranges.
- Do not infer developer intent (e.g., deliberate override).
- Do not support suppression via `@Suppress` or `@SuppressWarnings`.
- Do not treat non-JSpecify annotations as semantics-affecting.

## Detection Strategy
- Use exception table to identify `finally` handlers (catch-all handlers that rethrow or run cleanup).
- Build CFG for each method and identify basic blocks belonging to `finally` handler ranges.
- Flag any `return` opcode (IRETURN/LRETURN/FRETURN/DRETURN/ARETURN/RETURN) whose block is in a `finally` handler range.
- Ensure deterministic ordering by sorting findings by method, then bytecode offset.

## Rule Message
- Problem: "Return in finally overrides exceptions or prior returns."
- Fix: "Move the return outside the finally block or assign to a local and return after the try/finally."

## Test Strategy
- TP: try/finally where finally returns a value while try throws.
- TP: try/catch/finally where finally returns after catch.
- TN: finally with no return.
- TN: return after try/finally using a local assigned in finally.
- Edge: nested try/finally with inner finally return; ensure correct handler association.

## Complexity and Determinism
- Single-pass per method over exception table and CFG; expected O(N) in bytecode size.
- Avoid unordered map iteration; sort handler ranges and findings.

## Annotation Policy
- No suppression annotations are supported.
- Only JSpecify annotations are in scope for annotation-driven semantics.
- Non-JSpecify annotations do not change rule behavior.

## Risks
- [ ] Misidentifying synthetic compiler-generated finally blocks (e.g., try-with-resources) as user-authored; consider filtering if noisy.
- [ ] Incorrect handler range mapping in obfuscated bytecode; ensure robust range checks.
- [ ] High false positives if return appears in a block shared by non-finally paths; validate CFG membership precisely.
