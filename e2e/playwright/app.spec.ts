import { expect, test } from "@playwright/test";
import type { APIRequestContext, Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { BASE_URL } from "./constants";
import { ADMIN, seedApp } from "./helpers";

// A single worker runs these serially (workers: 1); all 7 tests share one app instance
// and one server-side login session. seedApp runs once before the whole file.
test.beforeAll(async () => {
  await seedApp(BASE_URL);
});

/** Sidebar navigation button (scoped to <nav> to avoid matching same-named buttons inside pages). */
const sidebarButton = (page: Page, label: string) =>
  page.locator("nav").getByRole("button", { name: label });

/** Ensure we are logged in via the UI: fill the login form if shown, then wait for the Dashboard. */
async function ensureLoggedIn(page: Page) {
  await page.goto("/");
  const username = page.getByTestId("login-username");
  if (await username.isVisible()) {
    await username.fill(ADMIN.username);
    await page.getByTestId("login-password").fill(ADMIN.password);
    await page.getByRole("button", { name: "Đăng nhập" }).click();
  }
  await expect(page.locator("header h2")).toHaveText("Tổng quan", { timeout: 20_000 });
}

test("login with a wrong password shows an error toast and stays on the login screen", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByTestId("login-username").fill(ADMIN.username);
  await page.getByTestId("login-password").fill("wrong-password");
  await page.getByRole("button", { name: "Đăng nhập" }).click();
  await expect(page.locator(".p-toast-detail")).toContainText("Sai tên đăng nhập hoặc mật khẩu", {
    timeout: 10_000,
  });
  await expect(page.getByTestId("login-username")).toBeVisible();
});

test("login with valid credentials opens the Dashboard with the seeded business info", async ({
  page,
}) => {
  await ensureLoggedIn(page);
  await expect(page.locator("header h2")).toHaveText("Tổng quan");
  // Header shows the active business (name + tax code) and the revenue card renders.
  // Note: the header button has an icon prefix in its accessible name → use a regex.
  await expect(page.getByRole("button", { name: /E2E TEST BUSINESS/ })).toBeVisible();
  await expect(page.getByText("MST: 010000000000", { exact: true })).toBeVisible();
  await expect(page.getByText("Doanh thu ròng")).toBeVisible();
  // "Quản trị" is the seeded admin display name in the app's own seed data.
  await expect(page.getByText("Quản trị").first()).toBeVisible();
});

test("navigating all main tabs updates the header title correctly", async ({ page }) => {
  await ensureLoggedIn(page);
  // [sidebar button label, header h2 title]
  const cases: Array<[string, string]> = [
    ["Tổng quan", "Tổng quan"],
    ["Sản phẩm", "Danh mục sản phẩm"],
    ["Tài khoản", "Danh mục tài khoản"],
    ["Đối tác & Kho", "Danh mục đối tác & kho"],
    ["Bảng lương", "Bảng lương"],
    ["Chấm công", "Chấm công"],
    ["Thu / Chi", "Phiếu thu / chi"],
    ["Sổ nhật ký", "Sổ nhật ký chung"],
    ["Nhập liệu", "Nhập liệu Excel"],
    ["Nhập kho", "Nhập kho"],
    ["Xuất kho", "Xuất kho / Bán hàng"],
    ["Tồn kho", "Tồn kho"],
    ["Hóa đơn", "Hóa đơn"],
    ["Nhật ký HĐ", "Nhật ký hoạt động"],
    ["Kế toán HKD", "Kế toán HKD"],
    ["Người dùng", "Người dùng & phân quyền"],
    ["Hồ sơ HKD", "Hồ sơ HKD"],
    ["Cài đặt", "Cài đặt"],
  ];
  for (const [tab, title] of cases) {
    // Sidebar buttons carry an icon in their accessible name (e.g. " Tổng quan") → match substring.
    await sidebarButton(page, tab).click();
    await expect(page.locator("header h2")).toHaveText(title, { timeout: 15_000 });
  }
});

test("Products tab renders real data from the DB (seeded product)", async ({ page }) => {
  await ensureLoggedIn(page);
  await sidebarButton(page, "Sản phẩm").click();
  await expect(page.locator("header h2")).toHaveText("Danh mục sản phẩm");
  await expect(page.locator(".p-datatable")).toBeVisible();
  await expect(page.getByText("SP001", { exact: true })).toBeVisible();
  await expect(page.getByText("Bottled water", { exact: true })).toBeVisible();
});

test("Accounts tab renders the seeded chart of accounts (default accounts)", async ({ page }) => {
  await ensureLoggedIn(page);
  await sidebarButton(page, "Tài khoản").click();
  await expect(page.locator("header h2")).toHaveText("Danh mục tài khoản");
  await expect(page.locator(".p-datatable")).toBeVisible();
  await expect(page.getByText("Tiền mặt", { exact: true })).toBeVisible();
  await expect(page.getByText("Phải trả cho người bán", { exact: true })).toBeVisible();
});

test("no JS/console errors while navigating several tabs", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(`pageerror: ${String(e)}`));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push(`console: ${m.text()}`);
  });

  await ensureLoggedIn(page);
  const cases: Array<[string, string]> = [
    ["Sản phẩm", "Danh mục sản phẩm"],
    ["Tài khoản", "Danh mục tài khoản"],
    ["Bảng lương", "Bảng lương"],
    ["Cài đặt", "Cài đặt"],
    ["Tổng quan", "Tổng quan"],
  ];
  for (const [tab, title] of cases) {
    await sidebarButton(page, tab).click();
    await expect(page.locator("header h2")).toHaveText(title, { timeout: 15_000 });
  }
  expect(errors).toEqual([]);
});

