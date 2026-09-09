# inspequte Rule Idea Prompt (for plan input)

You are generating exactly one new static-analysis rule idea for inspequte.

## Required references
- Read `prompts/references/no-go-history.md` if the file exists.
- Exclude directions explicitly marked `disposition: design_rejected` unless materially different. Implementation or evidence failures are retry candidates, not rejected designs. Treat implemented entries as existing rules. For legacy entries, use explicit human design decisions and current code; do not infer design rejection from No-Go alone.
- Avoid duplicate or near-duplicate proposals by both `rule-id` and semantic intent.

## Goal
Propose one idea that should become input for the plan skill:
- `rule-id`
- `rule idea` (short text)

## Priority policy
- You may reference existing rules from SpotBugs, Error Prone, PMD, Facebook Infer, etc.
- However, do **not** prioritize ideas that are already well-covered by common tools.
- Prefer issues that are difficult for humans/agents to notice during review.
- Strongly prefer issues where findings depend on control flow, data flow, call order, exception paths, or broader context.
- Lower priority for issues that compilers can already detect reliably.

## Selection criteria
Choose one idea with the highest practical value under these constraints:
1. Hard to notice without systematic analysis
2. Actionable fix can be suggested clearly
3. Deterministic detection is plausible from bytecode-level analysis
4. Not a trivial duplicate of mainstream rule sets

## Output format (strict)
Output exactly these 2 lines and nothing else:
```text
rule-id: <snake_case_id>
rule idea: <one short sentence>
```

## Output constraints
- Only one idea.
- Keep `rule idea` concise and specific.
- Do not output explanations, alternatives, scoring, or extra sections.
