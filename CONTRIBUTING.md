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
- Use Java 21 for the test harness by setting `JAVA_HOME` to a JDK 21 installation.
- Install Kotlin and ensure the `kotlinc` executable is available in `PATH` for Kotlin harness tests.

### Environment variables
- `INSPEQUTE_VALIDATE_SARIF=1` validates SARIF output against the bundled schema (dev only).

### Benchmarks
- `scripts/bench-classpath.sh <input> [repeat] [classpath...]` captures timing baselines for a single input.
- `scripts/bench-spotbugs.sh [repeat]` benchmarks SpotBugs libraries (downloads if needed).

### OpenTelemetry traces
Use `--otel <url>` to send OTLP traces over HTTP to a collector:
- `inspequte --input path/to.jar --otel http://localhost:4318/v1/traces`
- `inspequte baseline --input path/to.jar --otel http://localhost:4318/v1/traces`

The spans include attributes like `inspequte.rule_id`, `inspequte.class`, and
`inspequte.jar_path`/`inspequte.jar_entry` to help isolate slow rules. Use a
collector UI such as Jaeger to inspect the trace details.

### Rule authoring skill
When adding or updating rules, mention `rule-authoring` in your request to trigger the repo-scoped skill in `.codex/skills/rule-authoring`.
Example: `$rule-authoring add a rule to detect empty catch blocks`

### Validate SARIF during CI (optional)
```yaml
- name: Run inspequte tasks with schema validation
  run: |
    INSPEQUTE_VALIDATE_SARIF=1 inspequte \
      --input app.jar \
      --classpath lib/ \
      --output results.sarif
```

## License
By contributing, you agree that your contributions will be licensed under AGPL-3.0.
