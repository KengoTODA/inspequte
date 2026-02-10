# inspequte

![inspequte logo](docs/logo.png)

[![CI](https://github.com/KengoTODA/inspequte/actions/workflows/ci.yml/badge.svg)](https://github.com/KengoTODA/inspequte/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/inspequte.svg)](https://crates.io/crates/inspequte)
![Gradle Plugin Portal Version](https://img.shields.io/gradle-plugin-portal/v/io.github.kengotoda.inspequte)
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
- No IDE integration required.
- Deterministic SARIF v2.1.0 output for LLM-friendly automation.

## Bytecode/JDK compatibility
- Supports JVM class files up to Java 21 (major version 65).
- Some advanced bytecode attributes may still be skipped in future releases.
- Some checks (such as the Prefer EnumSet rule for local variables) rely on the
  `LocalVariableTypeTable` attribute, which is only present when classes are
  compiled with debug symbols (for example, `javac -g`). Field and method
  signatures are still analyzed without debug info.

## Install
Install a pre-built binary from GitHub Releases:
- Linux (x86_64): `inspequte-<TAG>-x86_64-unknown-linux-gnu.tar.gz`
- Linux (ARM64): `inspequte-<TAG>-aarch64-unknown-linux-gnu.tar.gz`
- macOS (Apple Silicon): `inspequte-<TAG>-aarch64-apple-darwin.tar.gz`
- macOS (Intel): `inspequte-<TAG>-x86_64-apple-darwin.tar.gz`
- Windows (x86_64): `inspequte-<TAG>-x86_64-pc-windows-msvc.zip`

Example for Linux/macOS:
```bash
TAG="$(gh release list --repo KengoTODA/inspequte --exclude-drafts --exclude-pre-releases --limit 1 --json tagName --jq '.[0].tagName')"
TARGET="aarch64-apple-darwin" # use x86_64-apple-darwin on macOS Intel, aarch64-unknown-linux-gnu on Linux ARM64, x86_64-unknown-linux-gnu on Linux x86_64
curl -fL -o inspequte.tar.gz \
  "https://github.com/KengoTODA/inspequte/releases/download/${TAG}/inspequte-${TAG}-${TARGET}.tar.gz"
tar -xzf inspequte.tar.gz
chmod +x inspequte
sudo mv inspequte /usr/local/bin/inspequte
```

Example for Windows (PowerShell):
```powershell
$Tag = gh release list --repo KengoTODA/inspequte --exclude-drafts --exclude-pre-releases --limit 1 --json tagName --jq '.[0].tagName'
$Asset = "inspequte-$Tag-x86_64-pc-windows-msvc.zip"
Invoke-WebRequest -Uri "https://github.com/KengoTODA/inspequte/releases/download/$Tag/$Asset" -OutFile "inspequte.zip"
Expand-Archive -Path "inspequte.zip" -DestinationPath "."
# Move to a directory included in PATH
Move-Item ".\\inspequte.exe" "$HOME\\bin\\inspequte.exe" -Force
```

### macOS note (Gatekeeper for downloaded executables)
macOS can block directly executing binaries downloaded from the internet (Gatekeeper/quarantine behavior).
Follow Apple's official guidance to allow the executable:
- [Gatekeeper and runtime protection](https://support.apple.com/guide/security/gatekeeper-and-runtime-protection-sec5599b66df/web)
- [Open a Mac app from an unknown developer](https://support.apple.com/guide/mac-help/open-a-mac-app-from-an-unknown-developer-mh40616/mac)

For terminal tools, after confirming the binary source from the official release, you can remove the quarantine attribute:
```bash
xattr -d com.apple.quarantine /usr/local/bin/inspequte
```

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
Use the Gradle plugin:
```kotlin
plugins {
    id("java")
    id("io.github.kengotoda.inspequte") version "<VERSION>"
}

inspequte {
    // Optional: forward OTLP collector URL to inspequte via --otel
    otel.set("http://localhost:4318/v1/traces")
}

// Registered automatically:
// - writeInspequteInputs / writeInspequteInputsTest
// - inspequte / inspequteTest
// Each inspequte task emits:
// build/inspequte/<sourceSet>/report.sarif
```

The plugin hooks all generated `inspequte*` tasks into `check`.
The `inspequte` command must be available in `PATH`.
You can also pass the collector URL from CLI for a task run:
`./gradlew inspequte --inspequte-otel http://localhost:8080`

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
Use the Gradle plugin in CI and install the CLI from GitHub Releases:

```yaml
- name: Install inspequte
  uses: KengoTODA/setup-inspequte@8d212fa51a56245829f88e60f081c6549e312c57
- name: Setup Java
  uses: actions/setup-java@v5
  with:
    distribution: temurin
    java-version: "21"
- name: Setup Gradle
  uses: gradle/actions/setup-gradle@v4
- name: Run inspequte tasks
  run: ./gradlew check --no-daemon
- name: Upload SARIF to GitHub Code Scanning (optional)
  uses: github/codeql-action/upload-sarif@v4
  with:
    sarif_file: results.sarif
```

## License
AGPL-3.0. See `LICENSE`.