test("logging out returns to the login screen", async ({ page }) => {
  await ensureLoggedIn(page);
  await page.getByRole("button", { name: "Đăng xuất" }).click();
  await expect(page.getByTestId("login-username")).toBeVisible();
});

// ─── HĐĐT: tra cứu hóa đơn thật (cần credentials cổng + IP VN) ───

/**
 * Credentials cổng HĐĐT từ `info.txt` ở gốc repo (đã gitignore). Ưu tiên biến môi
 * trường E2E_HDDT_USERNAME / E2E_HDDT_PASSWORD; thiếu cả hai → test bị skip
 * (suite vẫn xanh trên máy không có tài khoản cổng).
 */
function portalCredentials(): { username: string; password: string } | null {
  const envU = process.env.E2E_HDDT_USERNAME;
  const envP = process.env.E2E_HDDT_PASSWORD;
  if (envU && envP) return { username: envU, password: envP };
  try {
    const txt = readFileSync(fileURLToPath(new URL("../../info.txt", import.meta.url)), "utf8");
    const pick = (key: string) =>
      txt
        .split("\n")
        .map((l) => l.split("="))
        .find(([k]) => k.trim() === key)?.[1]
        ?.trim();
    const username = pick("USERNAME");
    const password = pick("PASSWORD");
    return username && password ? { username, password } : null;
  } catch {
    return null;
  }
}

/** Cấu hình + đăng nhập cổng HĐĐT qua API (captcha solver chạy offline trong Rust). */
async function loginPortal(api: APIRequestContext) {
  const creds = portalCredentials();
  if (!creds) return null;
  const base = process.env.E2E_HDDT_BASE_URL ?? "https://hoadondientu.gdt.gov.vn";
  const cfg = await api.post("/api/hddt_save_config", {
    data: { username: creds.username, password: creds.password, baseUrl: base },
  });
  expect(cfg.ok(), `hddt_save_config failed ${cfg.status()}`).toBe(true);
  const login = await api.post("/api/hddt_login", { data: {} });
  expect(login.ok(), `hddt_login failed ${login.status()}`).toBe(true);
  const body = await login.json();
  expect(body.need_manual, "portal yêu cầu captcha thủ công — solver chưa pass").toBeFalsy();
  return body;
}

test("HĐĐT tra cứu hóa đơn: mặc định tab máy tính tiền và trả về hóa đơn thật", async ({
  page,
  request,
}) => {
  await ensureLoggedIn(page);
  const login = await loginPortal(request);
  test.skip(login === null, "Thiếu credentials cổng HĐĐT (info.txt / E2E_HDDT_*) — bỏ qua");

  await sidebarButton(page, "HĐĐT").click();
  await expect(page.locator("header h2")).toHaveText("Hóa đơn điện tử");
  await expect(page.getByText("Tra cứu hóa đơn", { exact: true })).toBeVisible();

  // Tab lớn mặc định = hóa đơn ra; tab nhỏ mặc định = máy tính tiền (sco-query).
  const kindTabs = page.getByRole("tab");
  await expect(kindTabs.filter({ hasText: "Hóa đơn ra" })).toHaveAttribute("aria-selected", "true");
  await expect(kindTabs.filter({ hasText: "Hóa đơn máy tính tiền" })).toHaveAttribute(
    "aria-selected",
    "true",
  );

  // Tìm kiếm thật: dải ngày mặc định = 30 ngày gần nhất.
  await page.getByRole("button", { name: "Tìm kiếm", exact: true }).click();
  const table = page.locator(".p-datatable");
  await expect(table).toBeVisible({ timeout: 30_000 });
  await expect(page.getByText(/Có \d[\d.]* kết quả/)).toBeVisible({ timeout: 30_000 });
  // Dữ liệu thật: các cột định danh của hóa đơn đều có giá trị.
  await expect(table.getByText("Ký hiệu HĐ")).toBeVisible();
  await expect(table.locator("tbody tr").first()).toBeVisible();
  const firstRow = table.locator("tbody tr").first();
  await expect(firstRow.locator("td").nth(1)).not.toHaveText("—");
  await expect(firstRow.locator("td").nth(4)).not.toHaveText("—");
});

test("HĐĐT đổi sang hóa đơn vào xóa kết quả tab trước và mặc định kết quả kiểm tra", async ({
  page,
  request,
}) => {
  await ensureLoggedIn(page);
  const login = await loginPortal(request);
  test.skip(login === null, "Thiếu credentials cổng HĐĐT (info.txt / E2E_HDDT_*) — bỏ qua");

  await sidebarButton(page, "HĐĐT").click();
  await expect(page.locator("header h2")).toHaveText("Hóa đơn điện tử");
  await page.getByRole("button", { name: "Tìm kiếm", exact: true }).click();
  await expect(page.getByText(/Có \d[\d.]* kết quả/)).toBeVisible({ timeout: 30_000 });

  // Sang tab "hóa đơn vào": kết quả cũ phải bị xóa (không lẫn endpoint khác).
  await page.getByRole("tab", { name: /Hóa đơn vào/ }).click();
  await expect(page.getByText(/Có \d[\d.]* kết quả/)).toBeHidden();
  // Nhãn đối tác đổi theo hướng tra cứu.
  await expect(page.getByText("MST người bán", { exact: true })).toBeVisible();
  // Mặc định của cổng: "Kết quả kiểm tra" = Đã cấp mã hóa đơn (ttxly==5).
  await expect(page.getByText("Đã cấp mã hóa đơn", { exact: true })).toBeVisible();
});
