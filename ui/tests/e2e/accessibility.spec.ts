import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";
import { execFileSync } from "node:child_process";

test("welcome screen has no detectable WCAG A/AA violations", async ({ page }) => {
  await page.goto(process.env.STRUCTTRACE_UI_URL ?? "/");
  await expect(page.getByRole("heading", { name: /Your schema passed/i })).toBeVisible();
  const results = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa"]).analyze();
  expect(results.violations).toEqual([]);
});

test("invoice demo reaches an honest evidence decision", async ({ page }) => {
  await page.goto(process.env.STRUCTTRACE_UI_URL ?? "/");
  await page.getByRole("button", { name: "Try invoice demo" }).click();
  await expect(page.getByRole("heading", { name: "Should I ship?" })).toBeVisible({ timeout: 20_000 });
  await expect(page.getByRole("heading", { name: "NOT ENOUGH EVIDENCE" })).toBeVisible();
  await expect(page.getByText("Regression gate", { exact: true })).toBeVisible();
  const results = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa"]).analyze();
  expect(results.violations).toEqual([]);
  await page.getByRole("button", { name: "Inspect regressions" }).click();
  await expect(page.getByText("3 cases", { exact: true })).toBeVisible();
  const firstCase = page.locator(".case-row").first();
  await expect(firstCase).toBeVisible();
  const caseId = await firstCase.locator("code").first().textContent();
  await firstCase.click();
  const drawer = page.getByRole("dialog", { name: /Case / });
  await drawer.getByRole("button", { name: "Save case", exact: true }).click();
  await expect(drawer.getByRole("button", { name: "Saved", exact: true })).toBeVisible();
  await drawer.getByRole("button", { name: "Close case" }).click();
  await page.getByRole("link", { name: "Saved cases" }).click();
  const pinnedRow = page.locator(".pinned-list > div").filter({ hasText: caseId ?? "missing-case-id" }).first();
  await expect(pinnedRow).toBeVisible();
  await pinnedRow.getByRole("button", { name: "Open latest evidence" }).click();
  await expect(page.getByRole("dialog", { name: `Case ${caseId}` })).toBeVisible();
});

test("new comparison begins with the recorded-output workflow", async ({ page }) => {
  await page.goto(process.env.STRUCTTRACE_UI_URL ?? "/");
  await page.getByRole("button", { name: "Compare a change" }).click();
  await expect(page.getByRole("heading", { name: "What are you comparing?" })).toBeVisible();
  await expect(page.getByRole("button", { name: /Recorded outputs/ })).toBeVisible();
  await expect(page.getByRole("button", { name: "Continue to field mapping" })).toBeVisible();
});

test("recorded files complete the visual workflow through the Rust engine", async ({ page }) => {
  await page.goto(process.env.STRUCTTRACE_UI_URL ?? "/");
  await page.getByRole("button", { name: "Compare a change" }).click();
  const savedDraft = page.waitForRequest((request) => request.method() === "PUT" && request.url().includes("/comparisons/draft") && request.postData()?.includes("sourceId") === true);
  const files = page.locator('input[type="file"]');
  await files.nth(0).setInputFiles("../examples/document-extraction/data/golden.jsonl");
  await files.nth(1).setInputFiles("../examples/document-extraction/outputs/baseline.jsonl");
  await files.nth(2).setInputFiles("../examples/document-extraction/outputs/candidate.jsonl");
  await files.nth(3).setInputFiles("../examples/document-extraction/schemas/output.schema.json");
  const draftRequest = await savedDraft;
  expect(draftRequest.postData()).not.toContain('"content"');
  await page.getByRole("button", { name: "Continue to field mapping" }).click();
  await expect(page.locator(".coverage-card").getByText("matched cases")).toBeVisible();
  await expect(page.locator(".coverage-card strong").first()).toHaveText("12");
  await page.getByRole("button", { name: "Looks right" }).click();
  await expect(page.getByRole("heading", { name: "What does correct mean for your application?" })).toBeVisible();
  await page.getByRole("checkbox", { name: "Use /line_items", exact: true }).check();
  await expect(page.getByRole("heading", { name: "Match items in /line_items" })).toBeVisible();
  await expect(page.getByText("Match key required", { exact: true })).toBeVisible();
  await page.getByRole("combobox", { name: "Role for /description" }).selectOption("key");
  await expect(page.getByRole("combobox", { name: "Role for /description" })).toHaveValue("key");
  await expect(page.getByRole("combobox", { name: "Role for /unit_price" })).not.toHaveValue("key");
  await expect(page.getByText("Pairs the same item across variants").first()).toBeVisible();
  await page.getByRole("button", { name: "Continue" }).click();
  await page.getByRole("button", { name: "Continue" }).click();
  await expect(page.getByRole("heading", { name: "Ready to compare" })).toBeVisible();
  await page.getByRole("button", { name: "Run comparison" }).click();
  await expect(page.getByRole("heading", { name: "Should I ship?" })).toBeVisible({ timeout: 20_000 });
  await expect(page.getByRole("heading", { name: "NOT ENOUGH EVIDENCE" })).toBeVisible();
});

