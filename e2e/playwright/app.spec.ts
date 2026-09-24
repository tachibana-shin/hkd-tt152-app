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

/**
 * Sidebar navigation button (scoped to <nav> to avoid matching same-named buttons inside pages).
 * Match on the label text exactly: the accessible name also contains the icon glyph, and
 * substring matching would make "HĐĐT" collide with "Đồng bộ HĐĐT".
 */
const sidebarButton = (page: Page, label: string) =>
  page.locator("nav button", { has: page.getByText(label, { exact: true }) });

/** Ô select "Kết quả kiểm tra" của form tra cứu (label → select ngay dưới). */
const ttxlySelect = (page: Page) => page.locator('label:text-is("Kết quả kiểm tra") + .p-select');

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
    ["HĐĐT", "Hóa đơn điện tử"],
    ["Tra cứu HĐĐT", "Tra cứu HĐĐT"],
    ["Đồng bộ HĐĐT", "Đồng bộ hóa đơn mua"],
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

test("Cấu hình HKD: ngày bắt đầu HĐĐT là bắt buộc và được lưu lại", async ({ page, request }) => {
  await ensureLoggedIn(page);
  await sidebarButton(page, "Hồ sơ HKD").click();
  await expect(page.locator("header h2")).toHaveText("Hồ sơ HKD");
  await page.getByRole("button", { name: "Cài đặt thông tin HKD" }).click();

  const dialog = page.getByRole("dialog", { name: "Cấu hình hộ kinh doanh" });
  const field = dialog.getByLabel("Ngày bắt đầu dùng HĐĐT");
  await expect(field).toBeVisible();

  // Bỏ trống → chặn lưu, báo thiếu ngày bắt đầu HĐĐT.
  // DatePicker của PrimeVue chỉ cập nhật model qua input + blur, nên phải xóa
  // bằng bàn phím (fill/clear chỉ đổi DOM, model vẫn giữ ngày cũ).
  await field.click();
  await field.press("ControlOrMeta+a");
  await field.press("Backspace");
  await field.press("Tab");
  await expect(field).toHaveValue("");
  await dialog.getByRole("button", { name: "Lưu cấu hình" }).click();
  await expect(page.getByText("Thiếu ngày bắt đầu HĐĐT", { exact: true })).toBeVisible({
    timeout: 10_000,
  });
  // Chưa lưu được thì dialog vẫn mở (không đóng được khi chế độ bắt buộc).
  await expect(dialog).toBeVisible();

  // Nhập ngày → lưu được, API trả về giá trị đã lưu.
  await field.click();
  await field.pressSequentially("15/03/2026");
  await field.press("Tab");
  await dialog.getByRole("button", { name: "Lưu cấu hình" }).click();
  await expect(page.getByText("Đã lưu thông tin hộ kinh doanh", { exact: true })).toBeVisible({
    timeout: 10_000,
  });

  const cfg = await (await request.post("/api/get_business_config")).json();
  expect(cfg.hddt_start_date).toBe("2026-03-15");
});

test("Đồng bộ HĐ mua: xem trước từ cache và nhập kho tạo phiếu + mặt hàng", async ({
  page,
  request,
}) => {
  await ensureLoggedIn(page);
  // Tra cứu và đồng bộ là 2 mục menu cùng cấp HĐĐT, không nằm cuộn trong màn HĐĐT.
  await sidebarButton(page, "HĐĐT").click();
  await expect(page.locator("header h2")).toHaveText("Hóa đơn điện tử");
  await expect(page.getByRole("button", { name: "Tìm kiếm", exact: true })).toBeHidden();
  await expect(page.getByRole("button", { name: "Quét cổng" })).toBeHidden();

  await sidebarButton(page, "Đồng bộ HĐĐT").click();
  await expect(page.locator("header h2")).toHaveText("Đồng bộ hóa đơn mua");
  await expect(page.getByRole("button", { name: "Quét cổng" })).toBeVisible();

  // Cache rỗng ngay từ đầu → bảng không có dòng, nút nhập tất cả ẩn.
  const table = page.locator(".p-datatable").last();
  await expect(table).toBeVisible();
  await expect(page.getByText("Nhập tất cả", { exact: false })).toBeHidden();

  // Chèn 1 hóa đơn mua đã cache (giả lập kết quả quét cổng) rồi tải lại cache.
  const detail = {
    hdhhdvu: [
      {
        ten: "Bình NN Rossi E2E",
        dvtinh: "Cái",
        mhhdvu: "puro30",
        sluong: 2,
        dgia: 1000000,
        stckhau: 0,
        tsuat: 0.08,
      },
      {
        ten: "Bộ lọc E2E",
        dvtinh: "Bộ",
        mhhdvu: "filter",
        sluong: 1,
        dgia: 500000,
        stckhau: 0,
        tsuat: 0.08,
      },
    ],
  };
  const seed = await request.post("/api/hddt_sync_test_seed", {
    data: { portal_id: "e2e-uuid-1", detail },
  });
  expect(seed.ok(), `seed hddt_sync_test_seed failed ${seed.status()}`).toBe(true);

  await page.getByRole("button", { name: "Xem lại cache" }).click();
  await expect(page.getByText("Có 1 hóa đơn trong cache", { exact: false })).toBeVisible({
    timeout: 15_000,
  });
  await expect(table.getByText("C26E2E", { exact: false })).toBeVisible();
  await expect(page.getByText("Chờ nhập kho", { exact: true })).toBeVisible();

  // Nhập kho → tạo phiếu, tạo 2 mặt hàng mới, gắn số phiếu vào hóa đơn.
  await table.getByRole("button", { name: "Nhập kho" }).first().click();
  await expect(page.getByText("Đã tạo 1 phiếu nhập", { exact: false })).toBeVisible({
    timeout: 20_000,
  });
  await expect(page.getByText("Đã nhập kho", { exact: true })).toBeVisible();

  // Mặt hàng mới tồn tại trong DB, liên kết chặt hóa đơn ↔ phiếu nhập.
  const inv = await (
    await request.post("/api/hddt_sync_preview", {
      data: { from: null, to: null, retry_failed: false },
    })
  ).json();
  const row = (inv.rows as Array<Record<string, unknown>>)[0];
  expect(row.status).toBe("imported");
  expect(String(row.voucher_no)).toMatch(/^PN\d+$/);
  const products = await (await request.post("/api/get_products", { data: {} })).json();
  const names = (products as Array<{ name: string }>).map((p) => p.name);
  expect(names).toContain("Bình NN Rossi E2E");
  expect(names).toContain("Bộ lọc E2E");
});

