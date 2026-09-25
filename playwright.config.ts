import { defineConfig } from "@playwright/test";
import { BASE_URL, MOCK_PORTAL_PORT, WEB_PORT } from "./e2e/playwright/constants";

/**
 * Playwright E2E — no manual build needed: webServer runs `bun run tauri dev` itself
 * (see e2e/playwright/start-app.sh) with:
 *   - Vite dev server on its own port 1421 (never touches the user's 1420),
 *   - the app's embedded web server on 45821 (HKD_WEB_PORT), Vite proxies /api to it
 *     via VITE_API_PROXY,
 *   - an isolated data dir (HKD_DATA_DIR) — the tests never touch real data.
 * Playwright only starts the tests once GET {web server}/api/health returns 200, i.e.
 * BOTH Vite and the app's web server are up — no startup race.
 *
 * A second webServer runs a mock HDDT portal so the HĐĐT tests run offline with no
 * credentials. Set E2E_HDDT_LIVE=1 to point those tests at the real portal instead
 * (needs ../info.txt or E2E_HDDT_USERNAME/E2E_HDDT_PASSWORD).
 *
 * Browser: system Google Chrome (channel "chrome") — no separate Chromium download.
 */
export default defineConfig({
  testDir: "./e2e/playwright",
  timeout: 90_000, // app desktop lần chạy đầu (vừa build) chậm hơn 60s
  expect: { timeout: 15_000 },
  fullyParallel: false,
  workers: 1,
  retries: 0,
  reporter: [["list"], ["html", { open: "never" }]],
  use: {
    baseURL: BASE_URL,
    channel: "chrome",
    headless: true,
    trace: "on-first-retry",
    screenshot: "only-on-failure",
    viewport: { width: 1440, height: 900 },
  },
  webServer: [
    {
      command: "bash ./e2e/playwright/start-app.sh",
      // Probe the app's web server directly (not through Vite — avoids a false-positive
      // 200 from Vite's SPA fallback). tauri CLI only launches the app once Vite 1421 is
      // up, so health 200 also means Vite is ready.
      url: `http://127.0.0.1:${WEB_PORT}/api/health`,
      reuseExistingServer: false,
      timeout: 300_000, // first `tauri dev` run compiles Rust — allow generous time
      stdout: "pipe",
      stderr: "pipe",
    },
    {
      command: `bun run ./e2e/playwright/mock-portal.ts`,
      env: { E2E_MOCK_PORTAL_PORT: String(MOCK_PORTAL_PORT) },
      url: `http://127.0.0.1:${MOCK_PORTAL_PORT}/api/captcha`,
      reuseExistingServer: true,
      timeout: 20_000,
      stdout: "pipe",
      stderr: "pipe",
    },
  ],
});