test("capability deep links refresh with correctly typed assets", async ({ page, request }) => {
  await page.goto(process.env.STRUCTTRACE_UI_URL ?? "/");
  await page.getByRole("button", { name: "Try invoice demo" }).click();
  await expect(page.getByRole("heading", { name: "Should I ship?" })).toBeVisible({ timeout: 20_000 });
  const directUrl = page.url();
  expect(new URL(directUrl).pathname).toMatch(/\/runs\/[^/]+$/);
  await page.reload();
  await expect(page.getByRole("heading", { name: "Should I ship?" })).toBeVisible();
  const capability = new URL(directUrl).pathname.split("/").filter(Boolean)[0];
  for (const [asset, mime] of [["app.js", "javascript"], ["app.css", "text/css"], ["structtrace-logo-mark.svg", "image/svg+xml"]] as const) {
    const response = await request.get(`${new URL(directUrl).origin}/${capability}/assets/${asset}`);
    expect(response.ok()).toBeTruthy();
    expect(response.headers()["content-type"]).toContain(mime);
  }
  expect((await request.get(`${new URL(directUrl).origin}/${capability}/assets/missing.js`)).status()).toBe(404);
});

test("mobile navigation keeps every primary route reachable", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto(process.env.STRUCTTRACE_UI_URL ?? "/");
  await page.getByRole("button", { name: "Try invoice demo" }).click();
  await expect(page.getByRole("heading", { name: "Should I ship?" })).toBeVisible({ timeout: 20_000 });
  await page.getByRole("button", { name: "Open navigation" }).click();
  await expect(page.getByRole("dialog", { name: "Mobile navigation" })).toBeVisible();
  await page.getByRole("link", { name: "Saved cases" }).click();
  await expect(page.getByRole("heading", { name: "Saved cases", exact: true })).toBeVisible();
});

