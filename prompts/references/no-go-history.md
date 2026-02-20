# Rule Ideation No-Go History

This reference is used by `prompts/ideate-rule.md` to avoid proposing duplicate or low-value rule ideas.
Append one entry each time verify returns `No-Go`.

## Entry format
- `rule-id`: snake_case identifier
- `rule idea`: short summary used in ideation
- `no-go reason`: concise reason summary from verify
- `run-url`: GitHub Actions run URL for traceability
- `status` (optional): implementation status, for example `implemented (2026-02-12)`
- `resolution-ref` (optional): commit hash or PR URL that resolved the No-Go
- `actions` (optional): short summary of remediation work and validation

## Entries

### mutate_unmodifiable_collection
- rule-id: `mutate_unmodifiable_collection`
- rule idea: Detect attempts to mutate collections that are known to be unmodifiable because they were created by JDK unmodifiable factories in the same method.
- no-go reason: build and test failures from missing opcode constants; no implementation/tests in verify-input to validate spec requirements
- run-url: https://github.com/KengoTODA/inspequte/actions/runs/21924738785

## 2026-02-12T00:38:17Z | return_in_finally
- rule-id: `return_in_finally`
- rule idea: Detect return statements inside finally blocks that override exceptions or prior returns.  
- no-go reason: The implementation emits the spec message text verbatim via `result_message("Return in finally overrides exceptions or prior returns. Move the return outside the finally block or return after the try/finally.")` in `src/rules/return_in_finally/mod.rs` within the change set. This matches the spec Output message requirement. Evidence: `verify-input/diff.patch`.
- run-url: https://github.com/KengoTODA/inspequte/actions/runs/21928662084
- status: implemented (2026-02-12)
- resolution-ref: https://github.com/KengoTODA/inspequte/pull/47
- actions: imported rule spec/plan/implementation from PR #47, fixed harness output type compatibility, registered `RETURN_IN_FINALLY`, and validated with `cargo fmt`, `cargo build`, `JAVA_HOME=<Java 21> cargo test`, and `cargo audit --format sarif`.

## 2026-02-18T22:12:20Z | deprecated_thread_control
- rule-id: `deprecated_thread_control`
- rule idea: Detect deprecated thread-control API calls (`Thread.stop/suspend/resume`) in analysis target classes.
- no-go reason: deprecated API usage is already surfaced by compiler warnings, so this rule was judged low-value and reverted by user decision.
- run-url: N/A (local user decision)
- status: abandoned (2026-02-18)
- actions: reverted all uncommitted rule files and registration/snapshot changes; recorded this entry to avoid re-proposing the same low-value idea.

## 2026-02-18T22:27:47Z | string_bytes_without_charset
- rule-id: `string_bytes_without_charset`
- rule idea: Detect default-charset String/byte conversion APIs (`String.getBytes()` and `new String(byte[])` without explicit charset).
- no-go reason: target projects assume JDK 18+ where default charset is UTF-8 by specification (JEP 400), so environment-dependent risk is low and rule value is insufficient.
- run-url: N/A (local user decision)
- status: abandoned (2026-02-18)
- actions: reverted all uncommitted rule files and registration/snapshot changes; documented rationale to avoid re-proposing this rule for JDK 18+ baseline projects.

## 2026-02-19T21:35:13Z | file_delete_result_ignored
- rule-id: `file_delete_result_ignored`
- rule idea: Detect `java.io.File.delete()` calls where the boolean result is ignored.
- no-go reason: ignoring the return value can be intentional in some cleanup paths, and the rule can become noisy when the developer explicitly chooses not to check deletion success.
- run-url: N/A (local user decision)
- status: abandoned (2026-02-19)
- actions: reverted all uncommitted rule files and registration/snapshot changes; recorded this entry to avoid re-proposing the same noisy direction.

## 2026-02-20T11:20:49Z | url_same_file_call
- rule-id: `url_same_file_call`
- rule idea: Detect `java.net.URL.sameFile(URL)` calls in analysis target classes.
- no-go reason: `sameFile(URL)` semantics intentionally compare the URL target identity, and this usage is considered explicit and acceptable by user policy.
- run-url: N/A (local user decision)
- status: abandoned (2026-02-20)
- actions: reverted all uncommitted rule files and registration/snapshot changes; recorded this entry to avoid re-proposing this direction.
