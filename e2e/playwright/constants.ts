// E2E-only ports — never collide with the user's dev environment (Vite 1420, app 45731).
// Must match e2e/playwright/start-app.sh (HKD_WEB_PORT, VITE_PORT, VITE_API_PROXY).
export const WEB_PORT = 45821; // the app's embedded web server
export const VITE_PORT = 1421; // Vite dev server (tauri dev)
export const BASE_URL = `http://localhost:${VITE_PORT}`;
export const MOCK_PORTAL_PORT = 45733; // mock HDDT portal cho E2E offline
export const MOCK_PORTAL_URL = `http://127.0.0.1:${MOCK_PORTAL_PORT}`;
