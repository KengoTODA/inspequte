#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { chromium } from "playwright";

const traceId = process.argv[2];
if (!traceId) {
  console.error("usage: node scripts/capture-jaeger-trace-screenshot.mjs <trace-id>");
  process.exit(1);
}

const jaegerBaseUrl = (process.env.JAEGER_BASE_URL ?? "http://localhost:16686").replace(/\/$/, "");
const outDir = process.env.BENCH_OUT_DIR ?? "target/bench";
const outFile = path.join(outDir, `jaeger-trace-${traceId}.png`);
const traceUrl = `${jaegerBaseUrl}/trace/${traceId}`;
const primaryCollapseXPath =
  "//*[@id=\"jaeger-ui-root\"]/div/div/main/div/section/div/div[1]/div[1]/div/svg[2]";
const fallbackCollapseXPath =
  "(//*[@id=\"jaeger-ui-root\"]//main//*[contains(@class,\"TimelineCollapser\")]//*[name()=\"svg\"])[2]";

fs.mkdirSync(outDir, { recursive: true });

const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1920, height: 1080 } });

try {
  await page.goto(traceUrl, { waitUntil: "domcontentloaded", timeout: 60000 });
  await page.waitForTimeout(3000);

  const countVisibleSpans = async () =>
    page.evaluate(() => document.querySelectorAll("[role='switch']").length);

  const clickOnceByXPath = async (xpath) => {
    const locator = page.locator(`xpath=${xpath}`);
    if ((await locator.count()) === 0) {
      return false;
    }

    try {
      await locator.first().click({ timeout: 5000, force: true });
      return true;
    } catch {
      return false;
    }
  };

  const before = await countVisibleSpans();
  let clicked = await clickOnceByXPath(primaryCollapseXPath);
  if (!clicked) {
    clicked = await clickOnceByXPath(fallbackCollapseXPath);
  }

  await page.waitForTimeout(800);
  const after = await countVisibleSpans();
  if (!clicked || after >= before) {
    throw new Error(
      `Failed to collapse timeline once (before=${before}, after=${after}, primaryXPath=${primaryCollapseXPath})`,
    );
  }

  await page.waitForTimeout(1000);
  await page.screenshot({ path: outFile, fullPage: true });
} finally {
  await browser.close();
}

process.stdout.write(`${outFile}\n`);
