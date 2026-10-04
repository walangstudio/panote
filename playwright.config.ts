import { defineConfig, devices } from "playwright/test";

export default defineConfig({
  testDir: "./e2e",
  globalSetup: "./e2e/global-setup.ts",
  timeout: 15000,
  retries: 0,
  // One worker on purpose. The default (cores/2) points six browsers at a single
  // vite dev server that compiles on demand, and pages then miss the timeout
  // above - which surfaces as "element not found" and reads like a product bug.
  // Measured: 13 of 43 specs failed on concurrency alone and passed serially.
  workers: 1,
  // Lazily imported modals (TransferModal) are compiled on first request, which
  // can outlast the 5s default under full-suite load.
  expect: { timeout: 10000 },
  use: {
    baseURL: "http://localhost:1420",
    headless: true,
    // Enough to debug a CI failure from the uploaded artifact; tracing every
    // test locally would only slow the suite down.
    trace: process.env.CI ? "retain-on-failure" : "off",
    screenshot: "only-on-failure",
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
  webServer: {
    command: "npm run dev",
    url: "http://localhost:1420",
    reuseExistingServer: true,
    // A cold vite start measured 17s here; 30s left no margin and the whole run
    // aborted before a single test began.
    timeout: 120000,
  },
  reporter: [["list"]],
});
