# src/rules/AGENTS.md

This directory contains inspequte rules.

This document defines **design principles for coding agents and contributors**.
Rules must remain scalable, deterministic, and low-noise as the rule set grows.

---

# 1. Rule Independence

## Principle
Each rule MUST be independent and pure.

A rule must:
- Analyze program facts only.
- Produce findings based solely on source inputs and shared read-only analysis artifacts.
- Not depend on other rules' outputs or execution order.

## Why
Cross-rule coupling does not scale.
It creates hidden ordering assumptions and makes behavior fragile as the rule set grows.

## Avoid
- Reading or filtering based on another rule’s warnings.
- Sharing mutable global state across rules.
- Assuming "Rule A runs before Rule B".
- Writing side-effects that affect other rules.

---

# 2. Determinism

## Principle
Running the same input twice MUST produce identical findings.

This includes:
- Same findings
- Same ordering
- Same identifiers for findings

## Why
Non-deterministic results make verify unreliable and break regression detection.

## Avoid
- Using unordered iteration without sorting.
- Keying findings only by line number.
- Depending on hash map iteration order.
- Producing outputs that vary by environment or timing.

---

# 3. Spec Is the Contract

## Principle
`spec.md` is the single source of truth for rule behavior.

Implementation must conform to `spec.md`.
If behavior changes, that is a **spec change**, not an implementation tweak.

## Why
Spec stability enables:
- Reliable verify
- Reproducible behavior
- Automatic documentation generation

## Avoid
- Modifying `spec.md` to justify implementation shortcuts.
- Allowing implementation comments to redefine behavior.
- Leaving non-goals undocumented.

---

# 4. Precision Over Noise

## Principle
Rules should prioritize precision over recall unless explicitly documented otherwise.

Each rule must clearly define:
- What it detects
- What it does NOT detect
- Acceptable noise thresholds (if any)

## Why
False positives erode trust and reduce tool adoption.

## Avoid
- Triggering on common idioms without filtering.
- Analyzing generated/vendor/testdata code by default.
- Emitting vague or non-actionable messages.
- Skipping suppression strategy discussion.

---

# 5. Performance Stability

## Principle
Rules must have predictable and bounded computational behavior.

If a rule may be expensive, it must:
- Document expected complexity in `plan.md`
- Reuse shared analysis artifacts safely
- Avoid redundant computation

## Why
Performance cliffs make the tool unusable in large codebases.

## Avoid
- O(N²) traversals without justification.
- Recomputing CFG/type resolution repeatedly.
- Re-parsing identical artifacts per rule.
- Hidden exponential behavior in nested graph walks.

---

# 6. Verify Isolation

## Principle
Verification must be based on explicit file inputs only.
Verification MUST only use spec + diff + reports.

Verify must use:
- `spec.md`
- `git diff` (or patch)
- Standardized test/e2e reports

No other inputs are allowed for verify decisions.

Verify must NOT use:
- `plan.md`
- Implementation discussion logs
- Prior conversation context

## Why
Verify must evaluate contract compliance, not implementation intent.

## Avoid
- Using design rationale to relax spec requirements.
- Inferring intended behavior from comments.
- Accepting deviations because "the implementation makes sense".

---

# 7. Reproducible E2E Evaluation

## Principle
E2E analysis must be reproducible and version-pinned.

Representative targets must:
- Be pinned by commit hash
- Have stable execution scope
- Produce normalized outputs

## Why
Moving targets invalidate regression detection.

## Avoid
- Running against HEAD of remote repositories.
- Comparing raw line-number-based findings.
- Mixing exploratory and representative targets.

---

# 8. Annotation Policy

## Principle
- `@Suppress`-style annotations are not supported for findings suppression.
- Annotation-driven rule semantics must support JSpecify only.
- Non-JSpecify annotations must be treated as unsupported and must not change rule behavior unless `spec.md` explicitly says otherwise.

## Why
Keeping annotation semantics narrow prevents cross-ecosystem ambiguity and nondeterministic behavior.

## Avoid
- Adding logic that suppresses findings via `@Suppress` or `@SuppressWarnings`.
- Mixing semantics from non-JSpecify annotation sets without an explicit spec change.

---

# Definition of Done (Per Rule)

A rule is complete when:

- `spec.md` exists and is complete.
- Unit tests cover TP, TN, and edge cases.
- Findings are deterministic and normalized.
- Representative E2E run stays within accepted thresholds.
- Documentation can be generated from `spec.md`.

---

# Directory Structure

Each rule must live under:

```
src/rules/<rule-id>/
    spec.md # Contract
    plan.md # Optional internal design notes
    implementation
    tests
    fixtures
```

No cross-rule hidden dependencies are allowed.
