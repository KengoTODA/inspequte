---
name: jaeger-spotbugs-benchmark
description: Launch Jaeger, run SpotBugs benchmark traces, capture Jaeger UI screenshots, and report bottlenecks by rule, jar, and class for inspequte. Use when asked to profile `scripts/bench-spotbugs.sh`, inspect Jaeger traces, click `Collapse +1`, save screenshots under `target/bench`, and explain the slowest components.
---

# jaeger spotbugs benchmark

## Inputs
- Repository root with `scripts/bench-spotbugs.sh`.
- Docker available locally.
- Jaeger UI reachable at `http://localhost:16686`.
- OTLP HTTP endpoint `http://localhost:4318/v1/traces`.
- Execute all commands from repository root.

## Outputs
- Jaeger container running.
- Benchmark log in `target/bench/spotbugs.log`.
- Jaeger trace screenshot in `target/bench/`.
- Trace JSON export in `target/bench/`.
- Bottleneck report naming the slowest rule, jar, and class.

## Workflow
1. Start Jaeger:
   - Run `.codex/skills/jaeger-spotbugs-benchmark/scripts/start-jaeger.sh`.
2. Run benchmark with tracing enabled:
   - Run `.codex/skills/jaeger-spotbugs-benchmark/scripts/run-bench-spotbugs-with-otel.sh`.
   - Override repeat count with positional arg when needed.
   - Override endpoint with `OTEL_ENDPOINT` only when explicitly requested.
3. Open Jaeger UI and load latest trace:
   - Navigate to `http://localhost:16686/search`.
   - Set service to `inspequte`.
   - Run trace search and open the latest trace.
4. Expand timeline context for visibility:
   - Use this exact XPath as the primary locator for the `Collapse +1` button:
     - `//*[@id="jaeger-ui-root"]/div/div/main/div/section/div/div[1]/div[1]/div/svg[2]`
   - Click that element exactly once.
   - If that XPath is not present in the current Jaeger UI build, fall back to clicking the first visible control labeled `Collapse +` once.
5. Capture screenshot:
   - Save to `target/bench/jaeger-trace-<trace-id>.png`.
6. Export and analyze trace:
   - Run `.codex/skills/jaeger-spotbugs-benchmark/scripts/export-jaeger-trace.sh <trace-id>`.
   - Run `.codex/skills/jaeger-spotbugs-benchmark/scripts/analyze-trace-json.sh <trace-json-path>`.
7. Report bottleneck:
   - Include trace ID and screenshot path.
   - Identify one slowest rule, one slowest jar, and one slowest class.
   - Include concrete timing evidence from trace analysis.
   - Add a short investigation note tying screenshot timeline shape to the extracted slow spans.

## Reporting Template
Use this exact format:

```markdown
## Bottleneck Report
- Trace ID: <trace-id>
- Screenshot: target/bench/jaeger-trace-<trace-id>.png
- Slowest rule: <rule-id> (<total-ms> ms total across <span-count> spans)
- Slowest jar: <jar-path-or-name> (<total-ms> ms total across <span-count> spans)
- Slowest class: <class-name-or-entry> (<total-ms> ms total across <span-count> spans)
- Investigation:
  - Screenshot evidence: <what the expanded timeline shows>
  - Trace evidence: <why these three are the bottleneck>
  - Next action: <one concrete optimization direction>
```

## Guardrails
- Use the exact Jaeger container image: `jaegertracing/all-in-one:latest`.
- Keep screenshot files only under `target/bench`.
- Do not report a bottleneck without numeric timing evidence.
- Prefer Jaeger API export plus screenshot observation over guesswork.
