# Jaeger Screenshot Procedure

1. Resolve the target trace ID (for OSS FP hunt runs, use `target/oss-fp/report.md`).
2. Ensure Playwright runtime is installed once:
   - `npm install --no-save --no-package-lock playwright@1.53.0`
   - `npx playwright install --with-deps chromium`
3. Capture a full-page screenshot through the shared helper:
   - `JAEGER_OUT_DIR=target/oss-fp/jaeger node scripts/capture-jaeger-trace-screenshot.mjs <trace-id>`

The helper script clicks `Collapse all` once, then `Expand +1` twice before taking the screenshot, and falls back to the legacy `Collapse +1` flow across known XPath variants if the new sequence fails.
