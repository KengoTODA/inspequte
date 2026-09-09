---
name: inspequte-rule-plan
description: Draft or refine an optional inspequte rule plan when design complexity, risks, or handoff needs justify durable notes. For a standalone plan request, leave the spec unchanged.
---

# inspequte rule plan

Use this guidance within the single-author flow in `prompts/authoring-rule.md`.
Do not spawn a new author for this activity. When the user requests only this
standalone task, keep its output scope; end-to-end authoring may continue to other
activities in the same context.

## Inputs
- Rule idea text (short problem statement).
- Target `rule-id`.
- Optional existing `src/rules/<rule-id>/plan.md`.

## Outputs
- Create or update `src/rules/<rule-id>/plan.md`.
- Include a short risk checklist section in `plan.md`.
- For a standalone plan request, do not create or modify `spec.md`.

## Relevant Context
1. Read `src/rules/AGENTS.md`.
2. Read `src/rules/<rule-id>/plan.md` if it exists.
3. Read related specs and source as needed to assess scope and feasibility. Follow dependencies when they affect the decision.

## Workflow
1. Confirm target path: `src/rules/<rule-id>/plan.md`.
2. Capture problem framing, detection strategy, non-goals, and test strategy.
3. Add complexity and deterministic behavior constraints.
4. Record annotation policy constraints in scope/non-goals:
   - no `@Suppress`-based suppression support
   - only JSpecify is in scope for annotation-driven semantics
5. Add a `## Risks` checklist with short, actionable bullets.

## Definition of Done
- `plan.md` exists at the target rule directory.
- Plan describes scope and non-goals clearly.
- Risks are listed as a short checklist.
- For a standalone plan request, `spec.md` is untouched.
