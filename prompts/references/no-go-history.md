# Rule Ideation No-Go History

This reference is used by `prompts/ideate-rule.md` to avoid proposing duplicate or low-value rule ideas.
Append one entry each time verify returns `No-Go`.

## Entry format
- `rule-id`: snake_case identifier
- `rule idea`: short summary used in ideation
- `no-go reason`: concise reason summary from verify
- `run-url`: GitHub Actions run URL for traceability

## Entries

### mutate_unmodifiable_collection
- rule-id: `mutate_unmodifiable_collection`
- rule idea: Detect attempts to mutate collections that are known to be unmodifiable because they were created by JDK unmodifiable factories in the same method.
- no-go reason: build and test failures from missing opcode constants; no implementation/tests in verify-input to validate spec requirements
- run-url: https://github.com/KengoTODA/inspequte/actions/runs/21924738785

## 2026-02-11T23:37:56Z | exception_cause_not_preserved
- rule-id: `exception_cause_not_preserved`
- rule idea: Detect catch blocks that throw a new exception without preserving the original as a cause.
- no-go reason: Unable to verify rule behavior against `verify-input/spec.md` because the provided change set only updates `src/rules/mod.rs` registration and does not include implementation or tests for `exception_cause_not_preserved`. Evidence: `verify-input/diff.patch` only touches rule registration and rule count.
- run-url: https://github.com/KengoTODA/inspequte/actions/runs/21927112667
