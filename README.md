# inspequte

![inspequte logo](docs/logo.png)

[![CI](https://github.com/KengoTODA/inspequte/actions/workflows/ci.yml/badge.svg)](https://github.com/KengoTODA/inspequte/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/inspequte.svg)](https://crates.io/crates/inspequte)
[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL%203.0-blue.svg)](https://www.gnu.org/licenses/agpl-3.0.en.html)
[![Conventional Commits](https://img.shields.io/badge/Conventional%20Commits-1.0.0-yellow.svg)](https://www.conventionalcommits.org/en/v1.0.0/)

> [!WARNING]
> **Experimental / Proof-of-Concept Project**
>
> This repository is intended **for experimental and evaluation purposes only**.
> It is **not designed, reviewed, or supported for production use**.
>
> Do **NOT** use this code in production environments.

inspequte is a fast, CLI-first static analysis tool for JVM class and JAR files.
It focuses on CI/CD speed, deterministic output, and SARIF-only reporting for global
standard compatibility.

The name combines "inspect" and "cute". The CLI command is `inspequte`.

## Goals
- Fast startup and analysis for CI pipelines.
- No IDE or build-tool integration required.
- Deterministic SARIF v2.1.0 output for LLM-friendly automation.

## Bytecode/JDK compatibility
- Supports JVM class files up to Java 21 (major version 65).
- Requires a Java 21 toolchain when compiling test harness sources via `JAVA_HOME`.
- Some advanced bytecode attributes may still be skipped in future releases.

## CLI usage
```
inspequte --input app.jar --classpath lib/ --output results.sarif
```

Create a baseline of current findings to suppress them in future runs:
```
inspequte baseline --input app.jar --classpath lib/ --output inspequte.baseline.json
```

Run with a baseline to emit only new issues:
```
inspequte --input app.jar --classpath lib/ --output results.sarif --baseline inspequte.baseline.json
```
If you omit `--baseline` output/input paths, `.inspequte/baseline.json` is used by default; missing files are ignored.

You can read input or classpath lists from a file by prefixing the path with `@`.
The file format is one path per line; empty lines and lines starting with `#` are ignored.
```
inspequte --input @inputs.txt --classpath @classpath.txt --output results.sarif
```

## Gradle usage
Use a Gradle task to write the inputs and classpath to files, then reference them via `@`:
```kotlin
tasks.register("writeInspequteInputs") {
    dependsOn(tasks.named("classes"))
    inputs.files(sourceSets.main.get().output.classesDirs, configurations.runtimeClasspath)
    outputs.files(
        file("$buildDir/inspequte/inputs.txt"),
        file("$buildDir/inspequte/classpath.txt")
    )
    doLast {
        val inputsFile = file("$buildDir/inspequte/inputs.txt")
        val classpathFile = file("$buildDir/inspequte/classpath.txt")
        inputsFile.parentFile.mkdirs()
        inputsFile.writeText(sourceSets.main.get().output.classesDirs.files.joinToString("\n"))
        classpathFile.writeText(configurations.runtimeClasspath.get().files.joinToString("\n"))
    }
}

tasks.register<Exec>("inspequte") {
    dependsOn(tasks.named("writeInspequteInputs"))
    inputs.files(
        file("$buildDir/inspequte/inputs.txt"),
        file("$buildDir/inspequte/classpath.txt")
    )
    outputs.file(file("$buildDir/inspequte.sarif"))
    commandLine(
        "inspequte",
        "--input", "@$buildDir/inspequte/inputs.txt",
        "--classpath", "@$buildDir/inspequte/classpath.txt",
        "--output", "$buildDir/inspequte.sarif"
    )
}
```

## SARIF output (example)
```json
{
  "version": "2.1.0",
  "$schema": "https://schemastore.azurewebsites.net/schemas/json/sarif-2.1.0.json",
  "runs": [
    {
      "tool": {
        "driver": {
          "name": "inspequte",
          "informationUri": "https://github.com/KengoTODA/inspequte"
        }
      },
      "results": []
    }
  ]
}
```

## CI integration (GitHub Actions)
```yaml
- name: Install inspequte
  run: cargo install inspequte --locked
- name: Run inspequte
  run: |
    inspequte \
      --input app.jar \
      --classpath lib/ \
      --output results.sarif
```

### Upload SARIF to GitHub Code Scanning
```yaml
- name: Upload SARIF
  uses: github/codeql-action/upload-sarif@v3
  with:
    sarif_file: results.sarif
```

### Validate SARIF during CI (optional)
```yaml
- name: Run inspequte with schema validation
  run: |
    INSPEQUTE_VALIDATE_SARIF=1 inspequte \
      --input app.jar \
      --classpath lib/ \
      --output results.sarif
```

## License
AGPL-3.0. See `LICENSE`.

## Contributing
Please follow Conventional Commits 1.0.0. See `CONTRIBUTING.md`.
