import { defineConfig } from "@playwright/test";
export default defineConfig({
  testDir: "./tests",
  testMatch: "**/*.spec.ts",
  timeout: 60000,
  use: {
    baseURL: "http://127.0.0.1:1421",
    channel: "msedge",
    headless: true,
    reducedMotion: "reduce",
  },
  workers: 1,
  webServer: {
    command: "npm run preview -- --port 1421",
    url: "http://127.0.0.1:1421",
    reuseExistingServer: true,
  },
  reporter: "list",
});
