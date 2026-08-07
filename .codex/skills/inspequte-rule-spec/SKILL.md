---
name: inspequte-rule-spec
description: Author or refine an inspequte rule spec from a rule idea, optional plan.md, and target rule-id. Use when writing src/rules/<rule-id>/spec.md from a fixed template while avoiding implementation details.
---

# inspequte rule spec

## Inputs
- Target `rule-id`.
- Rule idea text.
- Optional `src/rules/<rule-id>/plan.md`.

## Outputs
- Create or update `src/rules/<rule-id>/spec.md`.
- Keep scope contractual; avoid implementation details beyond constraints.

## OKF Frontmatter
Begin every spec with this OKF v0.2 frontmatter profile:

```yaml
---
type: 'Static Analysis Rule'
title: '<RuleMetadata.name>'
description: '<RuleMetadata.description>'
tags: ['jvm', 'static-analysis']
status: 'stable'
rule_id: '<RuleMetadata.id>'
---
```

- Keep this field order and use quoted strings plus an inline string list.
- `type` must remain `Static Analysis Rule`.
- `title`, `description`, and `rule_id` define the intended Rust `RuleMetadata` values.
- `rule_id` must use uppercase snake case and must match the Rule ID in `## Summary`.
- Keep the required `jvm` and `static-analysis` tags; append specific domain tags only when useful.
- Use `draft` only while the behavior contract is incomplete. A spec ready for implementation is `stable`.

## Fixed Template
Use this exact section order:
1. `## Summary`
2. `## Motivation`
3. `## What it detects`
4. `## What it does NOT detect`
5. `## Examples (TP/TN/Edge)`
6. `## Output`
7. `## Performance considerations`
8. `## Acceptance criteria`

## Minimal Context Loading
1. Read `src/rules/AGENTS.md`.
2. Read existing `src/rules/<rule-id>/spec.md` if present.
3. Read `src/rules/<rule-id>/plan.md` if present.
4. Read at most one related rule spec for style alignment.
5. Do not perform repo-wide scans.

## Guardrails
- Treat `spec.md` as a behavior contract, not a design doc.
- Do not include Rust APIs, struct names, function names, or algorithm internals.
- Keep messages user-facing and actionable.
- In `## Summary`, repeat the intended rule metadata (`id`, `name`, `description`) exactly as declared in frontmatter so implementation can map it directly.
- State annotation scope explicitly: `@Suppress`-style suppression is unsupported, and only JSpecify annotations are supported for annotation-driven semantics.

## Definition of Done
- `spec.md` exists under the target rule directory.
- OKF frontmatter is present and follows the project profile.
- All template sections are present and non-empty.
- Examples include true positive, true negative, and edge cases.
- Implementation-specific details are excluded.
