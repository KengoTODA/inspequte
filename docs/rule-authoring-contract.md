# Rule authoring contract

The rule-authoring workflow uses versioned JSON contracts so evidence validation,
semantic verification, and workflow routing do not depend on parsing prose.

## Verification reasons

`verification-result.json` uses the following closed reason taxonomy:

| Reason | Meaning | Route |
| --- | --- | --- |
| `none` | Verification passed. | Finish with `go`. |
| `implementation_defect` | The implementation does not satisfy the fixed specification. | Retry implementation. |
| `test_defect` | Required or correct test coverage is missing. | Retry implementation and tests. |
| `spec_ambiguity` | The fixed specification cannot determine correct behavior. | Stop for human review. |
| `suspected_false_positive` | The rule design is likely to report valid code. | Stop for rule-design and specification review. |
| `stale_or_missing_evidence` | Evidence does not describe the exact source state. | Regenerate evidence without consuming an implementation attempt. |
| `infrastructure_failure` | Tooling or workflow execution prevented verification. | Apply the bounded infrastructure retry policy. |

Only `implementation_defect` and `test_defect` may set
`implementationRetryable` to `true`. A `go` result must use reason `none` and
must set `implementationRetryable` to `false`.

## Terminal states

The workflow, rather than the verifier, derives one of these terminal states:

- `go`: independent verification passed.
- `needs_human`: the result requires specification or rule-design authority.
- `retry_exhausted`: three implementation attempts were unsuccessful.
- `infrastructure_failed`: the separate infrastructure retry budget was exhausted.

The specification is immutable during implementation retries. Specification
ambiguity and suspected false positives therefore cannot route directly back to
implementation.

## Evidence identity

`manifest.json` binds verification evidence to a base commit, current `HEAD`, a
Git tree containing the actual reviewed files, the review diff, the copied
specification, every changed file, and every command report. Consumers must
validate the manifest and recompute all hashes before semantic verification.

The source state is identified by `treeSha`; `headCommitSha` records the commit
from which the working state was produced. This distinction allows local or CI
changes to be verified before the final branch commit is published.

## Retry budgets

Implementation attempts and infrastructure retries are counted independently.
The initial implementation is attempt 1, and no more than three implementation
attempts are allowed. Regenerating stale evidence does not consume an
implementation attempt. Both budgets must be bounded by the workflow before a
repair loop is enabled.
