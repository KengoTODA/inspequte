# Coding Agent Integration

This page explains how to use coding agents such as Codex, Claude Code, and
GitHub Copilot CLI to run `inspequte` and turn SARIF output into actionable fixes.

## Prerequisites

- `inspequte` is installed and available in `PATH`.
- Your repository includes `AGENTS.md` with project constraints.
- You have target inputs ready (`.jar`, `.class`, and classpath paths).

## Base workflow

Use the following prompt with your coding agent after running the CLI command.

```text
Read AGENTS.md.
Run:
inspequte --input app.jar --classpath lib/ --output results.sarif

Then:
1) Summarize findings by rule ID.
2) Explain likely root causes.
3) Propose patch-ready fixes with file paths.
```

Run the analysis in your terminal:

```text
inspequte --input app.jar --classpath lib/ --output results.sarif
```

Then ask your agent:

```text
Read AGENTS.md.
Open results.sarif.
1) Summarize findings by rule ID.
2) Explain likely root causes.
3) Propose patch-ready fixes with file paths.
```

## Baseline comparison

Run in your terminal:

```text
inspequte baseline --input app.jar --classpath lib/ --output inspequte.baseline.json
inspequte --input app.jar --classpath lib/ --output results.sarif --baseline inspequte.baseline.json
```

Then ask your agent:

```text
Read AGENTS.md.
Open results.sarif.
Report only newly introduced findings and propose fixes.
```

## Repository-wide analysis

Run in your terminal:

```text
inspequte --input @inputs.txt --classpath @classpath.txt --output results.sarif
```

Then ask your agent:

```text
Read AGENTS.md.
Open results.sarif.
Group findings by module/package and propose a fix plan in priority order.
```

## Selected-rule analysis

Run in your terminal:

```text
inspequte --input app.jar --output results.sarif \
  --rules SYSTEM_EXIT,THREAD_RUN_DIRECT_CALL
```

Then ask your agent:

```text
Read AGENTS.md.
Open results.sarif.
Summarize findings only for the selected rule IDs and propose fixes.
```

`rules.txt` format for `--rules @rules.txt`:
- one rule ID per line
- empty lines ignored
- lines starting with `#` ignored

## Tips

- Always specify where to write SARIF (`--output`).
- Ask for summaries that include rule IDs.
- Request patch-ready fixes, not only high-level advice.
- If you automate this in GitHub Actions, see `docs/github-actions.md`.
