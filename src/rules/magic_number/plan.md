# Plan: magic_number

## Rule idea

Detect numeric literals (magic numbers) used directly in method bytecode, where a named constant would improve
readability and maintainability.

## Problem description

Magic numbers are unnamed numeric literals embedded directly in code.
They make code harder to understand the purpose of a code piece.
It also makes code less robust to change if a magic number is changed in one location but remains unchanged.
The numbers 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 100, 1000, 0L, 1L, 2L, 0.0, 1.0, 0.0F and 1.0F are NOT reported by this
inspection.

```java
// Bad: what does 3600 mean?
if(elapsed >3600){

resetSession(); }

// Good: intent is clear
private static final int SESSION_TIMEOUT_SECONDS = 3600;
if(elapsed >SESSION_TIMEOUT_SECONDS){

resetSession(); }
```

Static analysis tools like Checkstyle and PMD detect magic numbers at source level, but inspequte operates on bytecode.
This introduces a fundamental limitation: `javac` inlines compile-time constants (`static final` primitives and strings
with constant initializers) at usage sites. At the bytecode level, `use(NAMED_CONST)` and `use(42)` are
indistinguishable when `NAMED_CONST` is a compile-time constant.

Given this limitation, the rule targets a narrower, higher-confidence scope: numeric literals that appear in method
bodies via push/load instructions, excluding commonly acceptable values and known-safe bytecode patterns.

## Detection strategy

Bytecode-level detection:

1. Scan each method's instructions for numeric constant loading:
    - `bipush <value>` (byte-range integers)
    - `sipush <value>` (short-range integers)
    - `ldc` / `ldc_w` loading `CONSTANT_Integer`, `CONSTANT_Float`, `CONSTANT_Long`, or `CONSTANT_Double` from the
      constant pool
2. Exclude commonly acceptable values from a built-in allowlist:
    - Integers: -1, 0, 1, 2 (covered by `iconst_*` / `lconst_*` which are separate opcodes, but also via `bipush`)
    - Floats/doubles: 0.0, 1.0 (covered by `fconst_*` / `dconst_*`)
    - Powers of two up to a reasonable threshold (e.g., 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024)
    - Common byte/bit masks: 0xFF, 0xFFFF, 0xFFFFFFFF
3. Exclude numeric literals in known-safe instruction contexts:
    - Array creation sizes (`newarray`, `anewarray`, `multianewarray` immediate predecessors)
    - `tableswitch` / `lookupswitch` case values
    - Initial capacity for other known-collection-like types (StringBuilder, StringBuffer, Collection, Map)
    - Used in annotations
    - Used in the body of hashCode() method
4. Report each remaining non-allowlisted numeric literal occurrence, with the finding pointing to the instruction
   offset.

## Scope

**In scope:**

- Numeric literals loaded via `bipush`, `sipush`, `ldc`/`ldc_w`/`ldc2_w` within method bodies.
- Integer, long, float, and double constant types.
- A built-in allowlist of commonly acceptable values.

**Non-goals:**

- Distinguishing inlined compile-time constants from true magic numbers (fundamental bytecode limitation).
- Cross-class analysis to check whether a value is defined as a named constant elsewhere.
- String literal analysis (magic strings are a separate concern).
- Annotation-based suppression: `@Suppress`-style annotations are not supported.
- Non-JSpecify annotation semantics are not supported.
- Configurable allowlist (initial version uses a fixed built-in allowlist; may be extended later).

## Determinism constraints

- Iterate over methods in class-file declaration order.
- Within each method, iterate over instructions in bytecode offset order.
- Sort findings by (class name, method name, descriptor, bytecode offset) before emitting.
- This guarantees stable output across repeated runs.

## Test strategy

- TP: Method containing `bipush 42` or `sipush 3600` not in the allowlist.
- TP: Method containing `ldc` for a float/double literal like `3.14` or `9.81`.
- TN: Method using only allowlisted values (0, 1, -1, powers of two).
- TN: Method where the numeric literal is an array size for `newarray`.
- TN: `iconst_*` / `lconst_*` / `fconst_*` / `dconst_*` instructions (these encode values 0-5 etc. and are not `bipush`/
  `sipush`/`ldc`).
- Edge: `static final` field initializer containing a magic number in `<clinit>` — should report (the literal still
  appears in bytecode).
- Edge: `tableswitch` case labels — should NOT report.
- Edge: Negative values via `bipush` (e.g., `bipush -128`).

## Complexity

- O(N x M) where N = number of methods per class and M = number of instructions per method.
- No inter-method or inter-class analysis required; each method is evaluated independently.
- Allowlist lookup is O(1) with a HashSet.

## Risks

- [ ] High false positive rate: inlined compile-time constants are indistinguishable from true magic numbers at the
  bytecode level. Mitigation: document as a known limitation; keep the allowlist generous; consider making the rule
  opt-in or low severity.
- [ ] Noisy on generated code: annotation processors, serialization frameworks, and compiler-generated methods (e.g.,
  `switch` on enums) may contain many numeric literals. Mitigation: exclude `synthetic` / `bridge` methods; consider
  excluding `<clinit>` entirely or treating it specially.
- [ ] Allowlist calibration: too narrow an allowlist produces excessive noise; too broad misses real issues. Mitigation:
  start with a conservative (broader) allowlist and tighten based on representative target results.
- [ ] `bipush`/`sipush` opcode support: verify these opcodes are already defined in `src/opcodes.rs` or add them.
- [ ] Constant pool type resolution: ensure `ldc` instruction parsing correctly identifies the constant pool entry
  type (Integer vs Float vs String vs Class) to avoid reporting non-numeric constants.
