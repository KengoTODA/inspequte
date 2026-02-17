---
name: inspequte-oss-fp-hunt
description: Run inspequte false-positive hunting against small OSS libraries (Kotlin and Java+Guava), enable inspequte-gradle-plugin in cloned fixtures, capture Jaeger trace screenshots via the shared `scripts/capture-jaeger-trace-screenshot.mjs` helper, and triage SARIF findings one by one.
---

# inspequte OSS FP hunt

## Inputs
- Repository root of this project (contains `gradle-plugin/` and `target/debug/inspequte`).
- Docker available for Jaeger.
- Java 21 available and set via `JAVA_HOME`.
- Network access to clone OSS fixtures from GitHub.
- Node.js + npm available for screenshot capture.

## Outputs
- Fixture clones and temporary patches under `target/oss-fp/workdir/`.
- SARIF reports under `target/oss-fp/<fixture>/inspequte/`.
- Triage ledgers under `target/oss-fp/triage/`.
- Run summary in `target/oss-fp/report.md`.
- Jaeger screenshot and trace analysis under `target/oss-fp/jaeger/`.

## Fixture policy
Default fixture set:
- Kotlin fixture: `plasmoapp/plasmo-config`
- Java+Guava fixture: `launchdarkly/okhttp-eventsource`

If one fixture is not buildable, replace it with another thin OSS fixture of the same category and record the reason in `target/oss-fp/report.md`.

## Workflow
1. Execute `.codex/skills/inspequte-oss-fp-hunt/scripts/run-once.sh` from repository root.
2. The script clones fixtures, enables `io.github.kengotoda.inspequte` with local `includeBuild`, runs analysis tasks, exports SARIF, and writes triage ledgers with first-pass `TP`/`FP`/`FAILED` classification plus reason and `spec=...` reference.
3. Export trace JSON and resolve trace ID:
   - `trace_json="$(JAEGER_OUT_DIR=target/oss-fp/jaeger .codex/skills/jaeger-spotbugs-benchmark/scripts/export-jaeger-trace.sh)"`
   - `trace_id="$(basename "${trace_json}" .json)"`
   - `trace_id="${trace_id#jaeger-trace-}"`
4. Prepare Playwright runtime once per environment:
   - `npm install --no-save --no-package-lock playwright@1.53.0`
   - `npx playwright install --with-deps chromium`
5. Capture screenshot via shared script:
   - `screenshot_png="$(JAEGER_OUT_DIR=target/oss-fp/jaeger node scripts/capture-jaeger-trace-screenshot.mjs "${trace_id}")"`
6. Analyze trace:
   - `.codex/skills/jaeger-spotbugs-benchmark/scripts/analyze-trace-json.sh "${trace_json}"`
7. Review findings one-by-one in each triage ledger and adjust status only when code evidence contradicts the first-pass classification.

## Finding triage rules
First-pass automated classification in `run-once.sh`:
- `FP` when file is under `/.gradle/caches/modules-2/` (classpath dependency out of scope).
- `FAILED` when `line=0` (non-actionable location; requires manual rule/location handling).
- `FP` when duplicated by exact key `(rule, file, line, message)` after first occurrence.
- Other findings are marked `FAILED` with `spec=<path>` to the corresponding rule spec file.

Manual review policy:
- Confirm referenced file/line exists in fixture source.
- Compare finding message and source code against the referenced `spec.md`.
- Resolve `FAILED` items first, then override status only with short code-backed reason.

## Guardrails
- Do not edit fixture repositories outside `target/oss-fp/workdir/`.
- Keep all artifacts under `target/oss-fp/`.
- Do not report performance bottlenecks without numeric evidence from trace summary.
- Keep screenshot files only under `target/oss-fp/jaeger/`.
