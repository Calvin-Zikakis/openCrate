import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests/browser",
  fullyParallel: false,
  workers: 1,
  retries: 0,
  use: { baseURL: "http://127.0.0.1:1421", viewport: { width: 1440, height: 960 }, trace: "retain-on-failure" },
  projects: [
    { name: "chromium", use: { browserName: "chromium" } },
    // The desktop app runs on WebKit on macOS; drag and drop is where the engines differ most.
    { name: "webkit", use: { browserName: "webkit" }, grep: /crates:|arrange:|player:/ },
  ],
  webServer: {
    command: "npm run preview",
    url: "http://127.0.0.1:1421",
    reuseExistingServer: false,
    timeout: 30_000,
  },
});
