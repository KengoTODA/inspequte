# Getting Started

This guide shows how to install `inspequte` and run your first scan.

## 1. Install `inspequte`

### Option A: Homebrew (macOS/Linux)

```bash
brew install KengoTODA/tap/inspequte
```

### Option B: Pre-built binary (all major platforms)

1. Open the [GitHub Releases page](https://github.com/KengoTODA/inspequte/releases).
2. Download the archive for your OS/CPU:
   - Linux (x86_64): `inspequte-<TAG>-amd64-unknown-linux-gnu.tar.gz`
   - Linux (ARM64): `inspequte-<TAG>-arm64-unknown-linux-gnu.tar.gz`
   - macOS (Apple Silicon): `inspequte-<TAG>-arm64-apple-darwin.tar.gz`
   - macOS (Intel): `inspequte-<TAG>-amd64-apple-darwin.tar.gz`
   - Windows (x86_64): `inspequte-<TAG>-amd64-pc-windows-msvc.zip`
   (`TAG` is the GitHub release tag, for example `inspequte-v0.15.1`.)
3. Extract it and place `inspequte` (or `inspequte.exe`) in a directory on your `PATH`.

## 2. Verify installation

```bash
inspequte --version
inspequte --help
```

If both commands work, installation is complete.

## 3. First invocation (basic scan)

Run `inspequte` against your application JAR/class files:

```bash
inspequte --input app.jar --classpath lib/ --output results.sarif
```

Arguments:
- `--input`: target class/JAR files to analyze
- `--classpath`: dependency jars/directories used for type resolution
- `--output`: output SARIF file path

`inspequte` always writes SARIF v2.1.0 output.

## 4. Optional: baseline workflow

Create a baseline from current findings:

```bash
inspequte baseline --input app.jar --classpath lib/ --output inspequte.baseline.json
```

Later, compare against that baseline to report only newly introduced findings:

```bash
inspequte --input app.jar --classpath lib/ --output results.sarif --baseline inspequte.baseline.json
```

## 5. Next step

Browse available checks in [Rules](rules/index.md).
