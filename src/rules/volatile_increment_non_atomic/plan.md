# Plan: volatile_increment_non_atomic

## Objective
Detect non-atomic read-modify-write updates on `volatile` fields (for example `++`, `--`, `+=`, `-=`) that can lose updates under concurrent access.

## Problem framing
`volatile` guarantees visibility, not atomicity of compound updates. Field updates that compile to read-then-write sequences can race across threads and overwrite each other, even though each individual volatile read/write is visible.

## Scope
- Analyze JVM bytecode for volatile instance/static field updates implemented as read-modify-write sequences in a single method.
- Cover common compiler patterns for `++`, `--`, and compound assignments on primitive numeric volatile fields.
- Report one finding per volatile field update site that matches a non-atomic RMW pattern.

## Non-goals
- Proving thread safety via external locking, calling context, or inter-procedural synchronization.
- Detecting races for non-volatile fields (handled by other analyses, if any).
- Modeling semantics from non-JSpecify annotations.
- `@Suppress` / `@SuppressWarnings` based suppression behavior.

## Detection strategy
1. Resolve field metadata and keep only fields marked `volatile`.
2. Scan instructions in method order and identify candidate windows where the same volatile field is:
   - loaded (`GETFIELD`/`GETSTATIC`),
   - transformed by arithmetic/bitwise op,
   - then stored back (`PUTFIELD`/`PUTSTATIC`).
3. Support compiler-emitted stack-shape variants for pre/post increment and compound assignment forms.
4. Exclude plain writes that do not read the old field value first.
5. Deduplicate by `(class, method, instruction_offset)`.

## Determinism constraints
- Iterate classes, methods, and instructions in stable source/bytecode order.
- Use deterministic collections (`BTreeMap`/`BTreeSet`) where ordering matters.
- Emit findings sorted by `(class, method, offset)`.

## Complexity and performance
- Primary pass is linear in instruction count per method: `O(I)`.
- Candidate matching uses bounded local windows around volatile field access, avoiding CFG-wide path exploration.
- Memory is `O(K)` for candidate bookkeeping, where `K` is number of volatile field access candidates.

## Test strategy
- TP: `volatile` instance field with `field++`.
- TP: `volatile` static field with `field += value`.
- TN: plain assignment (`field = value`) to volatile field.
- TN: same compound update on non-volatile field.
- Edge: post-increment used in expression context (`x = field++`) still reports once at update site.
- Edge: repeated updates in one method produce stable, deduplicated findings.

## Risks
- [ ] False negatives from uncommon compiler bytecode shapes not covered by initial matcher.
- [ ] False positives when updates are guarded by synchronization not modeled by this rule.
- [ ] Type/stack-shape handling bugs across primitive widths (`int`, `long`, `float`, `double`).
- [ ] Duplicate findings if multiple overlapping candidate windows map to one logical update.
