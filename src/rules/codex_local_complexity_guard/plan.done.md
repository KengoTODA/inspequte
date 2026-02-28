# Plan: codex_local_complexity_guard

## Objective
Detect methods whose local cyclomatic complexity exceeds a strict fixed threshold and emit one deterministic finding per method.

## Rule idea
Detect methods whose cyclomatic complexity exceeds a strict local threshold and report deterministic findings.

## Problem framing
Methods with many independent control-flow paths are harder to review, test, and maintain.
A bytecode-level local complexity guard highlights risky methods even when source code is unavailable.
The rule must remain deterministic and low-noise so results are trustworthy in CI.

## Scope
- Analyze concrete method bodies in analyzed class files.
- Compute method-local cyclomatic complexity from bytecode control-flow constructs.
- Use one built-in strict threshold (initial target: complexity `> 10`).
- Emit at most one finding per method when complexity exceeds the threshold.
- Include measured complexity and threshold in the finding message so remediation is clear.

## Non-goals
- Inter-procedural complexity (callee complexity does not affect caller findings).
- Cognitive complexity or style-driven scoring heuristics.
- Auto-remediation beyond clear guidance to split or simplify method logic.
- Annotation-based suppression via `@Suppress` or `@SuppressWarnings`.
- Annotation-driven semantics from non-JSpecify annotations.
- Any annotation-driven behavior beyond JSpecify (not needed for this rule's initial logic).

## Detection strategy
1. Iterate class members in deterministic order and skip methods without bytecode (`abstract`, `native`).
2. Skip synthetic/bridge methods to reduce compiler-generated noise.
3. Count local decision contributions per method:
   - conditional branch opcodes (`if*`, `ifnull`, `ifnonnull`);
   - `tableswitch` and `lookupswitch` case branches (non-default targets);
   - exception handlers (`catch`) as additional branch points.
4. Compute `cyclomatic_complexity = 1 + decision_contributions`.
5. Compare complexity to the fixed threshold and report once per method when exceeded.
6. Deduplicate/report by stable method identity `(class_name, method_name, descriptor)`.

## Determinism constraints
- Traverse classes, methods, and instructions in stable bytecode order.
- Use deterministic collections when storing interim method metrics/findings.
- Sort emitted findings by `(class_name, method_name, descriptor)`.
- Keep threshold rule-internal and constant (no environment-dependent inputs).

## Complexity and performance
- Time: `O(I)` per method, where `I` is method instruction count.
- Memory: `O(1)` per method plus bounded space for findings.
- No whole-program graph traversal and no cross-method state dependencies.

## Test strategy
- TP: method with nested conditionals/switch cases that exceed threshold.
- TN: simple method below threshold.
- Boundary: method complexity exactly equal to threshold does not report.
- Edge: large switch counted correctly and reported deterministically.
- Edge: methods with catch handlers increase complexity as specified.
- Edge: synthetic/bridge methods are ignored.
- Determinism: repeated runs produce identical findings and ordering.

## Risks
- [ ] Complexity inflation from compiler-desugared bytecode (especially switch and try/catch patterns).
- [ ] Fixed strict threshold may be noisy in legacy codebases.
- [ ] Bytecode shape differences across compilers/JVM languages may require extra fixtures.
- [ ] Catch-handler counting policy may need tuning after representative OSS validation.

## Post-mortem
- Went well: the existing bytecode utilities (`opcode_length`, `padding`, and `read_u32`) made switch/branch counting straightforward without scanner changes.
- Tricky: ensuring deterministic ordering by method identity required explicit sorting independent of complexity values in user-facing messages.
- Follow-up: run broader OSS false-positive sweeps to validate that counting only typed catch handlers (`catch_type.is_some()`) stays precise across compiler patterns.
