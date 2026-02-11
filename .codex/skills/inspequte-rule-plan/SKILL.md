---
name: inspequte-rule-plan
description: Draft or update a rule plan for inspequte from a short idea and target rule-id. Use when the task is to create or refine src/rules/<rule-id>/plan.md, including risks, without creating spec.md.
---

# inspequte rule plan

## Inputs
- Rule idea text (short problem statement).
- Target `rule-id`.
- Optional existing `src/rules/<rule-id>/plan.md`.

## Outputs
- Create or update `src/rules/<rule-id>/plan.md`.
- Include a short risk checklist section in `plan.md`.
- Do not create or modify `spec.md`.

## Minimal Context Loading
1. Read `src/rules/AGENTS.md`.
2. Read `src/rules/<rule-id>/plan.md` if it exists.
3. Read only one or two related rule specs if needed for scope calibration.
4. Do not scan the whole repository.

## Workflow
1. Confirm target path: `src/rules/<rule-id>/plan.md`.
2. Capture problem framing, detection strategy, non-goals, and test strategy.
3. Add complexity and deterministic behavior constraints.
4. Add a `## Risks` checklist with short, actionable bullets.

## Definition of Done
- `plan.md` exists at the target rule directory.
- Plan describes scope and non-goals clearly.
- Risks are listed as a short checklist.
- `spec.md` is untouched.
