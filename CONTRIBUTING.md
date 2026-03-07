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
- `scripts/bench-classpath.sh <input> [repeat] [classpath...]` runs benchmark scans for a single input.
- `scripts/bench-spotbugs.sh [repeat]` benchmarks `spotbugs.jar` (downloads SpotBugs if needed).

### OpenTelemetry traces
Use `--otel <url>` to send OTLP traces over HTTP to a collector:
- `inspequte --input path/to.jar --otel http://localhost:4318/`
- `inspequte baseline --input path/to.jar --otel http://localhost:4318/`

The spans include attributes like `inspequte.rule_id`, `inspequte.class`, and
`inspequte.jar_path`/`inspequte.jar_entry` to help isolate slow rules. Use a
collector UI such as Jaeger to inspect the trace details.

### Rule workflow skills
When adding or updating rules, use the local workflow skills in `.codex/skills/`:
- `inspequte-rule-plan`
- `inspequte-rule-spec`
- `inspequte-rule-impl`
- `inspequte-rule-verify`

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
