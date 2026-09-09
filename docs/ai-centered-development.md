# AI-centered development with accountable humans

inspequte explores AI-assisted engineering with explicit acceptance criteria and
human accountability. Agents may exercise engineering judgment within the requested
scope. Humans own product policy and authorization of changes to approved contracts.

## Durable evidence

Keep enough information to explain and repeat an acceptance decision:
- the behavior contract, including non-goals and meaningful examples;
- the reviewed source identity and diff;
- commands, environment, results, and relevant pinned evaluation fixtures;
- material design decisions, known limitations, and unresolved follow-ups;
- an independent structured verification result linked to its evidence.

Not every intermediate decision needs a file or machine-readable record. Plans are
useful for complex work and handoffs; they are not required for every small change.
Schema-validated data is reserved for machine consumers such as evidence validation
and routing. Narrative design rationale should be concise and written for reviewers.

## Responsibilities

`AGENTS.md` defines shared constraints and entry points. Rule-specific invariants
live in `src/rules/AGENTS.md`; each `spec.md` defines behavior. Skills describe
specialized work, and scripts own mechanical evidence checks. See
`development-validation.md` and `rule-authoring-contract.md` for validation policy.

The authoring prompt uses one author across investigation, design, specification,
implementation, and development tests, followed by independent review. This
reduces mandatory handoffs and lets the author retain discoveries while working.
It is an operational choice adopted without a comparative model experiment; no
measured quality or performance improvement is claimed. Accountability rests on
acceptance criteria and reproducible evidence, not a prescribed reasoning sequence.