test("Đồng bộ HĐ mua: chạy lần 2 không tạo trùng", async ({ page, request }) => {
  await ensureLoggedIn(page);
  // Dùng lại hóa đơn đã nhập ở test trước (đã 'imported').
  const before = await (
    await request.post("/api/hddt_sync_preview", {
      data: { from: null, to: null, retry_failed: false },
    })
  ).json();
  const importedBefore = (before.rows as Array<{ status: string }>).filter(
    (r) => r.status === "imported",
  ).length;

  const res = await (
    await request.post("/api/hddt_sync_import", {
      data: {
        ids: [],
        warehouse_code: null,
        unit_code: null,
        debit_account: null,
        credit_account: null,
      },
    })
  ).json();
  expect(res.imported).toBe(0);
  expect(res.failed).toBe(0);

  const after = await (
    await request.post("/api/hddt_sync_preview", {
      data: { from: null, to: null, retry_failed: false },
    })
  ).json();
  const importedAfter = (after.rows as Array<{ status: string }>).filter(
    (r) => r.status === "imported",
  ).length;
  expect(importedAfter).toBe(importedBefore);
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

  await sidebarButton(page, "Tra cứu HĐĐT").click();
  await expect(page.locator("header h2")).toHaveText("Tra cứu HĐĐT");

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
  await expect(firstRow.locator("td").nth(1)).not.toHaveText("—"); // ký hiệu HĐ
  await expect(firstRow.locator("td").nth(2)).not.toHaveText("—"); // số HĐ
  // Cột "Mã số thuế" và "Mẫu số" đã bỏ khỏi UI (dữ liệu vẫn còn trong API).
  await expect(table.getByRole("columnheader", { name: "Mã số thuế" })).toBeHidden();
  await expect(table.getByRole("columnheader", { name: "Mẫu số" })).toBeHidden();
});

