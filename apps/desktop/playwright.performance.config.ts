import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "./tests/performance",
  timeout: 120_000,
  workers: 1,
  reporter: [["list"], ["json", { outputFile: "test-results/terminal-performance.json" }]],
  use: {
    baseURL: "http://127.0.0.1:5175",
    trace: "retain-on-failure",
  },
  projects: [
    {
      name: "chromium-terminal-renderer-100-percent",
      metadata: { deviceScaleFactor: 1 },
      use: { ...devices["Desktop Chrome"], deviceScaleFactor: 1 },
    },
    {
      name: "chromium-terminal-renderer-150-percent",
      metadata: { deviceScaleFactor: 1.5 },
      use: { ...devices["Desktop Chrome"], deviceScaleFactor: 1.5 },
    },
  ],
  webServer: {
    command: "npm run dev -- --port 5175 --strictPort",
    url: "http://127.0.0.1:5175",
    reuseExistingServer: false,
    timeout: 30_000,
  },
});
