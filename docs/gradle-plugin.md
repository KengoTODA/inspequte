# Gradle Plugin

The inspequte Gradle plugin integrates static analysis into your Gradle build.
It registers analysis tasks for each Java source set and wires them into `check`
automatically.

## Prerequisites

- `inspequte` must be on your `PATH`.
  See [Getting Started](getting-started.md) for installation instructions.
- The `java` (or `java-base`) plugin must be applied to your project.

## Applying the plugin

```kotlin
plugins {
    id("java")
    id("io.github.kengotoda.inspequte") version "<VERSION>"
}
```

Replace `<VERSION>` with the latest version shown on the
[Gradle Plugin Portal](https://plugins.gradle.org/plugin/io.github.kengotoda.inspequte).

## Registered tasks

For each Java source set the plugin registers a task pair:

| Task | Description |
|---|---|
| `writeInspequteInputs<SourceSet>` | Writes input/classpath list files for analysis |
| `inspequte<SourceSet>` | Runs `inspequte` and produces a SARIF report |

The `<SourceSet>` part is the capitalized source set name: `Main` for the `main`
source set, `Test` for `test`, and so on.

For a standard Java project the registered tasks are:

```
writeInspequteInputsMain   inspequteMain
writeInspequteInputsTest   inspequteTest
```

Both task types belong to the `verification` group. All `inspequte*` tasks are
added as dependencies of `check`, so `./gradlew check` runs analysis
automatically.

### Output location

Each `inspequte<SourceSet>` task writes its SARIF report to:

```
build/inspequte/<sourceSetName>/report.sarif
```

For example, `inspequteMain` produces `build/inspequte/main/report.sarif`.

### Skipping when inspequte is not found

If the `inspequte` command is not available in `PATH` at task execution time the
task is skipped with a warning rather than failing the build:

```
Skipping 'inspequteMain': the 'inspequte' command is not available in PATH.
Install it with: cargo install inspequte --locked
```

## Extension configuration

Use the `inspequte` extension block to configure all tasks at once:

```kotlin
inspequte {
    // Forward an OTLP collector URL to inspequte via --otel
    otel.set("http://localhost:4318/")

    // Override the --automation-details-id prefix (task appends /<sourceSetName>)
    automationDetailsIdPrefix.set("inspequte/custom-path")

    // Warn instead of failing when the same class name appears in multiple inputs
    allowDuplicateClasses.set(true)

    // Run only selected rules (repeatable, supports comma-separated IDs and @file references)
    rules.set(listOf("SYSTEM_EXIT,THREAD_RUN_DIRECT_CALL", "@rules.txt"))
}
```

### `otel`

Optional. When set, the value is passed to the CLI as `--otel <url>`.
Useful for exporting OpenTelemetry trace data during analysis.

### `automationDetailsIdPrefix`

Optional. Sets the SARIF `run.automationDetails.id` prefix.
Each task appends `/<sourceSetName>` to this value, so the full ID becomes
`<prefix>/<sourceSetName>`.

When not set, the default is derived from the project's path relative to the
root project:

```
inspequte/<relative-project-path>/<sourceSetName>
```

For a single-project build the path is `.`, producing IDs like
`inspequte/./<sourceSetName>`.

### `allowDuplicateClasses`

Optional. Defaults to `false`.

When `true`, passes `--allow-duplicate-classes` to the CLI. If the same class
name is found in more than one input artifact the tool emits a warning instead
of failing, and always picks the class from the artifact whose path is
lexicographically smallest, ensuring deterministic results. Set this to `true`
when your build produces shadow JARs, shaded dependencies, or other scenarios
that intentionally place the same class in multiple inputs.

### `rules`

Optional. When set, forwards each entry to the CLI as `--rules <value>`.
Each value may be a comma-separated list of rule IDs or an `@file` reference
(one rule ID per line).

## Per-run CLI overrides

You can override task properties for a single Gradle invocation using task
options on the command line:

```bash
# Override the OTLP collector URL for a single run
./gradlew inspequteMain --inspequte-otel http://localhost:8080

# Override the automation details ID for a single run
./gradlew inspequteMain --inspequte-automation-details-id "inspequte/override/main"

# Warn instead of failing on duplicate class names for a single run
./gradlew inspequteMain --inspequte-allow-duplicate-classes

# Run only selected rules for a single run (repeatable)
./gradlew inspequteMain --inspequte-rules SYSTEM_EXIT,THREAD_RUN_DIRECT_CALL --inspequte-rules @rules.txt
```

These flags take precedence over values set in the `inspequte` extension block.

## Multi-project builds

In a multi-project build each subproject applies the plugin independently.
The default `automationDetailsIdPrefix` includes the subproject path relative to
the root, so SARIF reports from different subprojects have distinct IDs and do
not clash in Code Scanning dashboards.

For example, a subproject at `services/api` produces IDs like
`inspequte/services/api/main`.

## Next steps

- [GitHub Actions](github-actions.md) — upload SARIF reports to GitHub Code Scanning
- [Rules](rules/index.md) — browse the available analysis rules
