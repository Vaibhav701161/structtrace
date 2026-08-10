import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

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
  await drawer.getByRole("button", { name: "Pin", exact: true }).click();
  await expect(drawer.getByRole("button", { name: "Pinned", exact: true })).toBeVisible();
  await drawer.getByRole("button", { name: "Close case" }).click();
  await page.getByRole("link", { name: "Regression cases" }).click();
  const pinnedRow = page.locator(".pinned-list > div").filter({ hasText: caseId ?? "missing-case-id" }).first();
  await expect(pinnedRow).toBeVisible();
  await pinnedRow.getByRole("button", { name: "Open evidence" }).click();
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
  const files = page.locator('input[type="file"]');
  await files.nth(0).setInputFiles("../examples/document-extraction/data/golden.jsonl");
  await files.nth(1).setInputFiles("../examples/document-extraction/outputs/baseline.jsonl");
  await files.nth(2).setInputFiles("../examples/document-extraction/outputs/candidate.jsonl");
  await files.nth(3).setInputFiles("../examples/document-extraction/schemas/output.schema.json");
  await page.getByRole("button", { name: "Continue to field mapping" }).click();
  await expect(page.locator(".coverage-card").getByText("matched cases")).toBeVisible();
  await expect(page.locator(".coverage-card strong").first()).toHaveText("12");
  await page.getByRole("button", { name: "Looks right" }).click();
  await expect(page.getByRole("heading", { name: "What does correct mean for your application?" })).toBeVisible();
  await page.getByRole("button", { name: "Continue" }).click();
  await page.getByRole("button", { name: "Continue" }).click();
  await expect(page.getByRole("heading", { name: "Ready to compare" })).toBeVisible();
  await page.getByRole("button", { name: "Run comparison" }).click();
  await expect(page.getByRole("heading", { name: "Should I ship?" })).toBeVisible({ timeout: 20_000 });
  await expect(page.getByRole("heading", { name: "NOT ENOUGH EVIDENCE" })).toBeVisible();
});
