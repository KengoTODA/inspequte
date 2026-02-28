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
const outDir = process.env.JAEGER_OUT_DIR ?? process.env.BENCH_OUT_DIR ?? "target/bench";
const outFile = path.join(outDir, `jaeger-trace-${traceId}.png`);
const traceUrl = `${jaegerBaseUrl}/trace/${traceId}`;
const firstTimelineCollapserXPath =
  "(//*[@id='jaeger-ui-root']//main//*[contains(@class,'TimelineCollapser')])[1]";
const collapseAllXPath = `${firstTimelineCollapserXPath}//*[name()='svg'][4]`;
const expandPlusOneXPath = `${firstTimelineCollapserXPath}//*[name()='svg'][1]`;
const legacyPrimaryCollapseXPath =
  "//*[@id=\"jaeger-ui-root\"]/div/div/main/div/section/div/div[1]/div[1]/div/svg[2]";
const legacySiblingAfterExpandXPath =
  "(//*[@id='jaeger-ui-root']//*[contains(@class,'TimelineCollapser--btn-expand')])[1]/following-sibling::*[contains(@class,'TimelineCollapser--btn')][1]";
const legacyFallbackCollapseXPath =
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
      return { ok: false, reason: "missing" };
    }

    try {
      await locator.first().click({ timeout: 5000, force: true });
      return { ok: true };
    } catch {
      return { ok: false, reason: "click-error" };
    }
  };

  const before = await countVisibleSpans();
  const newSequenceAttempts = [];
  const newSequence = [
    { step: "collapse-all", xpath: collapseAllXPath },
    { step: "expand-plus-one-1", xpath: expandPlusOneXPath },
    { step: "expand-plus-one-2", xpath: expandPlusOneXPath },
  ];
  let newSequenceSuccess = true;
  for (const { step, xpath } of newSequence) {
    const result = await clickOnceByXPath(xpath);
    newSequenceAttempts.push({ step, xpath, ...result });
    if (!result.ok) {
      newSequenceSuccess = false;
      break;
    }
    await page.waitForTimeout(300);
  }

  await page.waitForTimeout(800);
  const afterNewSequence = await countVisibleSpans();
  if (newSequenceSuccess && afterNewSequence >= before) {
    newSequenceSuccess = false;
    newSequenceAttempts.push({
      step: "new-sequence-count-check",
      xpath: "n/a",
      ok: false,
      reason: "no-reduction",
    });
  }

  if (!newSequenceSuccess) {
    const beforeLegacy = await countVisibleSpans();
    const legacyAttempts = [];
    const legacyXPaths = [
      legacyPrimaryCollapseXPath,
      legacySiblingAfterExpandXPath,
      legacyFallbackCollapseXPath,
    ];
    let legacyClicked = false;
    for (const xpath of legacyXPaths) {
      const result = await clickOnceByXPath(xpath);
      legacyAttempts.push({ xpath, ...result });
      if (result.ok) {
        legacyClicked = true;
        break;
      }
    }

    await page.waitForTimeout(800);
    const afterLegacy = await countVisibleSpans();
    const legacySuccess = legacyClicked && afterLegacy < beforeLegacy;
    if (!legacySuccess) {
      const details = JSON.stringify({
        before,
        afterNewSequence,
        beforeLegacy,
        afterLegacy,
        newSequenceAttempts,
        legacyAttempts,
      });
      throw new Error(`Failed timeline controls for screenshot (new-sequence + legacy failed): ${details}`);
    }
  }

  const after = await countVisibleSpans();
  if (after >= before) {
    const details = JSON.stringify({
      before,
      after,
      newSequenceAttempts,
    });
    throw new Error(
      `Timeline controls did not reduce visible spans after attempts (new-sequence or legacy may have run): ${details}`,
    );
  }

  await page.waitForTimeout(1000);
  await page.screenshot({ path: outFile, fullPage: true });
} finally {
  await browser.close();
}

process.stdout.write(`${outFile}\n`);
