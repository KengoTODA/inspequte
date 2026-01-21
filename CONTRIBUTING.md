# Contributing

Thanks for your interest in inspequte.

## Commit messages
We follow Conventional Commits 1.0.0. Examples:
- `feat(cli): add sarif output`
- `fix(parser): handle invalid constant pool`
- `docs: update README`

## Development
- Run `cargo build` for a debug build.
- Run `cargo test` before submitting changes.

### Environment variables
- `INSPEQUTE_VALIDATE_SARIF=1` validates SARIF output against the bundled schema (dev only).

### Benchmarks
- `scripts/bench-classpath.sh <input> [repeat] [classpath...]` captures timing baselines for a single input.
- `scripts/bench-spotbugs.sh [repeat]` benchmarks SpotBugs libraries (downloads if needed).

### OpenTelemetry traces
Use `--otel <file>` to emit an OTLP/JSON trace file for performance analysis:
- `inspequte --input path/to.jar --otel /tmp/inspequte-trace.json`
- `inspequte baseline --input path/to.jar --otel /tmp/inspequte-trace.json`

The JSON includes spans tagged with `inspequte.rule_id` (per rule), `inspequte.class`,
and `inspequte.jar_path`/`inspequte.jar_entry`. For example, to inspect a specific rule:
- `jq '.resourceSpans[].scopeSpans[].spans[] | select(.attributes[]?.key=="inspequte.rule_id")' /tmp/inspequte-trace.json`
- `jq '.resourceSpans[].scopeSpans[].spans[] | select(.attributes[]?.value.stringValue=="SLF4J_PLACEHOLDER_MISMATCH")' /tmp/inspequte-trace.json`

### Rule authoring skill
When adding or updating rules, mention `rule-authoring` in your request to trigger the repo-scoped skill in `.codex/skills/rule-authoring`.
Example: `rule-authoring: add a rule to detect empty catch blocks`

## License
By contributing, you agree that your contributions will be licensed under AGPL-3.0.
