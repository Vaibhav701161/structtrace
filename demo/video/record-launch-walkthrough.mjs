#!/usr/bin/env node
import { createRequire } from "node:module";
import fs from "node:fs/promises";
import path from "node:path";

const require = createRequire(import.meta.url);
const { chromium } = require("../../ui/node_modules/playwright");

const url = process.env.STRUCTTRACE_UI_URL;
if (!url) throw new Error("STRUCTTRACE_UI_URL is required");
const output = path.resolve(process.argv[2] ?? "demo/video/generated/launch");
await fs.mkdir(output, { recursive: true });

const browser = await chromium.launch({ headless: true });
const context = await browser.newContext({
  viewport: { width: 2560, height: 1440 },
  deviceScaleFactor: 1,
  colorScheme: "dark",
  recordVideo: { dir: output, size: { width: 2560, height: 1440 } },
});
const page = await context.newPage();

async function installPointer() {
  await page.evaluate(() => {
    const pointer = document.createElement("div");
    pointer.id = "launch-pointer";
    Object.assign(pointer.style, {
      position: "fixed", zIndex: "2147483647", width: "18px", height: "18px",
      border: "2px solid #fffdf8", borderRadius: "50%", background: "#95502f",
      boxShadow: "0 2px 12px rgba(0,0,0,.38)", pointerEvents: "none",
      transform: "translate(80px,80px)", transition: "transform 420ms cubic-bezier(.2,.8,.2,1)",
    });
    document.body.append(pointer);
  });
}

async function point(locator, settle = 550) {
  const box = await locator.boundingBox();
  if (!box) throw new Error("Walkthrough target is not visible");
  const x = Math.round(box.x + Math.min(box.width - 8, Math.max(8, box.width * .62)));
  const y = Math.round(box.y + box.height * .52);
  await page.evaluate(({ x, y }) => {
    const pointer = document.querySelector("#launch-pointer");
    if (pointer instanceof HTMLElement) pointer.style.transform = `translate(${x}px,${y}px)`;
  }, { x, y });
  await page.waitForTimeout(settle);
}

async function click(locator, pause = 900) {
  await point(locator);
  await locator.click();
  await page.waitForTimeout(pause);
}

const exactRow = (value) => JSON.stringify(value)
  .replace('"__BIG__"', "9007199254740993")
  .replace('"__DECIMAL__"', "0.12345678901234567890123456789");
const dataset = Array.from({ length: 100 }, (_, index) => exactRow({
  id: `launch-${String(index).padStart(3, "0")}`,
  input: { request: `Resolve account ${index}`, amount: "__BIG__" },
  expected: { answer: index, confidence: "__DECIMAL__" },
})).join("\n") + "\n";
const outputs = (candidate) => Array.from({ length: 100 }, (_, index) => exactRow({
  id: `launch-${String(index).padStart(3, "0")}`,
  status: "ok",
  output: { answer: index === (candidate ? 73 : 12) ? index + 1 : index, confidence: "__DECIMAL__" },
})).join("\n") + "\n";
const schema = JSON.stringify({
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  type: "object",
  properties: { answer: { type: "integer" }, confidence: { type: "number" } },
  required: ["answer", "confidence"], additionalProperties: false,
});

try {
  await page.goto(url, { waitUntil: "networkidle" });
  await installPointer();
  await page.waitForTimeout(1800);
  await click(page.getByRole("button", { name: "Compare a change" }), 1100);
  const inputs = page.locator('input[type="file"]');
  await inputs.nth(0).setInputFiles({ name: "launch-golden.jsonl", mimeType: "application/x-ndjson", buffer: Buffer.from(dataset) });
  await inputs.nth(1).setInputFiles({ name: "launch-baseline.jsonl", mimeType: "application/x-ndjson", buffer: Buffer.from(outputs(false)) });
  await inputs.nth(2).setInputFiles({ name: "launch-candidate.jsonl", mimeType: "application/x-ndjson", buffer: Buffer.from(outputs(true)) });
  await inputs.nth(3).setInputFiles({ name: "launch-contract.schema.json", mimeType: "application/json", buffer: Buffer.from(schema) });
  await page.getByRole("button", { name: "Continue to field mapping" }).waitFor({ state: "visible" });
  await page.waitForTimeout(1800);
  await click(page.getByRole("button", { name: "Continue to field mapping" }), 1400);
  await page.waitForTimeout(1200);
  await click(page.getByRole("button", { name: "Looks right" }), 1300);
  await page.getByText(/Rust analyzed all 100 expected/).waitFor({ state: "visible" });
  await page.waitForTimeout(1800);
  await click(page.getByRole("checkbox", { name: "Use /answer" }), 700);
  await click(page.getByRole("checkbox", { name: "Use /confidence" }), 700);
  await click(page.getByRole("button", { name: "Continue" }), 1100);
  await click(page.getByRole("button", { name: /Release decision/ }), 700);
  await click(page.getByRole("button", { name: "Continue" }), 900);
  await page.getByLabel("Comparison name").fill("Launch candidate: account extraction contract");
  await page.getByLabel("Baseline").fill("Production v4");
  await page.getByLabel("Candidate").fill("Contract refactor v5");
  await page.waitForTimeout(1000);
  await click(page.getByRole("button", { name: "Run comparison" }), 800);
  await page.getByRole("heading", { name: "RELEASE AUTHORIZED" }).waitFor({ state: "visible", timeout: 30000 });
  await page.waitForTimeout(3500);
  await page.mouse.wheel(0, 760);
  await page.waitForTimeout(2300);
  await page.mouse.wheel(0, 780);
  await page.waitForTimeout(2300);
  await click(page.getByRole("button", { name: "Inspect cases" }).first(), 1200);
  await page.locator(".case-row").first().waitFor({ state: "visible" });
  await click(page.locator(".case-row").first(), 1600);
  await page.getByRole("dialog", { name: /Case launch-073/ }).waitFor({ state: "visible" });
  await page.waitForTimeout(3000);
  await click(page.getByRole("button", { name: /Copy exact evidence/ }), 700);
  await click(page.getByRole("button", { name: "Close case" }), 900);
  await click(page.getByRole("button", { name: "Overview" }), 900);
  await page.evaluate(() => window.scrollTo({ top: document.body.scrollHeight, behavior: "instant" }));
  await page.waitForTimeout(800);
  await click(page.getByRole("button", { name: "Accept as next baseline" }), 1200);
  await page.getByText("Verified baseline revision committed").waitFor({ state: "visible" });
  await page.waitForTimeout(2300);
  await click(page.getByRole("button", { name: "Export CI project" }), 1100);
  await click(page.getByRole("button", { name: "Export complete CI project" }), 1300);
  await page.getByText("Complete CI project exported").waitFor({ state: "visible" });
  await page.waitForTimeout(2800);
} finally {
  const video = page.video();
  await context.close();
  await browser.close();
  if (video) {
    const raw = await video.path();
    await fs.rename(raw, path.join(output, "structtrace-launch-walkthrough.webm"));
  }
}
