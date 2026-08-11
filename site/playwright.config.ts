import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "./tests",
  fullyParallel: true,
  retries: 0,
  use: { baseURL: "http://127.0.0.1:4321", trace: "retain-on-failure" },
  webServer: { command: "python3 -m http.server 4321 --bind 127.0.0.1 --directory dist", url: "http://127.0.0.1:4321", reuseExistingServer: true },
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"] } },
    { name: "mobile", use: { ...devices["Pixel 7"] } },
  ],
});
