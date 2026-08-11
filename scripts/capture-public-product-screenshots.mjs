#!/usr/bin/env node
import { createRequire } from "node:module";
import fs from "node:fs/promises";
import path from "node:path";

const require = createRequire(import.meta.url);
const { chromium } = require("../ui/node_modules/playwright");
const baseUrl = process.env.STRUCTTRACE_UI_URL;
if (!baseUrl) throw new Error("STRUCTTRACE_UI_URL is required");

const repositoryRoot = path.resolve(import.meta.dirname, "..");
const output = path.join(repositoryRoot, "site/public/screenshots");
await fs.mkdir(output, { recursive: true });

const browser = await chromium.launch({ headless: true });
const context = await browser.newContext({ viewport: { width: 1440, height: 1000 }, colorScheme: "light", deviceScaleFactor: 1 });
const page = await context.newPage();

async function capture(name) {
  await page.screenshot({ path: path.join(output, `${name}.png`), animations: "disabled" });
}

try {
  await page.goto(baseUrl, { waitUntil: "networkidle" });
  await capture("structtrace-welcome");

  await page.getByRole("button", { name: "Try invoice demo" }).click();
  await page.getByRole("heading", { name: "Should I ship?" }).waitFor({ timeout: 20_000 });
  await page.evaluate(() => window.scrollTo(0, 0));
  await capture("structtrace-results");

  await page.getByRole("button", { name: "Inspect regressions" }).click();
  await page.locator(".case-row").first().waitFor();
  await capture("structtrace-cases");
  await page.locator(".case-row").first().click();
  await page.getByRole("dialog", { name: /Case / }).waitFor();
  await capture("structtrace-case-detail");

  await page.getByRole("button", { name: "Close case" }).click();
  await page.getByRole("button", { name: "Toggle color theme" }).click();
  await page.getByRole("button", { name: "Overview" }).click();
  await page.getByRole("heading", { name: "Should I ship?" }).waitFor();
  await page.evaluate(() => window.scrollTo(0, 0));
  await capture("structtrace-results-dark");

  await page.goto(baseUrl, { waitUntil: "networkidle" });
  await page.getByRole("button", { name: "Compare a change" }).click();
  await page.getByRole("heading", { name: "What are you comparing?" }).waitFor();
  await capture("structtrace-source-setup");

  const files = page.locator('input[type="file"]');
  await files.nth(0).setInputFiles(path.join(repositoryRoot, "examples/document-extraction/data/golden.jsonl"));
  await files.nth(1).setInputFiles(path.join(repositoryRoot, "examples/document-extraction/outputs/baseline.jsonl"));
  await files.nth(2).setInputFiles(path.join(repositoryRoot, "examples/document-extraction/outputs/candidate.jsonl"));
  await files.nth(3).setInputFiles(path.join(repositoryRoot, "examples/document-extraction/schemas/output.schema.json"));
  await page.getByRole("button", { name: "Continue to field mapping" }).click();
  await page.locator(".coverage-card").first().waitFor();
  await capture("structtrace-field-mapping");
  await page.getByRole("button", { name: "Looks right" }).click();
  await page.getByRole("heading", { name: "What does correct mean for your application?" }).waitFor();
  await capture("structtrace-correctness-builder");
} finally {
  await context.close();
  await browser.close();
}
