# inspequte Rule Authoring

Complete the requested rule change with one author retaining context across
investigation, design, specification, implementation, and development tests.
Choose the order and depth of these activities to fit the task. Do not create a
new agent merely to move from one activity to the next.

## Scope and context
- Start from the user's goal and `AGENTS.md`, then `src/rules/AGENTS.md`.
- If a rule idea is requested, use the selection criteria in `prompts/ideate-rule.md`.
  Skip ideation when the user already specified the work. Distinguish rejected
  designs from implementation/evidence failures in the No-Go history.
- Read relevant source, callers, shared analysis helpers, tests, and specifications
  as needed. There is no fixed file-count limit.
- Create a plan only when complexity or handoff needs justify one. Keep material
  decisions and unresolved issues, not a record of every reasoning step.
- Use the plan/spec/implementation skills for their task-specific guidance. Their
  standalone prompt output restrictions apply when that standalone task is
  requested; they do not force separate stages or agents in this end-to-end task.

## Behavioral contract
- Investigate bytecode and shared analysis capabilities, and prototype when useful,
  before finalizing a new behavior contract. Refine draft specs within the user's
  requested scope, without weakening the requested acceptance criteria.
- Once a spec is ready for implementation, treat it as the fixed contract for
  implementation and repair. Keep an existing approved spec fixed from the start.
- If meeting the goal requires changing approved detection behavior or product
  policy, present the concrete spec change and its rationale for authorization.
  Do not stop for routine implementation choices already within scope.
- Implement the contract with TP, TN, and meaningful edge-case coverage. Use
  `.codex/skills/inspequte-rule-impl/SKILL.md` for repository-specific mechanics.

## Validation and independent review

Use focused tests during development. Before handoff, regenerate rule docs and
collect full acceptance evidence once for the final source, following
`docs/development-validation.md`:

1. Run `scripts/prepare-verify-input.sh <RULE_ID> [<BASE_REF_OR_EMPTY>]`.
2. Run `cargo build`, `cargo test`, and `cargo audit --format sarif`, saving stdout,
   stderr, and actual exit codes in the report files expected by
   `docs/rule-authoring-contract.md` and the evidence scripts.
3. Run `scripts/create-verify-manifest.sh` and `scripts/validate-verify-input.sh`.
4. Launch an independent reviewer with `prompts/authoring-verify.md`. Give it the
   validated evidence and access to source from the recorded Git tree, without
   the author's conversation or plan. The reviewer must not modify implementation.
   If independent execution is unavailable, report that review remains outstanding;
   do not label self-review as independent verification.
5. Use the structured result and reason taxonomy in `docs/rule-authoring-contract.md`.
   Keep repair work with the original author. Implementation/test defects can be
   repaired within the existing three-attempt limit; approved specs remain fixed.
   Other reasons follow the contract's authority and infrastructure boundaries.
   Regenerate evidence after source changes and obtain a fresh independent review.

Finish with changed behavior, validation results, and unresolved limitations.
Preserve the spec, diff/source identity, reports, and structured review result
needed to explain and repeat acceptance. Use `prompts/authoring-no-go-resume.md`
when specifically resuming an external No-Go PR.
