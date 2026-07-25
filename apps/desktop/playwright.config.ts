import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "./tests/ui",
  // The full Windows CI suite repeatedly cold-boots the large preview bundle.
  // Keep local failures fast while allowing slower hosted runners to finish
  // the same readiness handshake without turning a healthy final test flaky.
  timeout: process.env.CI ? 60_000 : 30_000,
  expect: {
    timeout: 5_000
  },
  use: {
    baseURL: "http://127.0.0.1:5174",
    trace: "retain-on-failure"
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] }
    }
  ],
  webServer: {
    command: "npm run dev -- --port 5174 --strictPort",
    url: "http://127.0.0.1:5174",
    reuseExistingServer: !process.env.CI,
    timeout: process.env.CI ? 60_000 : 30_000
  }
});