test("Tra cứu HĐĐT: mỗi tab nhớ bộ lọc riêng và giữ state khi đổi menu (KeepAlive)", async ({
  page,
}) => {
  await ensureLoggedIn(page);
  await sidebarButton(page, "Tra cứu HĐĐT").click();
  await expect(page.locator("header h2")).toHaveText("Tra cứu HĐĐT");

  // Ô "bán ra × máy tính tiền" (mặc định): đặt Số hóa đơn + đổi Kết quả kiểm tra.
  const soHoaDon = page.locator('label:text-is("Số hóa đơn") + input');
  await soHoaDon.fill("821");
  await ttxlySelect(page).click();
  await page.getByRole("option", { name: "Đã phê duyệt", exact: true }).click();

  // Sang "mua vào": bộ lọc phải là bộ mặc định của ô đó, không kế thừa ô trước.
  await page.getByRole("tab", { name: /Hóa đơn vào/ }).click();
  await expect(soHoaDon).toHaveValue("");
  await expect(ttxlySelect(page)).toContainText("Đã cấp mã hóa đơn");

  // Quay lại "bán ra" → nhớ đúng giá trị đã đặt.
  await page.getByRole("tab", { name: /Hóa đơn ra/ }).click();
  await expect(soHoaDon).toHaveValue("821");
  await expect(ttxlySelect(page)).toContainText("Đã phê duyệt");

  // Rời màn rồi vào lại (KeepAlive) → tab + bộ lọc vẫn nguyên, không nạp lại.
  await sidebarButton(page, "Tổng quan").click();
  await expect(page.locator("header h2")).toHaveText("Tổng quan");
  await sidebarButton(page, "Tra cứu HĐĐT").click();
  await expect(page.locator("header h2")).toHaveText("Tra cứu HĐĐT");
  await expect(page.getByRole("tab", { name: /Hóa đơn ra/ })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await expect(soHoaDon).toHaveValue("821");
});

test("Màn in phiếu không bị KeepAlive giữ (nạp lại theo route.params)", async ({ page }) => {
  await ensureLoggedIn(page);
  // Chặn API để mỗi lần mở trả một phiếu khác nhau: nếu component bị cache,
  // lần thứ hai vẫn hiện dữ liệu của lần đầu.
  let calls = 0;
  await page.route("**/api/get_voucher", async (route) => {
    calls += 1;
    const no = calls === 1 ? "PN-CACHE-1" : "PN-CACHE-2";
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify([
        {
          entry_type: "PN",
          voucher_no: no,
          posting_date: "2026-09-25",
          product_code: "SP001",
          product_name: `Phiếu ${calls}`,
          unit: "Cái",
          quantity: 1,
          unit_price: 1000,
          amount: 1000,
        },
      ]),
    });
  });

  await page.goto("/print/PN-1");
  await expect(page.getByText("Phiếu 1", { exact: false })).toBeVisible({ timeout: 15_000 });
  await page.goto("/print/PN-2");
  await expect(page.getByText("Phiếu 2", { exact: false })).toBeVisible({ timeout: 15_000 });
  expect(calls).toBe(2);
});

test("HĐĐT đổi sang hóa đơn vào xóa kết quả tab trước và mặc định kết quả kiểm tra", async ({
  page,
  request,
}) => {
  await ensureLoggedIn(page);
  const login = await loginPortal(request);
  test.skip(login === null, "Thiếu credentials cổng HĐĐT (info.txt / E2E_HDDT_*) — bỏ qua");

  await sidebarButton(page, "Tra cứu HĐĐT").click();
  await expect(page.locator("header h2")).toHaveText("Tra cứu HĐĐT");
  await page.getByRole("button", { name: "Tìm kiếm", exact: true }).click();
  await expect(page.getByText(/Có \d[\d.]* kết quả/)).toBeVisible({ timeout: 30_000 });

  // Sang tab "hóa đơn vào": kết quả cũ phải bị xóa (không lẫn endpoint khác).
  await page.getByRole("tab", { name: /Hóa đơn vào/ }).click();
  await expect(page.getByText(/Có \d[\d.]* kết quả/)).toBeHidden();
  // Nhãn đối tác đổi theo hướng tra cứu.
  await expect(page.getByText("MST người bán", { exact: true })).toBeVisible();
  // Mặc định của cổng: "Kết quả kiểm tra" = Đã cấp mã hóa đơn (ttxly==5).
  await expect(ttxlySelect(page)).toContainText("Đã cấp mã hóa đơn");

  // Mỗi ô tab giữ bộ lọc riêng: quay lại "bán ra" phải là "Tất cả", không phải
  // "Đã cấp mã hóa đơn" của ô vừa rời; sang lại "mua vào" thì vẫn "Đã cấp mã".
  await page.getByRole("tab", { name: /Hóa đơn ra/ }).click();
  await expect(ttxlySelect(page)).toContainText("Tất cả");
  await page.getByRole("tab", { name: /Hóa đơn vào/ }).click();
  await expect(ttxlySelect(page)).toContainText("Đã cấp mã hóa đơn");

  // Tra "hóa đơn vào" → cột đối tác phải là NGƯỜI BÁN (người mua là chính mình).
  // Ô "hóa đơn vào × máy tính tiền" rỗng với tài khoản này (0 kết quả với ttxly==5
  // mặc định của cổng) → chuyển sang "Hóa đơn điện tử" để có dữ liệu thật.
  await page.getByRole("tab", { name: "Hóa đơn điện tử" }).click();
  await page.getByRole("button", { name: "Tìm kiếm", exact: true }).click();
  await expect(page.getByText(/Có \d[\d.]* kết quả/)).toBeVisible({ timeout: 30_000 });
  const table = page.locator(".p-datatable");
  await expect(table.getByRole("columnheader", { name: "Thông tin người bán" })).toBeVisible();
  const firstRow = table.locator("tbody tr").first();
  await expect(firstRow.getByText("MST người bán:").first()).toBeVisible();
  await expect(firstRow.getByText("Tên người bán:").first()).toBeVisible();
  // MST người bán nằm trong ô đối tác (cột "Mã số thuế" đã bỏ) → phải là MST thật.
  await expect(firstRow.getByText(/MST người bán:\s*\d{10,13}/).first()).toBeVisible();
});
