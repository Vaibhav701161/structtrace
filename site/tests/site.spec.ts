import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

const routes = ["/", "/product", "/use-cases", "/try", "/research", "/docs", "/security", "/download", "/privacy"];

for (const route of routes) {
  test(`${route} renders without accessibility violations`, async ({ page }) => {
    const response = await page.goto(route);
    expect(response?.ok()).toBeTruthy();
    await expect(page.locator("h1")).toBeVisible();
    const result = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa"]).analyze();
    expect(result.violations).toEqual([]);
  });
}

test("guided demo preserves its non-authoritative boundary", async ({ page }) => {
  await page.goto("/try");
  await expect(page.getByText(/not enough independent evidence/i)).toBeVisible();
  await page.getByRole("tab", { name: "2. Paired cases" }).click();
  await expect(page.getByRole("cell", { name: "invoice-011", exact: true })).toBeVisible();
  await page.getByRole("tab", { name: "3. Regression evidence" }).click();
  await expect(page.getByText(/Schema-valid. Semantically wrong./)).toBeVisible();
});

test("mobile menu exposes the full primary navigation", async ({ page }, testInfo) => {
  test.skip(testInfo.project.name !== "mobile");
  await page.goto("/");
  const toggle = page.getByRole("button", { name: "Open navigation" });
  await toggle.click();
  const navigation = page.getByRole("navigation", { name: "Primary navigation" });
  await expect(navigation).toBeVisible();
  await expect(navigation.getByRole("link", { name: "Download", exact: true })).toBeVisible();
});