test("dark theme and command search remain accessible", async ({ page }) => {
  await page.goto(process.env.STRUCTTRACE_UI_URL ?? "/");
  await page.getByRole("button", { name: "Try invoice demo" }).click();
  await expect(page.getByRole("heading", { name: "Should I ship?" })).toBeVisible({ timeout: 20_000 });
  await page.getByRole("button", { name: "Toggle color theme" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await page.getByRole("button", { name: /Search or run a command/ }).click();
  const palette = page.getByRole("dialog", { name: "Command palette" });
  await palette.getByRole("textbox", { name: "Search commands" }).fill("CI");
  await expect(palette.getByRole("button", { name: /Export CI project/ })).toBeVisible();
  await expect(palette.getByRole("button", { name: /New comparison/ })).toBeHidden();
  const results = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa"]).analyze();
  expect(results.violations).toEqual([]);
});

test("authorized candidate becomes the next baseline in the same project", async ({ page }, testInfo) => {
  const projectName = `E2E authorized lifecycle ${testInfo.project.name} ${Date.now()}`;
  const dataset = Array.from({ length: 100 }, (_, index) => JSON.stringify({ id: `case-${index}`, input: { value: index }, expected: { answer: index } })).join("\n") + "\n";
  const outputs = Array.from({ length: 100 }, (_, index) => JSON.stringify({ id: `case-${index}`, status: "ok", output: { answer: index } })).join("\n") + "\n";
  const schema = JSON.stringify({ "$schema": "https://json-schema.org/draft/2020-12/schema", type: "object", properties: { answer: { type: "integer" } }, required: ["answer"], additionalProperties: false });
  await page.goto(process.env.STRUCTTRACE_UI_URL ?? "/");
  await page.getByRole("button", { name: "Compare a change" }).click();
  const files = page.locator('input[type="file"]');
  await files.nth(0).setInputFiles({ name: "release-dataset.jsonl", mimeType: "application/x-ndjson", buffer: Buffer.from(dataset) });
  await files.nth(1).setInputFiles({ name: "release-baseline.jsonl", mimeType: "application/x-ndjson", buffer: Buffer.from(outputs) });
  await files.nth(2).setInputFiles({ name: "release-candidate.jsonl", mimeType: "application/x-ndjson", buffer: Buffer.from(outputs) });
  await files.nth(3).setInputFiles({ name: "answer.schema.json", mimeType: "application/json", buffer: Buffer.from(schema) });
  await page.getByRole("button", { name: "Continue to field mapping" }).click();
  await page.getByRole("button", { name: "Looks right" }).click();
  await page.getByRole("checkbox", { name: "Use /answer" }).check();
  await page.getByRole("button", { name: "Continue" }).click();
  await page.getByRole("button", { name: /Release decision/ }).click();
  await page.getByRole("button", { name: "Continue" }).click();
  await page.getByLabel("Comparison name").fill(projectName);
  await page.getByRole("button", { name: "Run comparison" }).click();
  await expect(page.getByRole("heading", { name: "RELEASE AUTHORIZED" })).toBeVisible({ timeout: 20_000 });
  const resultUrl = page.url();
  const runId = new URL(resultUrl).pathname.split("/").at(-1);
  await page.getByRole("button", { name: "Export CI project" }).click();
  const originalCiTarget = new URL(page.url());
  const originalProjectId = originalCiTarget.searchParams.get("project");
  expect(originalProjectId).toBeTruthy();
  expect(originalCiTarget.searchParams.get("run")).toBe(runId);
  await page.goto(resultUrl);
  await page.getByRole("button", { name: "New comparison" }).click();
  await expect(page.getByRole("heading", { name: "What are you comparing?" })).toBeVisible();
  await page.goto(resultUrl);
  await page.getByRole("button", { name: "Export CI project" }).click();
  const historicalCiTarget = new URL(page.url());
  expect(historicalCiTarget.searchParams.get("project")).toBe(originalProjectId);
  expect(historicalCiTarget.searchParams.get("run")).toBe(runId);
  await page.goto(resultUrl);
  await page.getByRole("button", { name: "Accept as next baseline" }).click();
  await expect(page.getByText("Verified baseline revision committed")).toBeVisible();
  await page.getByRole("button", { name: "Export CI project" }).click();
  await expect(page.getByRole("heading", { name: "Export a reproducible CI project" })).toBeVisible();
  await expect(page.getByRole("button", { name: /Release authorization/ })).toHaveClass(/selected/);
  await page.getByRole("button", { name: "Export complete CI project" }).click();
  await expect(page.getByText("Complete CI project exported")).toBeVisible();
  await expect(page.getByText(/files were materialized/)).toBeVisible();
  const generatedWorkflow = page.locator(".generated-file").filter({ hasText: ".github/workflows/structtrace.yml" });
  await expect(generatedWorkflow).toContainText("structtrace release-check latest");
  const sourceRevision = execFileSync("git", ["rev-parse", "HEAD"], { encoding: "utf8", cwd: ".." }).trim();
  await expect(generatedWorkflow).toContainText(`ref: ${sourceRevision}`);
  await page.goto(resultUrl);
  await expect(page.getByRole("heading", { name: "RELEASE AUTHORIZED" })).toBeVisible();
  await page.getByRole("link", { name: "Projects" }).click();
  const project = page.locator(".pinned-list > div").filter({ hasText: projectName }).first();
  await expect(project).toContainText("1 immutable run");
  await expect(project).toContainText("Accepted revision");
  await project.getByRole("button", { name: "Open" }).click();
  await expect(page.getByText(/accepted-.*\.jsonl/)).toBeVisible();
  await expect(page.getByRole("button", { name: "Continue to field mapping" })).toBeDisabled();
  await page.getByRole("link", { name: "Projects" }).click();
  await project.getByRole("button", { name: `Duplicate ${projectName}` }).click();
  await expect(page.getByRole("heading", { name: "What are you comparing?" })).toBeVisible();
  await page.getByRole("link", { name: "Projects" }).click();
  const copyName = `${projectName} copy`;
  const copy = page.locator(".pinned-list > div").filter({ hasText: copyName }).first();
  await expect(copy).toBeVisible();
  page.once("dialog", (dialog) => dialog.accept());
  await copy.getByRole("button", { name: `Archive ${copyName}` }).click();
  await expect(copy).toBeHidden();
});

test("long comparison exposes real reload-safe cancellation and honest retry", async ({ page }) => {
  test.setTimeout(60_000);
  const count = 10_000;
  const dataset = Array.from({ length: count }, (_, index) => JSON.stringify({ id: `job-${index}`, input: { value: index }, expected: { answer: index } })).join("\n") + "\n";
  const outputs = Array.from({ length: count }, (_, index) => JSON.stringify({ id: `job-${index}`, status: "ok", output: { answer: index } })).join("\n") + "\n";
  const schema = JSON.stringify({ type: "object", properties: { answer: { type: "integer" } }, required: ["answer"], additionalProperties: false });
  await page.goto(process.env.STRUCTTRACE_UI_URL ?? "/");
  await page.getByRole("button", { name: "Compare a change" }).click();
  const files = page.locator('input[type="file"]');
  await files.nth(0).setInputFiles({ name: "job-dataset.jsonl", mimeType: "application/x-ndjson", buffer: Buffer.from(dataset) });
  await files.nth(1).setInputFiles({ name: "job-baseline.jsonl", mimeType: "application/x-ndjson", buffer: Buffer.from(outputs) });
  await files.nth(2).setInputFiles({ name: "job-candidate.jsonl", mimeType: "application/x-ndjson", buffer: Buffer.from(outputs) });
  await files.nth(3).setInputFiles({ name: "job.schema.json", mimeType: "application/json", buffer: Buffer.from(schema) });
  await page.getByRole("button", { name: "Continue to field mapping" }).click();
  await page.getByRole("button", { name: "Looks right" }).click();
  await page.getByRole("checkbox", { name: "Use /answer" }).check();
  await page.getByRole("button", { name: "Continue" }).click();
  await page.getByRole("button", { name: "Continue" }).click();
  await page.getByRole("button", { name: "Run comparison" }).click();
  await expect(page.getByRole("button", { name: "Cancel safely" })).toBeVisible({ timeout: 15_000 });
  await expect(page.locator(".stage-list li").first()).toBeVisible();
  await page.reload();
  await expect(page.getByRole("button", { name: "Cancel safely" })).toBeVisible({ timeout: 10_000 });
  await page.getByRole("button", { name: "Cancel safely" }).click();
  await expect(page.getByRole("button", { name: "Retry from retained sources" })).toBeVisible({ timeout: 15_000 });
  await expect(page.getByText(/restarts the complete comparison/)).toBeVisible();
  await page.getByRole("button", { name: "Retry from retained sources" }).click();
  await expect(page.getByRole("button", { name: "Cancel safely" })).toBeVisible({ timeout: 10_000 });
  await page.getByRole("button", { name: "Cancel safely" }).click();
  await expect(page.getByText("No decision was produced", { exact: true })).toBeVisible({ timeout: 15_000 });
});

test("full-source inventory and browser evidence preserve late fields and exact numbers", async ({ page }) => {
  const exactRow = (value: unknown) => JSON.stringify(value)
    .replace('"__BIG_INTEGER__"', "9007199254740993")
    .replace('"__EXACT_DECIMAL__"', "0.12345678901234567890123456789");
  const dataset = Array.from({ length: 30 }, (_, index) => exactRow({
    id: `precision-${index}`,
    input: { value: index },
    expected: index === 29
      ? { amount: "__BIG_INTEGER__", exact_decimal: "__EXACT_DECIMAL__", late_field: "required" }
      : { amount: "__BIG_INTEGER__", exact_decimal: "__EXACT_DECIMAL__" },
  })).join("\n") + "\n";
  const outputRows = (includeLate: boolean) => Array.from({ length: 30 }, (_, index) => exactRow({
    id: `precision-${index}`, status: "ok",
    output: { amount: "__BIG_INTEGER__", exact_decimal: "__EXACT_DECIMAL__", ...(includeLate && index === 29 ? { late_field: "required" } : {}) },
  })).join("\n") + "\n";
  const schema = JSON.stringify({ type: "object", properties: { amount: { type: "integer" }, exact_decimal: { type: "number" }, late_field: { type: "string" }, schema_only: { type: "boolean" } }, required: ["amount", "exact_decimal"], additionalProperties: false });
  await page.goto(process.env.STRUCTTRACE_UI_URL ?? "/");
  await page.getByRole("button", { name: "Compare a change" }).click();
  const files = page.locator('input[type="file"]');
  await files.nth(0).setInputFiles({ name: "precision-dataset.jsonl", mimeType: "application/x-ndjson", buffer: Buffer.from(dataset) });
  await files.nth(1).setInputFiles({ name: "precision-baseline.jsonl", mimeType: "application/x-ndjson", buffer: Buffer.from(outputRows(true)) });
  await files.nth(2).setInputFiles({ name: "precision-candidate.jsonl", mimeType: "application/x-ndjson", buffer: Buffer.from(outputRows(false)) });
  await files.nth(3).setInputFiles({ name: "precision.schema.json", mimeType: "application/json", buffer: Buffer.from(schema) });
  await page.getByRole("button", { name: "Continue to field mapping" }).click();
  await expect(page.locator(".sample-json").first()).toContainText("9007199254740993");
  await expect(page.locator(".sample-json").first()).toContainText("0.12345678901234567890123456789");
  await page.getByRole("button", { name: "Looks right" }).click();
  await expect(page.getByText(/Rust analyzed all 30 expected, 30 baseline, and 30 candidate rows/)).toBeVisible();
  await expect(page.getByRole("checkbox", { name: "Use /late_field" })).toBeVisible();
  await expect(page.getByText("Candidate omission", { exact: true })).toBeVisible();
  await expect(page.getByRole("checkbox", { name: "Use /schema_only" })).toBeVisible();
});
