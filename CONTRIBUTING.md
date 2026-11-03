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
- `INSPEQUTE_VALIDATE_SARIF=1` validates runtime output against the bundled official OASIS schema. SARIF tests enable validation automatically.

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

### Validate SARIF in custom integration jobs
```yaml
- name: Run inspequte tasks with schema validation
  run: |
    INSPEQUTE_VALIDATE_SARIF=1 inspequte \
      --input app.jar \
      --classpath lib/ \
      --output results.sarif
```

Run `scripts/update-sarif-schema.sh --check` to verify the vendored official
schema checksum without network access. See `docs/sarif.md` before refreshing it.

## GitHub Actions dependencies

All workflows, including the reusable release builders, are onboarded to
`.github/workflows/actions.lock`. The lockfile pins direct and transitive action
dependencies and records their source repository identities. Workflow references
use readable tags; their reviewed commits are recorded in the lockfile along
with nested actions.
For `dtolnay/rust-toolchain`, use the general-purpose action with an explicit
`toolchain: stable` input. The old `stable`-branch commit was no longer reachable
from upstream refs and failed lockfile provenance verification. The generator
resolved the replacement from `master` to its matching `v1` tag.

Install the official generator and run it from the repository root whenever you
add or remove a workflow or change a `uses:` reference:

```sh
gh extension install github/gh-actions-lock --pin v0.4.1
gh actions-lock
gh actions-lock --verify
```

Commit the generated lockfile and any workflow changes together. Never edit the
lockfile by hand. Review changes to commits as dependency updates, and investigate
any change to `owner_id` or `repo_id` before merging. `--verify-local` is a quick
offline coverage check; CI uses `--verify` to check upstream provenance as well.
Do not use `--accept-moved` in automation. Review upstream history before using
`--relock` to advance a moved reference.

Dependabot manages weekly GitHub Actions version updates and synchronizes the
lockfile in the same PR. Renovate continues to manage other dependencies and the
pinned lockfile verifier version, but its GitHub Actions manager is disabled to
avoid competing updates.

Lockfiles do not pin runner images, downloaded tools, container images, or calls
to external reusable workflows. Our same-repository release builders are
onboarded individually. If an external reusable workflow is added, pin its call
to a full commit SHA and confirm that its source repository locks its actions.

After rollout, a repository administrator can enable **Require lockfile** and
require the **Verify Actions lockfile / verify** check in the branch rules. Check
any allowed-actions policy first: onboarded actions are matched against locked
SHAs, so tag-only allowlist entries need adjustment.

See GitHub's [workflow dependency lockfiles](https://docs.github.com/en/actions/concepts/security/workflow-dependency-lockfiles),
[onboarding guide](https://docs.github.com/en/actions/how-tos/secure-your-work/lock-workflow-dependencies),
and the [official generator](https://github.com/github/gh-actions-lock).

## License
By contributing, you agree that your contributions will be licensed under AGPL-3.0.
