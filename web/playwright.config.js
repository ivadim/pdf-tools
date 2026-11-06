import { defineConfig } from "@playwright/test";

const baseURL = process.env.PDFTOOLS_WEB_URL ?? "http://127.0.0.1:4173";

export default defineConfig({
  testDir: "./e2e",
  fullyParallel: false,
  timeout: 45_000,
  use: {
    baseURL,
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
    video: "retain-on-failure",
  },
  webServer: {
    command: `python3 -m http.server ${new URL(baseURL).port} --bind 127.0.0.1 --directory ../bazel-bin/web/site`,
    url: `${baseURL}/index.html`,
    reuseExistingServer: !process.env.CI,
  },
});
