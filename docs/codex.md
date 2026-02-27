# Codex Integration

This page explains how to use Codex to run `inspequte` and turn SARIF output
into actionable fixes.

## Prerequisites

- `inspequte` is installed and available in `PATH`.
- Your repository includes `AGENTS.md` with project constraints.
- You have target inputs ready (`.jar`, `.class`, and classpath paths).

## Basic prompt

```text
Run:
inspequte --input app.jar --classpath lib/ --output results.sarif

Then:
1) Summarize findings by rule ID.
2) Explain likely root causes.
3) Propose patch-ready fixes with file paths.
```

## Prompt with baseline

```text
Create baseline:
inspequte baseline --input app.jar --classpath lib/ --output inspequte.baseline.json

Run comparison:
inspequte --input app.jar --classpath lib/ --output results.sarif --baseline inspequte.baseline.json

Then report only newly introduced findings and propose fixes.
```

## Prompt for repository-wide analysis

```text
Run:
inspequte --input @inputs.txt --classpath @classpath.txt --output results.sarif

Then group findings by module/package and propose a fix plan in priority order.
```

## Prompt for selected-rule analysis

```text
Run:
inspequte --input app.jar --output results.sarif \
  --rules SYSTEM_EXIT,THREAD_RUN_DIRECT_CALL

Or with a file:
inspequte --input app.jar --output results.sarif --rules @rules.txt

Then summarize findings only for the selected rule IDs and propose fixes.
```

`rules.txt` format for `--rules @rules.txt`:
- one rule ID per line
- empty lines ignored
- lines starting with `#` ignored

## Tips

- Always tell Codex where to write SARIF (`--output`).
- Ask Codex to include rule IDs in summaries.
- Request patch-ready fixes, not only high-level advice.
- If you automate this in GitHub Actions, see `docs/github-actions.md`.
