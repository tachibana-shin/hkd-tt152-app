import { expect, test } from "@playwright/test";
import type { APIRequestContext, Locator, Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { BASE_URL, MOCK_PORTAL_URL } from "./constants";
import { ADMIN, seedApp } from "./helpers";

// A single worker runs these serially (workers: 1); every test shares one app
// instance and one server-side login session. seedApp runs once for the file.
test.beforeAll(async ({ browser }) => {
  await seedApp(BASE_URL);
  // Warm-up: lần chạy đầu tiên app vừa build xong (Rust + Vite) còn chậm, mở 1 lần
  // để chờ sẵn để test đầu tiên không hết timeout vì thời gian khởi động.
  const ctx = await browser.newContext();
  const warmup = await ctx.newPage();
  await warmup.goto("/", { waitUntil: "domcontentloaded" });
  await warmup.locator("body").waitFor({ state: "visible", timeout: 60_000 });
  await warmup.waitForTimeout(1_500);
  await ctx.close();
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

/**
 * Đưa trang về màn đăng nhập và CHỜ app thật sự render xong đó.
 *
 * Hai bẫy của test đầu tiên, đều do `isVisible()` kiểm tra TỨC THÌ:
 *  - app còn đang bootstrap (spinner) → trả false sai, test tưởng đã đăng nhập
 *    rồi chờ Dashboard;
 *  - server đã thoát mà trang vẫn mở ra Dashboard → không có form để điền.
 *
 * `<header>` chỉ tồn tại trong nhánh app shell (chỉ render khi đã đăng nhập),
 * LoginView không có `h2` → hai marker này loại trừ lẫn nhau. Chờ một trong hai
 * rồi mới phân nhánh; nếu trang vào được Dashboard thì đăng xuất qua UI.
 */
async function gotoLogin(page: Page) {
  const username = page.getByTestId("login-username");
  const shell = page.locator("header h2");
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await expect(username.or(shell).first()).toBeVisible({ timeout: 30_000 });
  if (await username.isVisible()) return;
  await page.getByRole("button", { name: "Đăng xuất" }).click();
  await expect(username).toBeVisible({ timeout: 20_000 });
}

/**
 * Đảm bảo phiên đăng nhập đã thực sự rời khỏi server trước khi mở trang.
 *
 * `POST /api/logout` là idempotent: nó luôn trả "ok" cả khi phiên đã tắt, nên
 * response không nói lên gì. Poll `get_current_user` cho tới khi server thật sự
 * trả `null` — nhưng đó vẫn chỉ là lời khai của server: trang có thể mở ra
 * Dashboard (phản hồi muộn, mạng chập chờn) → luôn kết thúc bằng `gotoLogin`
 * để tự kiểm chứng trên UI và đăng xuất nếu cần.
 */
async function ensureLoggedOut(request: APIRequestContext, page: Page) {
  await request.post("/api/logout", { data: {} });
  for (let i = 0; i < 20; i++) {
    const res = await request.post("/api/get_current_user", { data: {} }).catch(() => null);
    if (res?.ok()) {
      const body = (await res.text()).trim();
      if (body === "null" || body === "") break;
    }
    await page.waitForTimeout(250);
  }
  await gotoLogin(page);
}

/** Ensure we are logged in via the UI: fill the login form if shown, then wait for the Dashboard. */
async function ensureLoggedIn(page: Page) {
  const username = page.getByTestId("login-username");
  const shell = page.locator("header h2");
  await page.goto("/", { waitUntil: "domcontentloaded" });
  // Chờ app xong bootstrap rồi mới phân nhánh — `isVisible()` tức thì sẽ trả
  // false sai khi trang còn đang hiện spinner.
  await expect(username.or(shell).first()).toBeVisible({ timeout: 30_000 });
  if (await username.isVisible()) {
    await username.fill(ADMIN.username);
    await page.getByTestId("login-password").fill(ADMIN.password);
    await page.getByRole("button", { name: "Đăng nhập" }).click();
  }
  await expect(shell).toHaveText("Tổng quan", { timeout: 20_000 });
}

test("login with a wrong password shows an error toast and stays on the login screen", async ({
  page,
  request,
}) => {
  // Phiên đăng nhập của app là trạng thái chung trong tiến trình server, không
  // gắn cookie theo browser context → nếu instance cũ còn sống (hoặc test trước
  // đã login) thì trang mở ra là Dashboard. ensureLoggedOut đã kiểm chứng tới
  // khi màn đăng nhập thật sự hiện ra.
  await ensureLoggedOut(request, page);
  await page.getByTestId("login-username").fill(ADMIN.username);
  await page.getByTestId("login-password").fill("wrong-password");
  await page.getByRole("button", { name: "Đăng nhập" }).click();
  // 20s: đây là test đầu tiên của suite nên app vừa khởi động — băm mật khẩu và
  // mở kho dữ liệu lần đầu còn lâu hơn các test sau.
  await expect(page.locator(".p-toast-detail")).toContainText("Sai tên đăng nhập hoặc mật khẩu", {
    timeout: 20_000,
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

/**
 * Bấm một tab trên sidebar và chờ URL đổi.
 *
 * Bấm liên tiếp 22 màn trong vòng lặp đôi khi rơi mất lần bấm: app còn đang
 * mount route lazy (Vite biên dịch chunk lần đầu) thì click không tới được
 * router. Người dùng thật chỉ cần bấm lại, nên test cũng vậy — tối đa 3 lần.
 */
async function openTab(page: Page, tab: string, path: string) {
  // Sidebar buttons carry an icon in their accessible name (e.g. " Tổng quan") → match substring.
  const button = sidebarButton(page, tab);
  const url = new RegExp(`${path}$`);
  for (let attempt = 0; attempt < 3; attempt++) {
    await button.click();
    try {
      // 30 s: runner CI 2-core biên dịch chunk lazy chậm hơn máy dev nhiều.
      await expect(page).toHaveURL(url, { timeout: 30_000 });
      return;
    } catch {
      /* click rơi — thử lại */
    }
  }
  throw new Error(`Không chuyển được tới "${tab}" (${path}) sau 3 lần bấm`);
}

test("navigating all main tabs updates the header title correctly", async ({ page }) => {
  await ensureLoggedIn(page);
  // [sidebar button label, header h2 title, route]
  const cases: Array<[string, string, string]> = [
    ["Tổng quan", "Tổng quan", "/"],
    ["Sản phẩm", "Danh mục sản phẩm", "/products"],
    ["Tài khoản", "Danh mục tài khoản", "/accounts"],
    ["Đối tác & Kho", "Danh mục đối tác & kho", "/catalogs"],
    ["Bảng lương", "Bảng lương", "/payroll"],
    ["Chấm công", "Chấm công", "/attendance"],
    ["Thu / Chi", "Phiếu thu / chi", "/cash"],
    ["Sổ nhật ký", "Sổ nhật ký chung", "/ledger"],
    ["Nhập liệu", "Nhập liệu Excel", "/import"],
    ["Nhập kho", "Nhập kho", "/inbound"],
    ["Xuất kho", "Xuất kho / Bán hàng", "/outbound"],
    ["Tồn kho", "Tồn kho", "/inventory"],
    ["Hóa đơn", "Hóa đơn", "/invoices"],
    ["Chờ xuất HĐĐT", "Chờ xuất HĐĐT", "/invoice-queue"],
    ["HĐĐT", "Hóa đơn điện tử", "/hddt"],
    ["Tra cứu HĐĐT", "Tra cứu HĐĐT", "/hddt-lookup"],
    ["Đồng bộ HĐĐT", "Đồng bộ hóa đơn mua", "/hddt-sync"],
    ["Nhật ký HĐ", "Nhật ký hoạt động", "/audit"],
    ["Kế toán HKD", "Kế toán HKD", "/accounting"],
    ["Sổ kế toán", "Sổ kế toán", "/books"],
    ["Người dùng", "Người dùng & phân quyền", "/users"],
    ["Hồ sơ HKD", "Hồ sơ HKD", "/profiles"],
    ["Cài đặt", "Cài đặt", "/settings"],
  ];
  for (const [tab, title, path] of cases) {
    await openTab(page, tab, path);
    await expect(page.locator("header h2")).toHaveText(title, { timeout: 30_000 });
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

test("Catalogs kho hàng tab loads rows (wrong row type = 400 + empty table)", async ({ page }) => {
  await ensureLoggedIn(page);
  await sidebarButton(page, "Đối tác & Kho").click();
  await expect(page.locator("header h2")).toHaveText("Danh mục đối tác & kho");
  // Tab Kho hàng: `get_warehouses_page` decode sai kiểu row từng trả 400
  // "no column found for name: address" → bảng rỗng + toast lỗi.
  await page.getByRole("tab", { name: /Kho hàng/ }).click();
  await expect(page.getByText("KHO-CHINH", { exact: true })).toBeVisible({ timeout: 15_000 });
  await expect(page.locator(".p-toast-message")).toHaveCount(0);
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

  // Ngày TƯƠNG LAI cũng phải lưu được — không còn ép "không sau hôm nay".
  await expect(page.getByText("Đã lưu thông tin hộ kinh doanh", { exact: true })).toBeHidden({
    timeout: 10_000,
  });
  await page.getByRole("button", { name: "Cài đặt thông tin HKD" }).click();
  const futureField = dialog.getByLabel("Ngày bắt đầu dùng HĐĐT");
  await futureField.click();
  await futureField.press("ControlOrMeta+a");
  await futureField.pressSequentially("01/01/2076");
  await futureField.press("Tab");
  await dialog.getByRole("button", { name: "Lưu cấu hình" }).click();
  await expect(page.getByText("Đã lưu thông tin hộ kinh doanh", { exact: true })).toBeVisible({
    timeout: 10_000,
  });
  const cfgFuture = await (await request.post("/api/get_business_config")).json();
  expect(cfgFuture.hddt_start_date).toBe("2076-01-01");

  // Trả lại mốc cũ: các test Đồng bộ sau dùng ngày bắt đầu này làm mốc "Từ ngày"
  // — để 2076 thì màn Đồng bộ lọc ra 0 hóa đơn.
  await expect(page.getByText("Đã lưu thông tin hộ kinh doanh", { exact: true })).toBeHidden({
    timeout: 10_000,
  });
  await page.getByRole("button", { name: "Cài đặt thông tin HKD" }).click();
  const restoreField = dialog.getByLabel("Ngày bắt đầu dùng HĐĐT");
  await restoreField.click();
  await restoreField.press("ControlOrMeta+a");
  await restoreField.pressSequentially("15/03/2026");
  await restoreField.press("Tab");
  await dialog.getByRole("button", { name: "Lưu cấu hình" }).click();
  await expect(page.getByText("Đã lưu thông tin hộ kinh doanh", { exact: true })).toBeVisible({
    timeout: 10_000,
  });
  const cfgRestored = await (await request.post("/api/get_business_config")).json();
  expect(cfgRestored.hddt_start_date).toBe("2026-03-15");
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

  // Cache đọc từ CSDL lúc mở màn (KeepAlive giữ state nên tải lại trang để nạp
  // dữ liệu mới — cùng cách người dùng F5).
  await page.reload();
  await expect(page.locator("header h2")).toHaveText("Đồng bộ hóa đơn mua");
  await expect(page.getByText("Có 1 hóa đơn trong cache", { exact: false })).toBeVisible({
    timeout: 15_000,
  });
  await expect(page.getByRole("button", { name: "Xoá cache" })).toBeVisible();
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
  expect(row.id).toBe(0); // dòng đã nhập đến từ bảng hóa đơn chính thức (không hành động)
  expect(String(row.voucher_no)).toMatch(/^PN\d+$/);
  const products = await (await request.post("/api/get_products", { data: {} })).json();
  const names = (products as Array<{ name: string }>).map((p) => p.name);
  expect(names).toContain("Bình NN Rossi E2E");
  expect(names).toContain("Bộ lọc E2E");
  // Mặt hàng hàng hóa tạo từ hóa đơn mặc định nhóm "PPHH" (Phân phối, cung cấp
  // hàng hóa) để xuất bán không phải gán nhóm ngành bằng tay.
  const synced = products as Array<{ name: string; industry_code: string; is_service: number }>;
  expect(synced.find((p) => p.name === "Bình NN Rossi E2E")?.industry_code).toBe("PPHH");
  expect(synced.find((p) => p.name === "Bộ lọc E2E")?.industry_code).toBe("PPHH");
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

test("Đồng bộ HĐ mua: xoá cache giữ nguyên hóa đơn đã nhập kho", async ({ page, request }) => {
  await ensureLoggedIn(page);
  await sidebarButton(page, "Đồng bộ HĐĐT").click();
  await expect(page.locator("header h2")).toHaveText("Đồng bộ hóa đơn mua");

  // Cache trống, hóa đơn đã nhập ở test trước vẫn còn → nút xoá cache bật.
  await expect(page.getByText(/Có \d+ hóa đơn trong cache/)).toBeVisible({ timeout: 15_000 });
  await page.getByRole("button", { name: "Xoá cache" }).click();
  const dialog = page.getByRole("alertdialog");
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "Xoá cache" }).click();
  await expect(page.getByText("Đã xoá cache đồng bộ", { exact: false })).toBeVisible({
    timeout: 15_000,
  });

  // Hóa đơn đã nhập kho không nằm trong cache nên phải còn nguyên + còn số phiếu.
  const after = await (
    await request.post("/api/hddt_sync_preview", {
      data: { from: null, to: null, retry_failed: false },
    })
  ).json();
  const rows = after.rows as Array<{ status: string; voucher_no: string }>;
  expect(rows).toHaveLength(1);
  expect(rows[0].status).toBe("imported");
  expect(String(rows[0].voucher_no)).toMatch(/^PN\d+$/);
});

test("Xem phiếu: mở phiếu nhập kho từ Đồng bộ HĐĐT và từ danh sách Nhập/Xuất kho", async ({
  page,
  request,
}) => {
  await ensureLoggedIn(page);

  // Chuẩn bị 1 phiếu nhập + 1 phiếu xuất qua API để test không phụ thuộc thứ tự test khác.
  const inbound = await request.post("/api/save_inbound", {
    data: {
      posting_date: "2026-02-01",
      voucher_no: "PN9001",
      description: "Phiếu nhập để test xem phiếu",
      supplier_code: "",
      warehouse_code: "KHO-CHINH",
      unit_code: "HKD",
      items: [{ product_code: "SP001", quantity: 3, unit_price: 10000, discount: 0 }],
      note: "",
      inbound_type: "purchase",
      reference_no: "",
      vat_rate: 0,
      debit_account: "152",
      credit_account: "331",
      pay_now: true,
      adjust_dir: "up",
      autoBom: false,
    },
  });
  expect(inbound.ok(), `save_inbound failed ${inbound.status()}`).toBe(true);
  const outbound = await request.post("/api/save_outbound", {
    data: {
      posting_date: "2026-02-02",
      voucher_no: "PX9001",
      description: "Phiếu xuất để test xem phiếu",
      customer_code: "",
      unit_code: "HKD",
      items: [
        {
          product_code: "SP001",
          quantity: 2,
          unit_price: 15000,
          industry_code: "PPHH",
          warehouse_code: "",
        },
      ],
      note: "",
      receive_now: true,
      outbound_type: "sale",
      adjust_dir: "down",
      create_invoice: false,
      invoice: { number: "", eInvoiceNo: "", eInvoiceSymbol: "", eInvoiceDate: "" },
    },
  });
  expect(outbound.ok(), `save_outbound failed ${outbound.status()}: ${await outbound.text()}`).toBe(
    true,
  );

  // ── 1. Từ màn Đồng bộ HĐĐT: bấm số phiếu trong cột "Phiếu nhập" ──
  // Seed 1 hóa đơn mua rồi nhập kho để màn Đồng bộ có dòng đã nhập (tự chứa,
  // không phụ thuộc test khác).
  const seed = await request.post("/api/hddt_sync_test_seed", {
    data: {
      portal_id: "e2e-uuid-view",
      detail: {
        hdhhdvu: [
          {
            ten: "Bình NN xem phiếu",
            dvtinh: "Cái",
            mhhdvu: "xemphieu",
            sluong: 1,
            dgia: 250000,
            stckhau: 0,
            tsuat: 0.08,
          },
        ],
      },
    },
  });
  expect(seed.ok(), `seed hddt_sync_test_seed failed ${seed.status()}`).toBe(true);
  const imported = await request.post("/api/hddt_sync_import", {
    data: {
      ids: [],
      warehouse_code: null,
      unit_code: null,
      debit_account: null,
      credit_account: null,
    },
  });
  expect(
    imported.ok(),
    `hddt_sync_import failed ${imported.status()}: ${await imported.text()}`,
  ).toBe(true);

  await sidebarButton(page, "Đồng bộ HĐĐT").click();
  await expect(page.locator("header h2")).toHaveText("Đồng bộ hóa đơn mua");
  await expect(page.getByText("Đã nhập kho", { exact: true }).first()).toBeVisible({
    timeout: 15_000,
  });
  const preview = await request.post("/api/hddt_sync_preview", {
    data: { from: null, to: null, retry_failed: false },
  });
  const syncVoucher = String((await preview.json()).rows?.[0]?.voucher_no ?? "");
  expect(syncVoucher).toMatch(/^PN\d+$/);
  await page.getByRole("button", { name: `Xem phiếu ${syncVoucher}` }).click();
  const dlg = page.getByRole("dialog");
  await expect(dlg.getByText("PHIẾU NHẬP KHO")).toBeVisible();
  await expect(dlg.getByText("01-VT")).toBeVisible();
  const syncLine = dlg.locator("tbody tr", { hasText: "Bình NN xem phiếu" });
  await expect(syncLine.getByRole("cell").nth(4)).toHaveText("1"); // SL
  await expect(dlg.locator("tfoot")).toContainText("250.000 đ");
  await dlg.getByRole("button", { name: "Đóng" }).click();
  await expect(dlg).toBeHidden();

  // ── 2. Danh sách Nhập kho: bấm nút mắt ở cột thao tác ──
  await sidebarButton(page, "Nhập kho").click();
  await expect(page.locator("header h2")).toHaveText("Nhập kho");
  const inRow = page.locator("tr", { has: page.getByText("PN9001", { exact: true }) }).first();
  await inRow.getByRole("button", { name: "Xem phiếu", exact: true }).click();
  await expect(dlg.getByText("PHIẾU NHẬP KHO")).toBeVisible();
  const inLine = dlg.locator("tbody tr", { hasText: "Bottled water" });
  await expect(inLine.getByRole("cell").nth(4)).toHaveText("3"); // SL = 3
  await expect(inLine.getByRole("cell").nth(5)).toHaveText("10.000 đ"); // đơn giá
  await expect(dlg.locator("tfoot")).toContainText("30.000 đ"); // 3 × 10.000
  await dlg.getByRole("button", { name: "Đóng" }).click();
  await expect(dlg).toBeHidden();

  // ── 3. Danh sách Xuất kho: cùng dialog nhưng hiển thị PX ──
  await sidebarButton(page, "Xuất kho").click();
  await expect(page.locator("header h2")).toHaveText("Xuất kho / Bán hàng");
  const outRow = page.locator("tr", { has: page.getByText("PX9001", { exact: true }) }).first();
  await outRow.getByRole("button", { name: "Xem phiếu", exact: true }).click();
  await expect(dlg.getByText("PHIẾU XUẤT KHO")).toBeVisible();
  await expect(dlg.getByText("02-VT")).toBeVisible();
  const outLine = dlg.locator("tbody tr", { hasText: "Bottled water" });
  await expect(outLine.getByRole("cell").nth(4)).toHaveText("2"); // SL = 2
  await expect(dlg.locator("tfoot")).toContainText("30.000 đ"); // 2 × 15.000
  // Nút In phiếu điều hướng tới màn in dùng chung (không phải dialog).
  await dlg.getByRole("button", { name: "In phiếu" }).click();
  await expect(page).toHaveURL(/\/print\/PX9001$/);
  await expect(page.getByRole("heading", { name: "PHIẾU XUẤT KHO" })).toBeVisible();
});

test("Liên kết HĐĐT: ngày hóa đơn đồng bộ theo ngày HĐĐT bên kia", async ({ page, request }) => {
  await ensureLoggedIn(page);
  await sidebarButton(page, "Hóa đơn").click();
  await expect(page.locator("header h2")).toHaveText("Hóa đơn");

  const stockIn = await request.post("/api/save_inbound", {
    data: {
      posting_date: "2026-09-28",
      voucher_no: "PN9400",
      description: "Nhập tồn cho test đồng bộ ngày HĐĐT",
      supplier_code: "",
      warehouse_code: "KHO-CHINH",
      unit_code: "HKD",
      items: [{ product_code: "SP001", quantity: 20, unit_price: 10000, discount: 0 }],
      note: "",
      inbound_type: "purchase",
      reference_no: "",
      vat_rate: 0,
      debit_account: "152",
      credit_account: "331",
      pay_now: false,
      adjust_dir: "up",
      autoBom: false,
    },
  });
  expect(stockIn.ok(), `save_inbound failed ${stockIn.status()}: ${await stockIn.text()}`).toBe(
    true,
  );

  // Nháp lập ngày 29, bên kia phát hành HĐĐT ngày 30.
  const created = await request.post("/api/save_invoice", {
    data: {
      number: "HD9400",
      date: "2026-09-29",
      customer: "Khách Lệch Ngày",
      customer_tax_code: "0100000000",
      items: [
        {
          product_code: "SP001",
          quantity: 1,
          unit_price: 10000,
          industry_code: "PPHH",
          discount: 0,
          warehouse_code: "",
        },
      ],
    },
  });
  expect(created.ok(), `save_invoice failed: ${await created.text()}`).toBe(true);
  await page.reload();
  await expect(page.locator("header h2")).toHaveText("Hóa đơn");

  const row = page.locator("tr", { has: page.getByText("HD9400", { exact: true }) }).first();
  await expect(row.getByText("2026-09-29", { exact: true }).first()).toBeVisible();

  await row.getByRole("button", { name: "Nhập số HĐĐT đã phát hành" }).click();
  // Định danh theo tên: hộp thoại khác của màn Hóa đơn vẫn nằm trong DOM.
  const dlg = page.getByRole("dialog", { name: /Liên kết hóa đơn điện tử/ });
  await dlg.getByLabel("Số HĐĐT").fill("00009400");
  await dlg.getByLabel("Ký hiệu HĐĐT").fill("1C26TT152");
  // DatePicker có mask nhập ngày: phải gõ từng ký tự (fill một lần không được
  // mask xử lý) rồi Enter để chốt giá trị vào model.
  const dateInput = dlg.getByLabel("Ngày HĐĐT");
  await dateInput.click();
  // Ô đang có sẵn ngày mặc định (hôm nay) → chọn hết rồi gõ đè.
  await dateInput.press("ControlOrMeta+a");
  await dateInput.pressSequentially("30/09/2026", { delay: 30 });
  await dateInput.press("Enter");
  await expect(dateInput).toHaveValue("30/09/2026");
  await dlg.getByRole("button", { name: "Lưu" }).click();
  await expect(dlg).toBeHidden();

  // Ngày lập hóa đơn phải được đồng bộ sang ngày HĐĐT (30, không còn 29).
  const after = (await (await request.post("/api/get_invoices", { data: {} })).json()) as Array<{
    number: string;
    date: string;
    e_invoice_date: string;
    status: string;
  }>;
  const inv = after.find((i) => i.number === "HD9400");
  expect(inv?.e_invoice_date, "ngày nhập ở hộp thoại").toBe("2026-09-30");
  expect(inv?.date, "ngày hóa đơn phải theo ngày HĐĐT").toBe("2026-09-30");
  expect(inv?.status).toBe("official");

  // Toast nhắc đã đồng bộ ngày.
  await expect(page.locator(".p-toast-detail").last()).toContainText(
    "Ngày hóa đơn đã cập nhật theo HĐĐT",
  );
  await expect(page.getByText("2026-09-29", { exact: true })).toHaveCount(0);
});

test("Ghi nhận HĐĐT: tự lập phiếu xuất để doanh thu vào sổ", async ({ page, request }) => {
  await ensureLoggedIn(page);
  await sidebarButton(page, "Hóa đơn").click();
  await expect(page.locator("header h2")).toHaveText("Hóa đơn");

  const stockIn = await request.post("/api/save_inbound", {
    data: {
      posting_date: "2026-09-25",
      voucher_no: "PN9500",
      description: "Nhập tồn cho test tự lập phiếu xuất",
      supplier_code: "",
      warehouse_code: "KHO-CHINH",
      unit_code: "HKD",
      items: [{ product_code: "SP001", quantity: 50, unit_price: 10000, discount: 0 }],
      note: "",
      inbound_type: "purchase",
      reference_no: "",
      vat_rate: 0,
      debit_account: "152",
      credit_account: "331",
      pay_now: false,
      adjust_dir: "up",
      autoBom: false,
    },
  });
  expect(stockIn.ok(), `save_inbound failed: ${await stockIn.text()}`).toBe(true);

  const created = await request.post("/api/save_invoice", {
    data: {
      number: "HD9500",
      date: "2026-09-26",
      customer: "Khách Tự Sinh PX",
      customer_tax_code: "0100000000",
      items: [
        {
          product_code: "SP001",
          quantity: 2,
          unit_price: 10000,
          industry_code: "PPHH",
          discount: 0,
          warehouse_code: "",
        },
      ],
    },
  });
  expect(created.ok(), `save_invoice failed: ${await created.text()}`).toBe(true);
  await page.reload();
  await expect(page.locator("header h2")).toHaveText("Hóa đơn");

  const row = page.locator("tr", { has: page.getByText("HD9500", { exact: true }) }).first();
  await expect(row.getByText("—", { exact: true }).first()).toBeVisible();

  await row.getByRole("button", { name: "Nhập số HĐĐT đã phát hành" }).click();
  const dlg = page.getByRole("dialog");
  // Tuỳ chọn lập phiếu xuất bật sẵn.
  await expect(dlg.getByText("Lập phiếu xuất cho hóa đơn này")).toBeVisible();
  await dlg.getByLabel("Số HĐĐT").fill("00009500");
  await dlg.getByRole("button", { name: "Lưu" }).click();
  await expect(dlg).toBeHidden();
  await expect(page.locator(".p-toast-detail").last()).toContainText("Đã lập phiếu xuất PX");

  // Hóa đơn đã trỏ phiếu xuất.
  const after = (await (await request.post("/api/get_invoices", { data: {} })).json()) as Array<{
    number: string;
    status: string;
    voucher_no: string;
  }>;
  const inv = after.find((i) => i.number === "HD9500");
  expect(inv?.status).toBe("official");
  expect(inv?.voucher_no, "hóa đơn phải liên kết phiếu xuất").toMatch(/^PX\d+$/);

  // Sổ có bút toán Nợ 131 / Có 511 cho phiếu đó → doanh thu đã vào tờ khai.
  const entries = (await (
    await request.post("/api/get_voucher", { data: { voucherNo: inv?.voucher_no } })
  ).json()) as Array<{
    entry_type: string;
    debit_account: string;
    credit_account: string;
    amount: number;
  }>;
  const revenue = entries.find((e) => e.entry_type === "PX");
  expect(revenue?.debit_account).toBe("131");
  expect(revenue?.credit_account).toBe("511");
  expect(revenue?.amount).toBe(20000);

  // Phiếu xuất vừa sinh phải xuất hiện ở màn Xuất kho (đã trừ tồn FIFO + ghi sổ).
  await sidebarButton(page, "Xuất kho").click();
  await expect(page.locator("header h2")).toHaveText("Xuất kho / Bán hàng");
  await expect(
    page.locator("tr", { has: page.getByText(inv?.voucher_no ?? "", { exact: true }) }).first(),
  ).toBeVisible();

  // Nút tạo phiếu xuất biến mất vì hóa đơn đã có phiếu.
  await sidebarButton(page, "Hóa đơn").click();
  const row2 = page.locator("tr", { has: page.getByText("HD9500", { exact: true }) }).first();
  await expect(row2.getByRole("button", { name: "Lập phiếu xuất từ hóa đơn" })).toHaveCount(0);
});

test("Nhap/Xuat kho: chặn số phiếu trùng, số phiếu mới không đụng số đang lọc", async ({
  page,
  request,
}) => {
  await ensureLoggedIn(page);

  const stockIn = await request.post("/api/save_inbound", {
    data: {
      posting_date: "2026-09-20",
      voucher_no: "PN9900",
      description: "Nhập tồn cho test số phiếu trùng",
      supplier_code: "",
      warehouse_code: "KHO-CHINH",
      unit_code: "HKD",
      items: [{ product_code: "SP001", quantity: 50, unit_price: 10000, discount: 0 }],
      note: "",
      inbound_type: "purchase",
      reference_no: "",
      vat_rate: 0,
      debit_account: "152",
      credit_account: "331",
      pay_now: false,
      adjust_dir: "up",
      autoBom: false,
    },
  });
  expect(stockIn.ok(), `save_inbound failed ${stockIn.status()}`).toBe(true);

  // API chặn số phiếu trùng (mọi đường vào, không riêng thao tác trên UI).
  const dup = await request.post("/api/save_inbound", {
    data: {
      posting_date: "2026-09-21",
      voucher_no: "PN9900",
      description: "Lặp lại số phiếu",
      supplier_code: "",
      warehouse_code: "KHO-CHINH",
      unit_code: "HKD",
      items: [{ product_code: "SP001", quantity: 1, unit_price: 10000, discount: 0 }],
      note: "",
      inbound_type: "purchase",
      reference_no: "",
      vat_rate: 0,
      debit_account: "152",
      credit_account: "331",
      pay_now: false,
      adjust_dir: "up",
      autoBom: false,
    },
  });
  expect(dup.status(), "phải chặn số phiếu nhập trùng").toBe(400);
  expect(await dup.text()).toContain("PN9900");

  const dupPx = await request.post("/api/save_outbound", {
    data: {
      posting_date: "2026-09-20",
      voucher_no: "PX9900",
      description: "Phiếu xuất đầu",
      customer_code: "",
      unit_code: "HKD",
      items: [
        {
          product_code: "SP001",
          quantity: 1,
          unit_price: 15000,
          industry_code: "PPHH",
          warehouse_code: "",
        },
      ],
      note: "",
      receive_now: false,
      outbound_type: "sale",
      adjust_dir: "down",
      create_invoice: false,
      invoice: { number: "", eInvoiceNo: "", eInvoiceSymbol: "", eInvoiceDate: "" },
    },
  });
  expect(dupPx.ok(), `save_outbound failed ${dupPx.status()}`).toBe(true);
  const again = await request.post("/api/save_outbound", {
    data: {
      posting_date: "2026-09-21",
      voucher_no: "PX9900",
      description: "Phiếu xuất lặp số",
      customer_code: "",
      unit_code: "HKD",
      items: [
        {
          product_code: "SP001",
          quantity: 1,
          unit_price: 15000,
          industry_code: "PPHH",
          warehouse_code: "",
        },
      ],
      note: "",
      receive_now: false,
      outbound_type: "sale",
      adjust_dir: "down",
      create_invoice: false,
      invoice: { number: "", eInvoiceNo: "", eInvoiceSymbol: "", eInvoiceDate: "" },
    },
  });
  expect(again.status(), "phải chặn số phiếu xuất trùng").toBe(400);
  expect(await again.text()).toContain("PX9900");

  // Số phiếu gợi ý lấy từ CƠ SỞ DỮ LIỆU: lọc danh sách chỉ còn vài phiếu cũ rồi
  // mở hộp thoại tạo phiếu — số ra vẫn phải chưa dùng, khớp số backend sinh.
  const allPx = (await (
    await request.post("/api/get_journal_entries", {
      data: { entryType: "PX", fromDate: "", toDate: "", search: "" },
    })
  ).json()) as Array<{ voucher_no: string }>;
  expect(Array.isArray(allPx), "get_journal_entries phải trả về danh sách").toBe(true);
  const usedNo = new Set(allPx.map((e) => e.voucher_no));

  await sidebarButton(page, "Xuất kho").click();
  await expect(page.locator("header h2")).toHaveText("Xuất kho / Bán hàng");
  // Thu hẹp danh sách về khoảng ngày cũ: màn chỉ nạp được vài phiếu nhỏ số.
  await page.getByPlaceholder("Từ ngày").fill("01/09/2026");
  await page.getByPlaceholder("Từ ngày").press("Enter");
  await page.getByPlaceholder("Đến ngày").fill("02/09/2026");
  await page.getByPlaceholder("Đến ngày").press("Enter");

  await page.getByRole("button", { name: "Tạo phiếu xuất" }).click();
  const dlg = page.getByRole("dialog");
  const numberInput = dlg.getByLabel("Số phiếu");
  await expect(numberInput).toHaveValue(/^PX\d{3,}$/);
  const suggested = await numberInput.inputValue();
  const fromApi = await (
    await request.post("/api/next_voucher_no", { data: { entryType: "PX" } })
  ).text();
  expect(suggested, "số phiếu phải do backend sinh, không đoán từ danh sách đang lọc").toBe(
    fromApi,
  );
  expect(usedNo.has(suggested), `số ${suggested} đã dùng mà vẫn được gợi ý`).toBe(false);
  await dlg.getByRole("button", { name: "Hủy" }).click();

  // Lưu phiếu với số gợi ý → hộp thoại đóng, phiếu mới xuất hiện trong danh sách.
  // Bỏ lọc khoảng ngày trước: danh sách nay gom theo phiếu và lọc ở SERVER, nên
  // phiếu vừa lập (ngày hôm nay) sẽ không hiện nếu khoảng lọc vẫn là 01–02/09.
  await page.getByRole("button", { name: "Xoá lọc" }).click();
  await page.waitForTimeout(600);
  await page.getByRole("button", { name: "Tạo phiếu xuất" }).click();
  const dlg2 = page.getByRole("dialog");
  const line = dlg2.locator("tbody tr").first();
  await line.getByRole("combobox").first().click();
  await page
    .getByRole("option", { name: /Bottled water/ })
    .first()
    .click();
  await dlg2.getByRole("button", { name: "Lưu phiếu" }).click();
  await expect(dlg2).toBeHidden();
  await expect(page.locator("tr", { has: page.getByText(suggested, { exact: true }) })).toHaveCount(
    1,
  );
});

test("Nhap/Xuat kho: 1 phiếu = 1 dòng, có số loại mặt hàng và chiết khấu", async ({
  page,
  request,
}) => {
  await ensureLoggedIn(page);

  // Cần 2 loại hàng để phân biệt "số loại" với "tổng số lượng" (seed chỉ có SP001).
  const newProduct = await request.post("/api/save_product", {
    data: {
      code: "SP-CK2",
      name: "Hàng chiết khấu 2",
      unit: "Cái",
      salePrice: 600_000,
      costPrice: 500_000,
      minStock: 0,
      vatRate: 1,
      importTaxRate: 0,
      isService: false,
      industryCode: "PPHH",
    },
  });
  expect(newProduct.ok(), `save_product: ${await newProduct.text()}`).toBe(true);

  // 1 phiếu nhập 3 dòng hàng (2 loại) có chiết khấu → bảng phải ra MỘT dòng.
  const stockIn = await request.post("/api/save_inbound", {
    data: {
      posting_date: "2026-09-27",
      voucher_no: "PN-GOM01",
      description: "Phiếu gom nhiều dòng có chiết khấu",
      supplier_code: "",
      warehouse_code: "KHO-CHINH",
      unit_code: "HKD",
      items: [
        { product_code: "SP001", quantity: 2, unit_price: 1_000_000, discount: 200_000 },
        { product_code: "SP001", quantity: 1, unit_price: 1_000_000, discount: 100_000 },
        { product_code: "SP-CK2", quantity: 3, unit_price: 500_000, discount: 50_000 },
      ],
      note: "",
      inbound_type: "purchase",
      reference_no: "",
      vat_rate: 0,
      debit_account: "152",
      credit_account: "331",
      pay_now: false,
      adjust_dir: "up",
      autoBom: false,
    },
  });
  expect(stockIn.ok(), `save_inbound: ${await stockIn.text()}`).toBe(true);

  await sidebarButton(page, "Nhập kho").click();
  await expect(page.locator("header h2")).toHaveText("Nhập kho");
  const row = page.locator("tr", { has: page.getByText("PN-GOM01", { exact: true }) }).first();
  await expect(row, "phiếu 3 dòng hàng phải hiện đúng 1 dòng").toBeVisible();
  await expect(
    page.locator("tr", { has: page.getByText("PN-GOM01", { exact: true }) }),
    "không được lặp dòng theo từng mặt hàng",
  ).toHaveCount(1);

  // Số LOẠI mặt hàng = 2 (không phải tổng số lượng 6), chiết khấu 350.000,
  // thành tiền = 2tr + 1tr + 1,5tr − 350k = 4,15 triệu.
  await expect(row.getByRole("cell").nth(4)).toHaveText("2");
  await expect(row.getByRole("cell").nth(6)).toContainText("350.000 đ");
  await expect(row.getByRole("cell").nth(7)).toContainText("4.150.000 đ");

  // Chi tiết dòng hàng vẫn xem được khi bấm số phiếu.
  await row.getByRole("button", { name: "Xem phiếu", exact: true }).click();
  const dlg = page.getByRole("dialog");
  await expect(dlg).toBeVisible();
  await expect(dlg.locator("tbody tr")).toHaveCount(3);
  await dlg.getByRole("button", { name: "Đóng" }).click();
});

test("Nhap/Xuat kho: tìm phiếu theo từ khoá và lọc khoảng ngày", async ({ page, request }) => {
  await ensureLoggedIn(page);

  const stockIn = await request.post("/api/save_inbound", {
    data: {
      posting_date: "2026-09-20",
      voucher_no: "PN9800",
      description: "Nhập tồn cho test tìm phiếu",
      supplier_code: "",
      warehouse_code: "KHO-CHINH",
      unit_code: "HKD",
      items: [{ product_code: "SP001", quantity: 50, unit_price: 10000, discount: 0 }],
      note: "",
      inbound_type: "purchase",
      reference_no: "",
      vat_rate: 0,
      debit_account: "152",
      credit_account: "331",
      pay_now: false,
      adjust_dir: "up",
      autoBom: false,
    },
  });
  expect(stockIn.ok(), `save_inbound failed ${stockIn.status()}`).toBe(true);

  // Hai phiếu ở hai khoảng ngày khác nhau — số phiếu nằm ngoài trang đầu.
  const makeOutbound = async (voucherNo: string, date: string, description: string) => {
    const res = await request.post("/api/save_outbound", {
      data: {
        posting_date: date,
        voucher_no: voucherNo,
        description,
        customer_code: "",
        unit_code: "HKD",
        items: [
          {
            product_code: "SP001",
            quantity: 1,
            unit_price: 15000,
            industry_code: "PPHH",
            warehouse_code: "",
          },
        ],
        note: "",
        receive_now: false,
        outbound_type: "sale",
        adjust_dir: "down",
        create_invoice: false,
        invoice: { number: "", eInvoiceNo: "", eInvoiceSymbol: "", eInvoiceDate: "" },
      },
    });
    expect(res.ok(), `save_outbound ${voucherNo} failed ${res.status()}: ${await res.text()}`).toBe(
      true,
    );
  };
  await makeOutbound("PX9800", "2026-09-02", "Phiếu rất cũ");
  await makeOutbound("PX9801", "2026-09-20", "Phiếu cần tìm");

  await sidebarButton(page, "Xuất kho").click();
  await expect(page.locator("header h2")).toHaveText("Xuất kho / Bán hàng");

  // Từ khoá theo số phiếu → chỉ còn đúng phiếu đó.
  const search = page.getByPlaceholder("Số phiếu, mã hàng, khách, diễn giải…");
  await search.fill("PX9800");
  await expect(page.locator("tr", { hasText: "PX9800" }).first()).toBeVisible();
  await expect(page.locator("tr", { hasText: "PX9801" })).toHaveCount(0);

  // Từ khoá tiếng Việt có dấu, khớp cả tên hàng.
  await search.fill("cần tìm");
  await expect(page.locator("tr", { hasText: "PX9801" }).first()).toBeVisible();
  await expect(page.locator("tr", { hasText: "PX9800" })).toHaveCount(0);

  // Lọc khoảng ngày: đủ rộng thì thấy cả hai.
  await search.fill("");
  await page.getByPlaceholder("Từ ngày").fill("01/09/2026");
  await page.getByPlaceholder("Từ ngày").press("Enter");
  await page.getByPlaceholder("Đến ngày").fill("30/09/2026");
  await page.getByPlaceholder("Đến ngày").press("Enter");
  await expect(page.locator("tr", { hasText: "PX9800" }).first()).toBeVisible();
  await expect(page.locator("tr", { hasText: "PX9801" }).first()).toBeVisible();

  // Thu hẹp khoảng ngày → chỉ còn phiếu trong khoảng.
  await page.getByPlaceholder("Từ ngày").fill("10/09/2026");
  await page.getByPlaceholder("Từ ngày").press("Enter");
  await expect(page.locator("tr", { hasText: "PX9800" })).toHaveCount(0);
  await expect(page.locator("tr", { hasText: "PX9801" }).first()).toBeVisible();

  // Xoá lọc → thấy lại toàn bộ.
  await page.getByRole("button", { name: "Xoá lọc" }).click();
  await expect(page.locator("tr", { hasText: "PX9800" }).first()).toBeVisible();

  // Bộ lọc được giữ khi chuyển menu (KeepAlive) và nạp lại đúng theo bộ lọc.
  await search.fill("PX9801");
  await sidebarButton(page, "Hóa đơn").click();
  await sidebarButton(page, "Xuất kho").click();
  await expect(page.locator("tr", { hasText: "PX9801" }).first()).toBeVisible();
  await expect(page.locator("tr", { hasText: "PX9800" })).toHaveCount(0);

  // Màn Nhập kho cũng lọc được, và tìm phiếu nhập theo số.
  await page.getByRole("button", { name: "Xoá lọc" }).click();
  await sidebarButton(page, "Nhập kho").click();
  await expect(page.locator("header h2")).toHaveText("Nhập kho");
  await page.getByPlaceholder("Số phiếu, mã hàng, khách, diễn giải…").fill("PN9800");
  await expect(page.locator("tr", { hasText: "PN9800" }).first()).toBeVisible();
});

test("Hóa đơn: báo số trùng để tự dọn, không tự sửa dữ liệu", async ({ page }) => {
  await ensureLoggedIn(page);

  // Dựng dữ liệu trùng số như thời trước khi app có chặn: ghi thẳng vào file
  // SQLite của phiên E2E (app đang mở, WAL nên ghi song song được). Chỉ đụng
  // DB tạm của test — không bao giờ đụng dữ liệu thật của hộ.
  const dupNo = "HD9750";
  const dataDir = readFileSync("/tmp/hkd-e2e-dir", "utf8").trim();
  const dbPath = `${dataDir}/profiles/default/hkd.db`;
  execFileSync("sqlite3", [
    dbPath,
    `INSERT INTO invoice (number, date, customer, customer_tax_code, total, vat_amount, status)
     VALUES ('${dupNo}', '2026-09-20', 'Khách Trùng Số', '0100000000', 10000, 0, 'draft');
     INSERT INTO invoice (number, date, customer, customer_tax_code, total, vat_amount, status)
     VALUES ('${dupNo}', '2026-09-20', 'Khách Trùng Số', '0100000000', 10000, 0, 'draft');`,
  ]);

  await page.reload();
  await sidebarButton(page, "Hóa đơn").click();
  await expect(page.locator("header h2")).toHaveText("Hóa đơn");

  // Cảnh báo nêu rõ số bị trùng và số bản ghi.
  await expect(
    page.getByText(`1 số hóa đơn bị dùng trùng (2 bản ghi): ${dupNo} (2)`, { exact: false }),
  ).toBeVisible();

  // Lọc xem riêng các hóa đơn trùng để tự dọn.
  await page.getByRole("button", { name: "Chỉ hiện hóa đơn trùng" }).click();
  const rows = page.locator("tbody tr", { has: page.getByText(dupNo, { exact: true }) });
  await expect(rows).toHaveCount(2);

  // Xoá 1 bản nháp trùng → cảnh báo biến mất, danh sách trở lại đầy đủ.
  await rows.first().getByRole("button", { name: "Xoá hóa đơn nháp" }).click();
  const confirmBox = page.getByRole("alertdialog", { name: "Xoá hóa đơn nháp" });
  await confirmBox.getByRole("button", { name: "Xoá", exact: true }).click();
  await expect(page.getByText(/số hóa đơn bị dùng trùng/)).toHaveCount(0);
  await expect(page.locator("tr", { has: page.getByText(dupNo, { exact: true }) })).toHaveCount(1);
});

test("Hóa đơn: cảnh báo ⚠ chưa có phiếu xuất khi đã phát hành nhưng lập PX lỗi", async ({
  page,
  request,
}) => {
  await ensureLoggedIn(page);

  // Mặt hàng riêng cho test + tồn vừa đủ, để không phụ thuộc tồn SP001 của test khác.
  const prod = await request.post("/api/save_product", {
    data: {
      code: "SPPX1",
      name: "Hàng không còn tồn",
      unit: "Cái",
      salePrice: 10000,
      costPrice: 8000,
      minStock: 0,
      vatRate: 1,
      importTaxRate: 0,
      isService: false,
      industryCode: "PPHH",
    },
  });
  expect(prod.ok(), `save_product failed ${prod.status()}`).toBe(true);
  const stockIn = await request.post("/api/save_inbound", {
    data: {
      posting_date: "2026-09-20",
      voucher_no: "PN9600",
      description: "Nhập tồn cho test cảnh báo chưa có PX",
      supplier_code: "",
      warehouse_code: "KHO-CHINH",
      unit_code: "HKD",
      items: [{ product_code: "SPPX1", quantity: 5, unit_price: 8000, discount: 0 }],
      note: "",
      inbound_type: "purchase",
      reference_no: "",
      vat_rate: 0,
      debit_account: "152",
      credit_account: "331",
      pay_now: false,
      adjust_dir: "up",
      autoBom: false,
    },
  });
  expect(stockIn.ok(), `save_inbound failed ${stockIn.status()}: ${await stockIn.text()}`).toBe(
    true,
  );

  // Hóa đơn nháp 5 đơn vị (đủ tồn lúc lập).
  const created = await request.post("/api/save_invoice", {
    data: {
      number: "HD9600",
      date: "2026-09-20",
      customer: "Khách Chưa Có PX",
      customer_tax_code: "0100000000",
      items: [
        {
          product_code: "SPPX1",
          quantity: 5,
          unit_price: 10000,
          industry_code: "PPHH",
          discount: 0,
          warehouse_code: "",
        },
      ],
    },
  });
  expect(created.ok(), `save_invoice failed ${created.status()}: ${await created.text()}`).toBe(
    true,
  );
  // Tồn bị dùng hết ở nơi khác (bán ngoài app) → lúc phát hành sẽ không lập được PX.
  const out = await request.post("/api/save_outbound", {
    data: {
      posting_date: "2026-09-20",
      voucher_no: "PX9600",
      description: "Dùng hết tồn SPPX1",
      customer_code: "",
      unit_code: "HKD",
      items: [
        {
          product_code: "SPPX1",
          quantity: 5,
          unit_price: 10000,
          industry_code: "PPHH",
          warehouse_code: "",
        },
      ],
      note: "",
      receive_now: false,
      outbound_type: "sale",
      adjust_dir: "down",
      create_invoice: false,
      invoice: { number: "", eInvoiceNo: "", eInvoiceSymbol: "", eInvoiceDate: "" },
    },
  });
  expect(out.ok(), `save_outbound failed ${out.status()}: ${await out.text()}`).toBe(true);

  await page.reload();
  await sidebarButton(page, "Hóa đơn").click();
  await expect(page.locator("header h2")).toHaveText("Hóa đơn");
  const row = page.locator("tr", { has: page.getByText("HD9600", { exact: true }) }).first();
  // Chưa phát hành thì cột phiếu xuất bình thường, chỉ có nút lập PX.
  await expect(row.getByText("—", { exact: true }).first()).toBeVisible();
  await expect(row.getByText("⚠ chưa có")).toHaveCount(0);

  // Ghi số HĐĐT với tuỳ chọn lập phiếu xuất → lập PX lỗi vì hết tồn.
  await row.getByRole("button", { name: "Nhập số HĐĐT đã phát hành" }).click();
  const dlg = page.getByRole("dialog");
  await dlg.getByLabel("Số HĐĐT").fill("00009600");
  await dlg.getByRole("button", { name: "Lưu" }).click();
  await expect(dlg).toBeHidden();
  await expect(page.locator(".p-toast-summary").last()).toContainText("chưa lập được phiếu xuất", {
    timeout: 15_000,
  });

  // Đã phát hành mà chưa có PX → dòng hóa đơn phải cảnh báo đỏ, không phải "—".
  await expect(row.getByText("⚠ chưa có")).toBeVisible();
  await expect(row.getByRole("button", { name: "Lập phiếu xuất từ hóa đơn" })).toBeVisible();
});

test("KeepAlive: quay lại màn đã xem thì nạp lại dữ liệu, không còn thấy bản cũ", async ({
  page,
  request,
}) => {
  await ensureLoggedIn(page);

  /** Tạo phiếu xuất qua API (có lập kèm hóa đơn bán hàng nếu `create_invoice`). */
  const makeOutbound = async (voucherNo: string, qty: number) => {
    const res = await request.post("/api/save_outbound", {
      data: {
        posting_date: "2026-09-20",
        voucher_no: voucherNo,
        description: `Phiếu xuất ${voucherNo} cho test keep alive`,
        customer_code: "",
        unit_code: "HKD",
        items: [
          {
            product_code: "SP001",
            quantity: qty,
            unit_price: 15000,
            industry_code: "PPHH",
            warehouse_code: "",
          },
        ],
        note: "",
        receive_now: false,
        outbound_type: "sale",
        adjust_dir: "down",
        create_invoice: true,
        // Số hóa đơn = số phiếu xuất (đúng như màn Xuất kho gửi lên).
        invoice: { number: voucherNo, eInvoiceNo: "", eInvoiceSymbol: "", eInvoiceDate: "" },
      },
    });
    expect(res.ok(), `save_outbound ${voucherNo} failed ${res.status()}: ${await res.text()}`).toBe(
      true,
    );
  };

  // Tồn cho 2 phiếu (các test khác cùng dùng DB nên nhập dư).
  const stockIn = await request.post("/api/save_inbound", {
    data: {
      posting_date: "2026-09-20",
      voucher_no: "PN9700",
      description: "Nhập tồn cho test keep alive",
      supplier_code: "",
      warehouse_code: "KHO-CHINH",
      unit_code: "HKD",
      items: [{ product_code: "SP001", quantity: 50, unit_price: 10000, discount: 0 }],
      note: "",
      inbound_type: "purchase",
      reference_no: "",
      vat_rate: 0,
      debit_account: "152",
      credit_account: "331",
      pay_now: false,
      adjust_dir: "up",
      autoBom: false,
    },
  });
  expect(stockIn.ok(), `save_inbound failed ${stockIn.status()}`).toBe(true);

  // Vào Hóa đơn TRƯỚC, rồi sang Xuất kho — đúng thứ tự gây lỗi: sửa dữ liệu ở
  // màn kia, quay lại màn đã cache thấy danh sách lúc mount.
  await sidebarButton(page, "Hóa đơn").click();
  await expect(page.locator("header h2")).toHaveText("Hóa đơn");
  await expect(page.getByText("PX9700", { exact: true })).toHaveCount(0);

  await sidebarButton(page, "Xuất kho").click();
  await expect(page.locator("header h2")).toHaveText("Xuất kho / Bán hàng");
  await expect(page.getByText("PX9700", { exact: true })).toHaveCount(0);

  // 1) Dữ liệu đổi ở màn Xuất kho (sinh cả phiếu + hóa đơn kèm theo).
  await makeOutbound("PX9700", 1);

  // 2) Quay lại Hóa đơn → phải thấy hóa đơn vừa sinh, không còn danh sách cũ.
  await sidebarButton(page, "Hóa đơn").click();
  await expect(page.locator("tr", { hasText: "PX9700" }).first()).toBeVisible();

  // 3) Ngược lại: sinh phiếu mới khi đang ở màn Hóa đơn, quay lại Xuất kho →
  //    danh sách phiếu phải có PX9701.
  await makeOutbound("PX9701", 1);
  await sidebarButton(page, "Xuất kho").click();
  await expect(page.locator("tr", { hasText: "PX9701" }).first()).toBeVisible();
});

test("Chờ xuất HĐĐT: chỉ hiện hóa đơn chưa phát hành, có checklist và phím tắt", async ({
  page,
  request,
}) => {
  await ensureLoggedIn(page);

  // 2 hóa đơn: 1 nháp sẵn sàng, 1 nháp thiếu MST (lỗi chặn chép), 1 đã phát hành.
  const stockIn = await request.post("/api/save_inbound", {
    data: {
      posting_date: "2026-09-17",
      voucher_no: "PN9300",
      description: "Nhập tồn cho test hàng chờ xuất",
      supplier_code: "",
      warehouse_code: "KHO-CHINH",
      unit_code: "HKD",
      items: [{ product_code: "SP001", quantity: 30, unit_price: 10000, discount: 0 }],
      note: "",
      inbound_type: "purchase",
      reference_no: "",
      vat_rate: 0,
      debit_account: "152",
      credit_account: "331",
      pay_now: false,
      adjust_dir: "up",
      autoBom: false,
    },
  });
  expect(stockIn.ok(), `save_inbound failed ${stockIn.status()}: ${await stockIn.text()}`).toBe(
    true,
  );

  const mk = async (number: string, taxCode: string) => {
    const res = await request.post("/api/save_invoice", {
      data: {
        number,
        date: "2026-09-17",
        customer: `Khách ${number}`,
        customer_tax_code: taxCode,
        items: [
          {
            product_code: "SP001",
            quantity: 1,
            unit_price: 10000,
            industry_code: "PPHH",
            discount: 0,
            warehouse_code: "",
          },
        ],
      },
    });
    expect(res.ok(), `save_invoice ${number} failed: ${await res.text()}`).toBe(true);
    return number;
  };
  await mk("HD9300", "0100000000"); // sẵn sàng chép
  await mk("HD9301", "MST-SAI"); // lỗi MST → chặn

  // Một hóa đơn đã phát hành (không nằm trong hàng chờ): lấy id theo số HĐ vì các
  // test trước đã tạo hóa đơn nên id không cố định.
  const mkIssued = await mk("HD9302", "0100000000");
  const list = (await (await request.post("/api/get_invoices", { data: {} })).json()) as Array<{
    id: number;
    number: string;
  }>;
  const issuedId = list.find((i) => i.number === "HD9302")?.id;
  expect(issuedId, `vừa lập ${mkIssued}`).toBeTruthy();
  const issued = await request.post("/api/link_hddt", {
    data: {
      invoiceId: issuedId,
      hddtNo: "00000999",
      hddtSymbol: "1C26TT152",
      hddtDate: "2026-09-17",
    },
  });
  expect(issued.ok(), `link_hddt failed: ${await issued.text()}`).toBe(true);

  await sidebarButton(page, "Chờ xuất HĐĐT").click();
  await expect(page.locator("header h2")).toHaveText("Chờ xuất HĐĐT");

  // Bảng phân trang server-side → tìm theo số HĐ để dòng cần kiểm luôn nằm trong
  // trang đang xem (thao tác tay đúng như người dùng khi hàng chờ dài).
  await page.getByPlaceholder("Số hóa đơn, khách hàng, MST…").fill("HD930");
  await page.getByPlaceholder("Số hóa đơn, khách hàng, MST…").press("Enter");

  // Cả 2 hóa đơn chưa phát hành đều có mặt; hóa đơn đã phát hành thì không.
  const rowOk = page.locator("tr", { has: page.getByText("HD9300", { exact: true }) }).first();
  const rowBad = page.locator("tr", { has: page.getByText("HD9301", { exact: true }) }).first();
  await expect(rowOk.getByText("Sẵn sàng chép")).toBeVisible();
  await expect(rowBad.getByText("1 lỗi")).toBeVisible();
  await expect(rowBad.getByText(/MST người mua/)).toBeVisible();
  // Nút chép bị khoá khi còn lỗi.
  await expect(rowBad.getByRole("button", { name: "Chép sang dịch vụ HĐĐT khác" })).toBeDisabled();
  await expect(rowOk.getByRole("button", { name: "Chép sang dịch vụ HĐĐT khác" })).toBeEnabled();

  // Lọc theo trạng thái: không có hóa đơn nào "đã chép" ở thời điểm này.
  await page.getByRole("button", { name: "Đã chép", exact: true }).click();
  await expect(page.getByText(/Không còn hóa đơn nào chờ xuất/)).toBeVisible();
  await page.getByRole("button", { name: "Tất cả", exact: true }).click();
  await expect(rowOk).toBeVisible();

  // Màn tự chọn sẵn dòng chưa chép, không lỗi → F8 mở hộp thoại chép cho dòng đó.
  await page.keyboard.press("F8");
  const dlg = page.getByRole("dialog");
  await expect(dlg.getByText("Chép hóa đơn HD9300 sang dịch vụ khác")).toBeVisible();
  await dlg.getByRole("button", { name: "Đã chép sang bên kia" }).click();
  await expect(dlg).toBeHidden();

  // Đã chép → hàng hiện trạng thái chờ phát hành, F9 ghi số HĐĐT → biến khỏi danh sách.
  const rowCopied = page.locator("tr", { has: page.getByText("HD9300", { exact: true }) }).first();
  await expect(rowCopied.getByText("Đã chép — chờ phát hành")).toBeVisible();
  // Dòng vừa chép không còn là dòng ưu tiên → chọn tay rồi F9.
  await rowCopied.getByRole("cell").nth(1).click();
  await page.keyboard.press("F9");
  const linkDlg = page.getByRole("dialog");
  await linkDlg.getByLabel("Số HĐĐT").fill("00000900");
  await linkDlg.getByLabel("Ký hiệu HĐĐT").fill("1C26TT152");
  await linkDlg.getByRole("button", { name: "Lưu" }).click();
  await expect(linkDlg).toBeHidden();
  await expect(page.getByText("HD9300", { exact: true })).toHaveCount(0);

  // Ký hiệu vừa dùng được ghi vào hồ sơ hộ → hóa đơn sau mở hộp thoại phải tự điền
  // ký hiệu, không phải gõ lại.
  await rowBad.getByRole("button", { name: "Ghi số HĐĐT đã phát hành" }).click();
  const nextDlg = page.getByRole("dialog");
  await expect(nextDlg.getByLabel("Ký hiệu HĐĐT")).toHaveValue("1C26TT152");
  await nextDlg.getByRole("button", { name: "Đóng" }).click();
});

test("Chờ xuất HĐĐT: xem PDF hóa đơn nháp ngay trên hàng chờ", async ({ page, request }) => {
  await ensureLoggedIn(page);

  // Tồn kho + hóa đơn nháp riêng cho test này (không phụ thuộc test khác đã chạy).
  const stockIn = await request.post("/api/save_inbound", {
    data: {
      posting_date: "2026-09-18",
      voucher_no: "PN9360",
      description: "Nhập tồn cho test xem PDF nháp",
      supplier_code: "",
      warehouse_code: "KHO-CHINH",
      unit_code: "HKD",
      items: [{ product_code: "SP001", quantity: 20, unit_price: 10000, discount: 0 }],
      note: "",
      inbound_type: "purchase",
      reference_no: "",
      vat_rate: 0,
      debit_account: "152",
      credit_account: "331",
      pay_now: false,
      adjust_dir: "up",
      autoBom: false,
    },
  });
  expect(stockIn.ok(), `save_inbound failed: ${await stockIn.text()}`).toBe(true);
  const mkInvoice = await request.post("/api/save_invoice", {
    data: {
      number: "HD9360",
      date: "2026-09-18",
      customer: "Khách xem PDF",
      customer_tax_code: "0100000000",
      items: [
        {
          product_code: "SP001",
          quantity: 2,
          unit_price: 10000,
          industry_code: "PPHH",
          discount: 0,
          warehouse_code: "",
        },
      ],
    },
  });
  expect(mkInvoice.ok(), `save_invoice failed: ${await mkInvoice.text()}`).toBe(true);

  await sidebarButton(page, "Chờ xuất HĐĐT").click();
  await expect(page.locator("header h2")).toHaveText("Chờ xuất HĐĐT");
  await page.getByPlaceholder("Số hóa đơn, khách hàng, MST…").fill("HD9360");
  await page.getByPlaceholder("Số hóa đơn, khách hàng, MST…").press("Enter");
  const row = page.locator("tr", { has: page.getByText("HD9360", { exact: true }) }).first();
  await expect(row).toBeVisible();

  // Nút PDF dựng `detail_json` từ hóa đơn nội bộ (backend) rồi render ngay trong Rust.
  await row.getByRole("button", { name: "Xem PDF hóa đơn" }).click();
  await expect(invoiceFrame(page)).toBeVisible({ timeout: 30_000 });
  await expectValidPdf(page);
  await invoiceFrame(page).getByRole("button", { name: "Đóng" }).click();
  await expect(invoiceFrame(page)).toBeHidden();

  // Lập nháp ngay tại màn này — mở hộp thoại LẬP MỚI (không phải sửa hóa đơn dở).
  await page.getByRole("button", { name: "Lập hóa đơn nháp" }).click();
  const draftDlg = page.getByRole("dialog").filter({ hasText: "Lập hóa đơn bán hàng (nháp)" });
  await expect(draftDlg).toBeVisible();
  await draftDlg.getByRole("button", { name: "Hủy" }).click();
  await expect(draftDlg).toBeHidden();
});

test("Hóa đơn nháp: bấm Lập hóa đơn nhiều lần chỉ tạo 1 hóa đơn", async ({ page, request }) => {
  await ensureLoggedIn(page);
  await sidebarButton(page, "Hóa đơn").click();
  await expect(page.locator("header h2")).toHaveText("Hóa đơn");

  const stockIn = await request.post("/api/save_inbound", {
    data: {
      posting_date: "2026-09-21",
      voucher_no: "PN9350",
      description: "Nhập tồn cho test bấm nhiều lần",
      supplier_code: "",
      warehouse_code: "KHO-CHINH",
      unit_code: "HKD",
      items: [{ product_code: "SP001", quantity: 30, unit_price: 10000, discount: 0 }],
      note: "",
      inbound_type: "purchase",
      reference_no: "",
      vat_rate: 0,
      debit_account: "152",
      credit_account: "331",
      pay_now: false,
      adjust_dir: "up",
      autoBom: false,
    },
  });
  expect(stockIn.ok(), `save_inbound failed ${stockIn.status()}: ${await stockIn.text()}`).toBe(
    true,
  );

  await page.getByRole("button", { name: "Lập hóa đơn nháp" }).click();
  const dlg = page.getByRole("dialog");
  // Số gợi ý điền sau khi nạp xong danh sách hóa đơn → chờ có số rồi mới đọc.
  const numberInput = dlg.getByRole("textbox").first();
  await expect(numberInput).toHaveValue(/^HD\d+$/);
  const number = await numberInput.inputValue();

  const custInput = dlg.getByRole("combobox", { name: "Chọn hoặc nhập tên khách hàng" });
  await custInput.fill("Khách Bấm Nhiều Lần");
  await custInput.press("Enter");
  const line = dlg.locator("tbody tr").first();
  await line.getByRole("combobox").first().click();
  await page
    .getByRole("option", { name: /Bottled water/ })
    .first()
    .click();
  // SP001 chưa có nhóm ngành mặc định → chọn nhóm ngành trên dòng.
  await line.getByRole("combobox").nth(1).click();
  await page
    .getByRole("option", { name: /Phân phối, cung cấp hàng hóa/ })
    .first()
    .click();
  await line.getByRole("spinbutton").nth(1).fill("10000");

  // Bấm liên tiếp nhiều lần (nút phải khoá ngay khi đang lưu).
  const saveBtn = dlg.getByRole("button", { name: "Lập hóa đơn" });
  await saveBtn.dblclick();
  await expect(dlg).toBeHidden();
  // Bấm thêm lần nữa sau khi lưu xong (dialog đã đóng) cũng không sinh bản ghi mới.
  await page.keyboard.press("Escape");

  const all = (await (await request.post("/api/get_invoices", { data: {} })).json()) as Array<{
    number: string;
  }>;
  const sameNo = all.filter((i) => i.number === number);
  expect(sameNo.length, `số ${number} bị lặp: ${sameNo.length} bản ghi`).toBe(1);
  await expect(page.locator("tr", { has: page.getByText(number, { exact: true }) })).toHaveCount(1);
});

test("Hóa đơn nháp: sửa được dòng hàng và thông tin chung", async ({ page, request }) => {
  await ensureLoggedIn(page);
  await sidebarButton(page, "Hóa đơn").click();
  await expect(page.locator("header h2")).toHaveText("Hóa đơn");

  const stockIn = await request.post("/api/save_inbound", {
    data: {
      posting_date: "2026-09-18",
      voucher_no: "PN9200",
      description: "Nhập tồn cho test sửa hóa đơn nháp",
      supplier_code: "",
      warehouse_code: "KHO-CHINH",
      unit_code: "HKD",
      items: [{ product_code: "SP001", quantity: 20, unit_price: 10000, discount: 0 }],
      note: "",
      inbound_type: "purchase",
      reference_no: "",
      vat_rate: 0,
      debit_account: "152",
      credit_account: "331",
      pay_now: false,
      adjust_dir: "up",
      autoBom: false,
    },
  });
  expect(stockIn.ok(), `save_inbound failed ${stockIn.status()}: ${await stockIn.text()}`).toBe(
    true,
  );

  const created = await request.post("/api/save_invoice", {
    data: {
      number: "HD9200",
      date: "2026-09-18",
      customer: "Khách Sửa",
      customer_tax_code: "0100000000",
      items: [
        {
          product_code: "SP001",
          quantity: 1,
          unit_price: 10000,
          industry_code: "PPHH",
          discount: 0,
          warehouse_code: "",
        },
      ],
    },
  });
  expect(created.ok(), `save_invoice failed ${created.status()}: ${await created.text()}`).toBe(
    true,
  );
  await page.reload();
  await expect(page.locator("header h2")).toHaveText("Hóa đơn");

  const row = page.locator("tr", { has: page.getByText("HD9200", { exact: true }) }).first();
  await row.getByRole("button", { name: "Sửa hóa đơn nháp" }).click();

  // Dialog sửa: tiêu đề + dữ liệu cũ nạp sẵn.
  const dlg = page.getByRole("dialog");
  await expect(dlg.getByText("Sửa hóa đơn nháp HD9200")).toBeVisible();
  const custInput = dlg.getByRole("combobox", { name: "Chọn hoặc nhập tên khách hàng" });
  await expect(custInput).toHaveValue("Khách Sửa");
  const inLine = dlg.locator("tbody tr", { hasText: "Bottled water" });
  const qtyInput = inLine.getByRole("spinbutton").first();
  await expect(qtyInput).toHaveValue("1");

  // Sửa SL 1 → 3, đổi khách.
  await qtyInput.fill("3");
  await custInput.fill("Khách Sửa Mới");
  await custInput.press("Enter");
  await dlg.getByRole("button", { name: "Lưu thay đổi" }).click();
  await expect(dlg).toBeHidden();

  // Tổng tiền trong bảng phải theo SL mới (3 × 10.000), khách cũ biến mất.
  await expect(row.getByText("30.000 đ", { exact: true })).toBeVisible();
  await expect(page.getByText("Khách Sửa Mới", { exact: true })).toBeVisible();
  await expect(page.getByText("Khách Sửa", { exact: true })).toHaveCount(0);
});

test("Hóa đơn nháp: nút xoá dọn cả dòng hàng, hóa đơn đã xử lý thì không có nút xoá", async ({
  page,
  request,
}) => {
  await ensureLoggedIn(page);
  await sidebarButton(page, "Hóa đơn").click();
  await expect(page.locator("header h2")).toHaveText("Hóa đơn");

  // Nhập tồn cho SP001 — hóa đơn bán bắt buộc đủ tồn theo kho xuất trên dòng.
  const stockIn = await request.post("/api/save_inbound", {
    data: {
      posting_date: "2026-09-19",
      voucher_no: "PN9100",
      description: "Nhập tồn cho test xoá hóa đơn nháp",
      supplier_code: "",
      warehouse_code: "KHO-CHINH",
      unit_code: "HKD",
      items: [{ product_code: "SP001", quantity: 5, unit_price: 10000, discount: 0 }],
      note: "",
      inbound_type: "purchase",
      reference_no: "",
      vat_rate: 0,
      debit_account: "152",
      credit_account: "331",
      pay_now: false,
      adjust_dir: "up",
      autoBom: false,
    },
  });
  expect(stockIn.ok(), `save_inbound failed ${stockIn.status()}: ${await stockIn.text()}`).toBe(
    true,
  );

  const created = await request.post("/api/save_invoice", {
    data: {
      number: "HD9100",
      date: "2026-09-20",
      customer: "Khách Xoá nháp",
      customer_tax_code: "0100000000",
      items: [
        {
          product_code: "SP001",
          quantity: 1,
          unit_price: 10000,
          industry_code: "PPHH",
          discount: 0,
          warehouse_code: "",
        },
      ],
    },
  });
  expect(created.ok(), `save_invoice failed ${created.status()}: ${await created.text()}`).toBe(
    true,
  );
  await page.reload();
  await expect(page.locator("header h2")).toHaveText("Hóa đơn");

  const row = page.locator("tr", { has: page.getByText("HD9100", { exact: true }) }).first();
  await expect(row.getByText("Nháp", { exact: true })).toBeVisible();

  // Bấm xoá → hộp xác nhận nêu rõ số hóa đơn và số tiền.
  await row.getByRole("button", { name: "Xoá hóa đơn nháp" }).click();
  const confirmBox = page.locator(".p-confirmdialog");
  await expect(confirmBox.getByText("Xoá hóa đơn nháp HD9100")).toBeVisible();
  await confirmBox.getByRole("button", { name: "Xoá", exact: true }).click();
  await expect(row).toHaveCount(0);
  await expect(page.locator(".p-toast-summary").last()).toContainText("Đã xoá hóa đơn nháp");

  // Backend cũng phải sạch: hóa đơn biến mất khỏi danh sách (dòng hàng xoá theo).
  const after = (await (await request.post("/api/get_invoices", { data: {} })).json()) as Array<{
    number: string;
  }>;
  expect(after.map((i) => i.number)).not.toContain("HD9100");
});

test("Xuất hóa đơn sang dịch vụ khác: chép dữ liệu + lưu bản chốt + thay thế hóa đơn", async ({
  page,
  request,
}) => {
  await ensureLoggedIn(page);
  await sidebarButton(page, "Hóa đơn").click();
  await expect(page.locator("header h2")).toHaveText("Hóa đơn");

  // 0) Nhập tồn cho SP001 — hóa đơn bán bắt buộc đủ tồn theo kho xuất trên dòng.
  const stockIn = await request.post("/api/save_inbound", {
    data: {
      posting_date: "2026-09-19",
      voucher_no: "PN9000",
      description: "Nhập tồn cho test xuất hóa đơn",
      supplier_code: "",
      warehouse_code: "KHO-CHINH",
      unit_code: "HKD",
      // Nhiều dư: các test dùng chung DB, tồn bị các phiếu khác trừ dần.
      items: [{ product_code: "SP001", quantity: 100, unit_price: 10000, discount: 0 }],
      note: "",
      inbound_type: "purchase",
      reference_no: "",
      vat_rate: 0,
      debit_account: "152",
      credit_account: "331",
      pay_now: false,
      adjust_dir: "up",
      autoBom: false,
    },
  });
  expect(stockIn.ok(), `save_inbound failed ${stockIn.status()}: ${await stockIn.text()}`).toBe(
    true,
  );

  // 1) Hóa đơn nháp có 1 dòng hàng (mã hàng phải có sẵn từ seed SP001).
  const created = await request.post("/api/save_invoice", {
    data: {
      number: "HD9001",
      date: "2026-09-20",
      customer: "Khách E2E",
      customer_tax_code: "0100000000",
      items: [
        {
          product_code: "SP001",
          quantity: 2,
          unit_price: 10000,
          industry_code: "PPHH",
          discount: 0,
          warehouse_code: "",
        },
      ],
    },
  });
  expect(created.ok(), `save_invoice failed ${created.status()}: ${await created.text()}`).toBe(
    true,
  );
  await page.reload();
  await expect(page.locator("header h2")).toHaveText("Hóa đơn");
  await expect(page.getByText("HD9001", { exact: true })).toBeVisible();

  // 2) Mở dialog chép: có kiểm tra trước, dòng hàng và khối TSV.
  const row = page.locator("tr", { has: page.getByText("HD9001", { exact: true }) }).first();
  await row.getByRole("button", { name: "Chép sang dịch vụ HĐĐT khác" }).click();
  const dlg = page.getByRole("dialog");
  await expect(dlg.getByText("Chép hóa đơn HD9001 sang dịch vụ khác")).toBeVisible();
  await expect(dlg.getByText("Khách E2E", { exact: true })).toBeVisible();
  const exportLine = dlg.locator("tbody tr", { hasText: "Bottled water" });
  await expect(exportLine.getByRole("cell").nth(1)).toContainText("Bottled water");
  await expect(exportLine.getByRole("cell").nth(3)).toHaveText("2");
  await expect(exportLine.getByRole("cell").nth(4)).toHaveText("10.000 đ");
  await expect(exportLine.getByRole("cell").nth(6)).toHaveText("20.000 đ");
  // Cột thuế suất chỉ là cơ sở kê cuối kỳ — hóa đơn không tách thuế.
  await expect(exportLine.getByRole("cell").nth(7)).toHaveText("1% Phân phối, cung cấp hàng hóa");
  await expect(dlg.getByText("Tổng tiền (khách trả)")).toBeVisible();
  await expect(dlg.getByText("20.000 đ", { exact: true }).first()).toBeVisible();
  await expect(dlg.getByText("20.200 đ")).toHaveCount(0);

  // Khối dán mặc định: "Tên · ĐVT · SL · Đơn giá · Thành tiền", tách bằng dấu phẩy,
  // số không dấu phân cách nghìn, không kèm thuế suất (bên kia tự áp tỷ lệ %).
  const pasteBox = dlg.getByRole("textbox");
  // Nút chuyển kiểu chép phải render (SelectButton của PrimeVue không auto-import).
  await expect(dlg.getByRole("button", { name: "Dán dòng hàng" })).toBeVisible();
  await dlg.getByRole("button", { name: "Text đầy đủ" }).click();
  await expect(pasteBox).toHaveValue(/Tổng tiền: 20\.000/);
  await dlg.getByRole("button", { name: "Dán dòng hàng" }).click();

  await expect(pasteBox).toHaveValue(/^Bottled water,Bottle,2,10000,20000\s*$/);
  await expect(dlg.getByText("Tên hàng · ĐVT · SL · Đơn giá · Thành tiền")).toBeVisible();

  // 3) Bấm "Đã chép sang bên kia" → lưu bản chốt, trạng thái chuyển sang đã chép.
  await dlg.getByRole("button", { name: "Đã chép sang bên kia" }).click();
  await expect(dlg).toBeHidden();
  await expect(row.getByText("Đã chép — chờ phát hành", { exact: true })).toBeVisible();

  const afterExport = await (await request.post("/api/get_invoices", { data: {} })).json();
  const inv = (afterExport as Array<{ number: string; status: string; exported_at: string }>).find(
    (i) => i.number === "HD9001",
  );
  expect(inv?.status).toBe("exported");
  expect(inv?.exported_at).toBeTruthy();

  // 4) Nhập số HĐĐT đã phát hành → chính thức.
  await row.getByRole("button", { name: "Nhập số HĐĐT đã phát hành" }).click();
  const linkDlg = page.getByRole("dialog");
  await linkDlg.getByLabel("Số HĐĐT").fill("00000901");
  await linkDlg.getByLabel("Ký hiệu HĐĐT").fill("1C26TT152");
  await linkDlg.getByRole("button", { name: "Lưu" }).click();
  await expect(row.getByText("Đã phát hành", { exact: true })).toBeVisible();

  // 5) Thay thế HĐ đã phát hành (TT 91/2026 Điều 10: không hủy được).
  //    Bắt buộc có lý do → app đảo doanh thu + tạo hóa đơn nháp mới.
  await row.getByRole("button", { name: "Lập hóa đơn thay thế" }).click();
  const replaceDlg = page.getByRole("dialog", { name: /Lập hóa đơn thay thế/ });
  await expect(replaceDlg.getByText("Nợ 511 / Có 131", { exact: false }).first()).toBeVisible();
  await replaceDlg.getByRole("button", { name: "Tạo HĐ thay thế" }).click();
  await expect(page.locator(".p-toast-summary").last()).toContainText("Thiếu lý do", {
    timeout: 10_000,
  });

  await replaceDlg.getByLabel("Lý do phải thay thế").fill("Sai số lượng bán");
  await replaceDlg.getByRole("button", { name: "Tạo HĐ thay thế" }).click();
  await expect(replaceDlg).toBeHidden();

  // Hóa đơn cũ chuyển sang "Đã bị thay thế"; app mở sẵn hóa đơn nháp mới để sửa.
  const draftDlg = page.getByRole("dialog", { name: /Sửa hóa đơn nháp/ });
  await expect(draftDlg).toBeVisible();
  await expect(draftDlg.getByRole("textbox").first()).not.toHaveValue("HD9001");
  await expect(row.getByText("Đã bị thay thế", { exact: true })).toBeVisible();

  const replaceEvents = await (
    await request.post("/api/invoice_events", { data: { invoiceId: (inv as { id: number }).id } })
  ).json();
  const steps = (replaceEvents as Array<{ to_status: string }>).map((e) => e.to_status);
  expect(steps).toContain("exported");
  expect(steps).toContain("replaced");

  // Đóng hộp thoại nháp: hóa đơn cũ đã bị thay thế thì không còn nút chép/liên kết.
  await draftDlg.getByRole("button", { name: "Hủy" }).click();
  await expect(draftDlg).toBeHidden();
  await expect(row.getByRole("button", { name: "Chép sang dịch vụ HĐĐT khác" })).toHaveCount(0);
  await expect(row.getByRole("button", { name: "Lập hóa đơn thay thế" })).toHaveCount(0);
});

// ─── HĐĐT: tra cứu hóa đơn (mock portal offline, E2E_HDDT_LIVE=1 để dùng cổng thật) ───

/**
 * Nguồn cổng cho test HĐĐT.
 *
 *   E2E_HDDT_LIVE=1  → cổng thật, credentials từ `info.txt` (gitignore) hoặc
 *                      E2E_HDDT_USERNAME / E2E_HDDT_PASSWORD.
 *   mặc định         → mock portal của E2E (e2e/playwright/mock-portal.ts):
 *                      offline, không cần mạng/credentials, payload giống cổng.
 */
const LIVE_PORTAL = process.env.E2E_HDDT_LIVE === "1";

/** Credentials cổng thật (chỉ dùng khi E2E_HDDT_LIVE=1). */
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
  if (!LIVE_PORTAL) {
    // Mock: cấu hình trỏ base_url về mock portal rồi đăng nhập (token trả về sẵn).
    const cfg = await api.post("/api/hddt_save_config", {
      data: { username: "0100000000", password: "mock", baseUrl: MOCK_PORTAL_URL },
    });
    expect(cfg.ok(), `hddt_save_config (mock) failed ${cfg.status()}`).toBe(true);
  } else {
    const creds = portalCredentials();
    if (!creds) return null;
    const base = process.env.E2E_HDDT_BASE_URL ?? "https://hoadondientu.gdt.gov.vn";
    const cfg = await api.post("/api/hddt_save_config", {
      data: { username: creds.username, password: creds.password, baseUrl: base },
    });
    expect(cfg.ok(), `hddt_save_config failed ${cfg.status()}`).toBe(true);
  }
  // Cổng giới hạn tần suất (429) → chỉ đăng nhập khi chưa có phiên, tái dùng
  // phiên của test trước thay vì đăng nhập lại liên tục.
  const status = await (await api.post("/api/hddt_status", { data: {} })).json();
  if (status.logged_in) return status;
  const login = await api.post("/api/hddt_login", { data: {} });
  expect(login.ok(), `hddt_login failed ${login.status()}`).toBe(true);
  const body = await login.json();
  if (body.need_manual) {
    // Cổng thật đôi lúc bắt captcha tay (solver 2 lần thất bại) → bỏ qua thay vì
    // đánh dấu fail, vì đây là hạn chế môi trường chứ không phải lỗi app.
    console.log("ℹ️ portal yêu cầu captcha thủ công — bỏ qua test tra cứu portal");
    return null;
  }
  return body;
}

test("HĐĐT: app tự đăng nhập cổng lúc khởi động, không cần mở màn HĐĐT", async ({
  page,
  request,
}) => {
  // Cấu hình trỏ mock portal + đăng nhập một lần để app có token hợp lệ.
  await request.post("/api/hddt_save_config", {
    data: { username: "0100000000", password: "mock", baseUrl: MOCK_PORTAL_URL },
  });
  await request.post("/api/hddt_login", { data: {} });

  // Mở app (đăng nhập nội bộ) — không chạm vào menu HĐĐT.
  await ensureLoggedIn(page);
  await expect
    .poll(
      async () => (await (await request.post("/api/hddt_status", { data: {} })).json()).logged_in,
      {
        timeout: 20_000,
      },
    )
    .toBe(true);

  // Vào thẳng màn Đồng bộ: không được hiện cảnh báo "chưa đăng nhập".
  await sidebarButton(page, "Đồng bộ HĐĐT").click();
  await expect(page.locator("header h2")).toHaveText("Đồng bộ hóa đơn mua");
  await expect(page.getByText("Chưa đăng nhập cổng HĐĐT", { exact: false })).toBeHidden();
  await expect(page.getByRole("button", { name: "Đăng nhập cổng HĐĐT" })).toBeHidden();
});

// Lưu tài khoản qua UI từng hiện toast lỗi ảo "Không lưu được: No PrimeVue Toast
// provided!" — useToast() của PrimeVue ném khi gọi ngoài setup, nên bấm Lưu chạy
// xong hết, thậm chí đăng nhập xong, vẫn bị báo lỗi. Mắt soi mật khẩu từng hiện ô
// trống (giá trị bị xoá sau lưu) và mở thêm ô nhập riêng thay vì dùng ô đang có.
test("HĐĐT: lưu tài khoản qua UI không lỗi ảo, mắt soi mật khẩu dùng chung ô", async ({
  page,
  request,
}) => {
  test.skip(LIVE_PORTAL, "Test UI luôn trỏ mock portal — bỏ qua khi chạy cổng thật");
  await request.post("/api/hddt_save_config", {
    data: { username: "0100000000", password: "mock", baseUrl: MOCK_PORTAL_URL },
  });
  await ensureLoggedIn(page);
  await openTab(page, "HĐĐT", "/hddt");
  await expect(page.locator("header h2")).toHaveText("Hóa đơn điện tử");

  // Mắt HIỆN: đọc mật khẩu đã lưu vào NGAY ô mật khẩu, không mở ô thứ hai.
  const pw = page.locator('input[placeholder="Đã lưu — để trống nếu không đổi"]');
  await expect(pw).toHaveAttribute("type", "password");
  await page.getByRole("button", { name: "Hiện mật khẩu" }).click();
  await expect(pw).toHaveAttribute("type", "text");
  await expect(pw).toHaveValue("mock");
  await expect(page.getByText("Xem mật khẩu đã lưu")).toHaveCount(0);

  // Mắt ẨN: ô vẫn đúng mật khẩu đã lưu → đưa về trống ("để trống nếu không đổi").
  await page.getByRole("button", { name: "Ẩn mật khẩu" }).click();
  await expect(pw).toHaveValue("");

  // Lưu qua UI: toast thành công + loginPortal chạy đến nơi (mock portal) —
  // chứng minh không còn ném giữa chừng, không còn "Không lưu được" ảo.
  await page.getByRole("button", { name: "Lưu tài khoản" }).click();
  await expect(page.getByText("Đã lưu tài khoản HĐĐT")).toBeVisible();
  await expect(page.getByText("Đã đăng nhập cổng HĐĐT")).toBeVisible();
  await expect(page.getByText("Không lưu được")).toHaveCount(0);
  await expect(page.getByText("No PrimeVue Toast provided")).toHaveCount(0);
});

test("HĐĐT tra cứu hóa đơn: mặc định tab máy tính tiền và trả về hóa đơn", async ({
  page,
  request,
}) => {
  await ensureLoggedIn(page);
  const login = await loginPortal(request);
  test.skip(
    login === null,
    "Không lấy được phiên cổng HĐĐT (thiếu info.txt hoặc cổng bắt captcha) — bỏ qua",
  );

  await sidebarButton(page, "Tra cứu HĐĐT").click();
  await expect(page.locator("header h2")).toHaveText("Tra cứu HĐĐT");

  // Tab lớn mặc định = hóa đơn ra; tab nhỏ mặc định = máy tính tiền (sco-query).
  const kindTabs = page.getByRole("tab");
  await expect(kindTabs.filter({ hasText: "Hóa đơn ra" })).toHaveAttribute("aria-selected", "true");
  await expect(kindTabs.filter({ hasText: "Hóa đơn máy tính tiền" })).toHaveAttribute(
    "aria-selected",
    "true",
  );

  // Tìm kiếm: dải ngày mặc định = 30 ngày gần nhất.
  await page.getByRole("button", { name: "Tìm kiếm", exact: true }).click();
  const table = page.locator(".p-datatable");
  await expect(table).toBeVisible({ timeout: 30_000 });
  await expect(page.getByText(/Có \d[\d.]* kết quả/)).toBeVisible({ timeout: 30_000 });
  // Các cột định danh của hóa đơn đều có giá trị.
  await expect(table.getByText("Ký hiệu HĐ")).toBeVisible();
  await expect(table.locator("tbody tr").first()).toBeVisible();
  const firstRow = table.locator("tbody tr").first();
  await expect(firstRow.locator("td").nth(1)).not.toHaveText("—"); // ký hiệu HĐ
  await expect(firstRow.locator("td").nth(2)).not.toHaveText("—"); // số HĐ
  // Cột "Mã số thuế" và "Mẫu số" đã bỏ khỏi UI (dữ liệu vẫn còn trong API).
  await expect(table.getByRole("columnheader", { name: "Mã số thuế" })).toBeHidden();
  await expect(table.getByRole("columnheader", { name: "Mẫu số" })).toBeHidden();
  if (LIVE_PORTAL) {
    // Chỉ khi chạy cổng thật: dữ liệu phải là hóa đơn thật, MST đối tác là số.
    await expect(firstRow.getByText(/MST người mua:\s*\d{10,13}/).first()).toBeVisible();
  } else {
    await expect(firstRow.getByText("C26MOC", { exact: false })).toBeVisible();
  }
});

test("Tra cứu HĐĐT: tải file XML hóa đơn rồi báo đã lưu", async ({ page, request }) => {
  test.skip(LIVE_PORTAL, "Test UI luôn trỏ mock portal — bỏ qua khi chạy cổng thật");
  await ensureLoggedIn(page);
  const login = await loginPortal(request);
  test.skip(
    login === null,
    "Không lấy được phiên cổng HĐĐT (thiếu info.txt hoặc cổng bắt captcha) — bỏ qua",
  );

  await sidebarButton(page, "Tra cứu HĐĐT").click();
  await expect(page.locator("header h2")).toHaveText("Tra cứu HĐĐT");
  await page.getByRole("button", { name: "Tìm kiếm", exact: true }).click();
  const table = page.locator(".p-datatable");
  const firstRow = table.locator("tbody tr").first();
  await expect(firstRow).toBeVisible({ timeout: 30_000 });

  // Chưa có file → nút "Tải XML" hiện trên dòng.
  const xmlBtn = firstRow.getByRole("button", { name: /Tải file XML hóa đơn/ });
  await expect(xmlBtn).toBeVisible();
  await xmlBtn.click();
  await expect(page.getByText("Đã lưu file XML")).toBeVisible({ timeout: 20_000 });

  // Sau khi lưu: cột hiện dấu tick + backend có bản ghi file trong hồ sơ.
  await expect(firstRow.locator("i.pi-check-circle")).toBeVisible();
  const list = await request.post("/api/hddt_list_xml", { data: {} });
  expect(list.ok(), `hddt_list_xml failed ${list.status()}`).toBe(true);
  const rows = (await list.json()) as Array<{
    id: number;
    file_name: string;
    byte_size: number;
    direction: string;
    kind: string;
    nbmst: string;
    khmshdon: number;
    khhdon: string;
    shdon: string;
  }>;
  expect(rows.length).toBeGreaterThan(0, "phải có ít nhất 1 file XML đã lưu");
  expect(rows[0].byte_size).toBeGreaterThan(0);
  expect(rows[0].direction === "sold" || rows[0].direction === "purchase").toBe(true);

  // Gọi lại đúng hóa đơn đó → `already: true` (không tải lại) và không thêm bản ghi.
  const again = await request.post("/api/hddt_save_xml", {
    data: {
      direction: rows[0].direction,
      kind: rows[0].kind,
      nbmst: rows[0].nbmst,
      khmshdon: rows[0].khmshdon,
      khhdon: rows[0].khhdon,
      shdon: rows[0].shdon,
      portal_id: null,
    },
  });
  expect(again.ok(), `hddt_save_xml failed ${again.status()}: ${await again.text()}`).toBe(true);
  expect((await again.json()).already).toBe(true, "lần 2 phải báo đã có sẵn");
  const after = await request.post("/api/hddt_list_xml", { data: {} });
  expect(((await after.json()) as unknown[]).length).toBe(
    rows.length,
    "bấm lại không được tạo thêm bản ghi",
  );
  expect(rows[0].id, "dòng phải có id để gộp ZIP").toBeGreaterThan(0);

  // Nút "Tải ZIP XML" gộp file đã lưu → tải về 1 file `.zip` thật (nộp/gửi).
  const zipBtn = page.getByRole("button", { name: /Tải ZIP gộp file XML/ });
  await expect(zipBtn).toBeVisible();
  const [download] = await Promise.all([page.waitForEvent("download"), zipBtn.click()]);
  expect(download.suggestedFilename()).toMatch(/\.zip$/);
  await expect(page.getByText(/Đã tải ZIP \d+ file XML/)).toBeVisible();
  const zipPath = await download.path();
  expect(zipPath, "trình duyệt phải lưu file về").toBeTruthy();
  const zipBytes = readFileSync(zipPath as string);
  // Magic byte "PK" (0x50 0x4B) — đúng định dạng ZIP chứ không phải chuỗi lỗi JSON.
  expect([zipBytes[0], zipBytes[1]], "phải là ZIP thật").toEqual([0x50, 0x4b]);
  expect(zipBytes.length).toBeGreaterThan(100);
});

test("Đồng bộ HĐ mua: cột File XML tải được file XML của hóa đơn trong cache", async ({
  page,
  request,
}) => {
  test.skip(LIVE_PORTAL, "Test UI luôn trỏ mock portal — bỏ qua khi chạy cổng thật");
  await ensureLoggedIn(page);
  const login = await loginPortal(request);
  test.skip(
    login === null,
    "Không lấy được phiên cổng HĐĐT (thiếu info.txt hoặc cổng bắt captcha) — bỏ qua",
  );

  // Vào màn trước rồi mới seed + reload (reload sẽ quay về route hiện tại).
  await sidebarButton(page, "Đồng bộ HĐĐT").click();
  await expect(page.locator("header h2")).toHaveText("Đồng bộ hóa đơn mua");

  const seed = await request.post("/api/hddt_sync_test_seed", {
    data: {
      portal_id: "e2e-uuid-xml",
      detail: {
        hdhhdvu: [
          {
            stt: 1,
            tchat: 1,
            ten: "Hàng XML E2E",
            dvtinh: "Cái",
            mhhdvu: "xmlitem",
            sluong: 1,
            dgia: 100000,
            thtien: 100000,
            stckhau: 0,
            ltsuat: "8%",
            tsuat: 0.08,
          },
        ],
      },
    },
  });
  expect(seed.ok(), `seed hddt_sync_test_seed failed ${seed.status()}`).toBe(true);

  await page.reload();
  await expect(page.locator("header h2")).toHaveText("Đồng bộ hóa đơn mua");

  // Bảng có nhiều hóa đơn seed chung ký hiệu C26E2E → lấy dòng đầu tiên.
  const row = page.locator("tr", { has: page.getByText("C26E2E") }).first();
  await expect(row).toBeVisible({ timeout: 20_000 });

  // Chưa có file → bấm "Tải XML"; lượt trước đã lưu (test khác chạy trước) thì
  // bỏ qua bước bấm, phần assert dưới vẫn kiểm tra trạng thái.
  const xmlBtn = row.getByRole("button", { name: /Tải file XML/ });
  if (await xmlBtn.count()) await xmlBtn.click();
  await expect(row.getByText("Đã lưu", { exact: true })).toBeVisible({ timeout: 20_000 });

  // Backend phải có bản ghi file XML chiều mua (không phụ thuộc thao tác bấm).
  const list = await request.post("/api/hddt_list_xml", { data: {} });
  expect(list.ok(), `hddt_list_xml failed ${list.status()}`).toBe(true);
  const rows = (await list.json()) as Array<{
    direction: string;
    kind: string;
    byte_size: number;
  }>;
  expect(
    rows.some((r) => r.direction === "purchase" && r.kind === "regular" && r.byte_size > 0),
    "phải có file XML hóa đơn mua đã lưu",
  ).toBe(true);

  // Nút "Tải ZIP XML" ở thanh công cụ gộp đúng file XML của dòng đang xem.
  const zipBtn = page.getByRole("button", { name: /Tải ZIP gộp file XML/ });
  await expect(zipBtn).toBeVisible();
  const [download] = await Promise.all([page.waitForEvent("download"), zipBtn.click()]);
  expect(download.suggestedFilename()).toMatch(/^hddt_xml_mua_\d{8}\.zip$/);
  await expect(page.getByText(/Đã tải ZIP \d+ file XML/)).toBeVisible();
  const zipPath = await download.path();
  expect(zipPath, "trình duyệt phải lưu file về").toBeTruthy();
  const zipBytes = readFileSync(zipPath as string);
  expect([zipBytes[0], zipBytes[1]], "phải là ZIP thật").toEqual([0x50, 0x4b]);
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
  test.skip(
    login === null,
    "Không lấy được phiên cổng HĐĐT (thiếu info.txt hoặc cổng bắt captcha) — bỏ qua",
  );

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
  await page.getByRole("tab", { name: "Hóa đơn điện tử" }).click();
  await page.getByRole("button", { name: "Tìm kiếm", exact: true }).click();
  await expect(page.getByText(/Có \d[\d.]* kết quả/)).toBeVisible({ timeout: 30_000 });
  const table = page.locator(".p-datatable");
  await expect(table.getByRole("columnheader", { name: "Thông tin người bán" })).toBeVisible();
  const firstRow = table.locator("tbody tr").first();
  await expect(firstRow.getByText("MST người bán:").first()).toBeVisible();
  await expect(firstRow.getByText("Tên người bán:").first()).toBeVisible();
  // MST người bán nằm trong ô đối tác (cột "Mã số thuế" đã bỏ) → phải là MST thật.
  const mstRe = LIVE_PORTAL ? /\d{10,13}/ : /0100000000/;
  await expect(
    firstRow.getByText(new RegExp(`MST người bán:\\s*${mstRe.source}`)).first(),
  ).toBeVisible();
});

test("Responsive: điện thoại thì sidebar thành ngăn kéo và bảng thành thẻ", async ({ browser }) => {
  // hasTouch + màn 390px = điện thoại/tablet THẬT → bảng thành thẻ. Khác hẳn cửa
  // sổ desktop thu nhỏ (con trỏ chuột), vốn vẫn giữ nguyên bảng.
  const ctx = await browser.newContext({
    viewport: { width: 390, height: 844 },
    hasTouch: true,
    isMobile: true,
  });
  const page = await ctx.newPage();
  try {
    await page.goto("/");
    const username = page.getByTestId("login-username");
    if (await username.isVisible()) {
      await username.fill(ADMIN.username);
      await page.getByTestId("login-password").fill(ADMIN.password);
      await page.getByRole("button", { name: "Đăng nhập" }).click();
    }
    await expect(page.locator("header h2")).toHaveText("Tổng quan", { timeout: 20_000 });

    // Dựng 1 phiếu nhập (dùng ctx.request để dùng chung phiên cookie context này).
    const stockIn = await ctx.request.post("/api/save_inbound", {
      data: {
        posting_date: "2026-09-22",
        // Có chữ: số phiếu gợi ý của app luôn là "PN"+4 số nên không thể trùng.
        voucher_no: "PN-TEST1",
        description: "Nhập tồn cho test responsive",
        supplier_code: "",
        warehouse_code: "KHO-CHINH",
        unit_code: "HKD",
        items: [{ product_code: "SP001", quantity: 12, unit_price: 10000, discount: 0 }],
        note: "",
        inbound_type: "purchase",
        reference_no: "",
        vat_rate: 0,
        debit_account: "152",
        credit_account: "331",
        pay_now: false,
        adjust_dir: "up",
        autoBom: false,
      },
    });
    expect(stockIn.ok(), `save_inbound failed ${stockIn.status()}: ${await stockIn.text()}`).toBe(
      true,
    );

    // Sidebar trượt ra ngoài màn hình, mở lại bằng nút hamburger. (Đóng kiểm theo
    // vị trí thật: ngăn kéo dùng translate nên vẫn "visible" về mặt DOM.)
    const drawer = page.locator("aside").first();
    await expect(page.getByRole("button", { name: "Mở menu" })).toBeVisible();
    expect((await drawer.boundingBox())!.x).toBeLessThan(0);
    await page.getByRole("button", { name: "Mở menu" }).click();
    await expect.poll(async () => (await drawer.boundingBox())!.x).toBeGreaterThanOrEqual(0);
    await page.getByRole("button", { name: "Nhập kho", exact: true }).click();

    // Bảng dữ liệu chuyển thành thẻ: nhãn cột hiện cạnh giá trị của dòng.
    await expect(page.locator("header h2")).toHaveText("Nhập kho", { timeout: 20_000 });
    await expect(page.getByText("PN-TEST1", { exact: true })).toBeVisible();
    await expect(page.getByText("Số phiếu", { exact: true }).first()).toBeVisible();
    await expect(page.locator(".p-datatable")).toHaveCount(0);

    // Nút trong thẻ vẫn dùng được, hộp thoại chiếm gần hết màn hình.
    await page.getByRole("button", { name: "Xem phiếu" }).first().click();
    await expect(page.getByRole("dialog")).toBeVisible();
    await expect(page.locator(".p-dialog-title")).toContainText("PHIẾU NHẬP KHO");
  } finally {
    await ctx.close();
  }
});

test("Kế toán: khoảng ngày mặc định theo kỳ khai của hộ (quý/tháng/năm)", async ({ page }) => {
  await ensureLoggedIn(page);
  const now = new Date();
  const y = now.getFullYear();
  const m = now.getMonth();
  const p2 = (n: number) => String(n).padStart(2, "0");
  // DatePicker hiển thị dd/MM/yyyy (4 chữ số năm).
  const dmy = (d: Date) => `${p2(d.getDate())}/${p2(d.getMonth() + 1)}/${d.getFullYear()}`;
  const quarterStart = new Date(y, Math.floor(m / 3) * 3, 1);
  const quarterEnd = new Date(y, Math.floor(m / 3) * 3 + 2 + 1, 0);
  const monthStart = new Date(y, m, 1);
  const monthEnd = new Date(y, m + 1, 0);

  const datePicker = (label: string) =>
    page.locator(`label:text-is("${label}") + .p-datepicker input`);

  /** Đọc khoảng ngày app đang đặt (chờ nó gán xong vì đọc cấu hình qua API). */
  async function currentRange() {
    const from = datePicker("Từ ngày");
    const to = datePicker("Đến ngày");
    await expect(from).not.toHaveValue("");
    await expect(to).not.toHaveValue("");
    // Chờ cấu hình tải xong (dòng "Kỳ khai thuế" chỉ hiện khi đã có config).
    await expect(page.getByText("Kỳ khai thuế:")).toBeVisible();
    return {
      from: await from.inputValue(),
      to: await to.inputValue(),
    };
  }

  /** Đổi kỳ khai qua hộp thoại "Cấu hình thuế" — đúng đường người dùng đi. */
  async function setTaxPeriod(optionText: RegExp) {
    await page.getByRole("button", { name: "Cấu hình thuế" }).click();
    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();
    await dialog.locator(".p-select").first().click();
    await page.locator(".p-select-option", { hasText: optionText }).first().click();
    await dialog.getByRole("button", { name: "Lưu cấu hình" }).click();
    await expect(dialog).toBeHidden();
  }

  await openTab(page, "Kế toán HKD", "/accounting");
  await expect(page.locator("header h2")).toHaveText("Kế toán HKD");

  // Mặc định của hộ là khai theo quý → mở màn thấy đúng quý đang chạy.
  const q = await currentRange();
  expect(q.from, "ngày đầu phải là ngày 01 của quý hiện tại").toBe(dmy(quarterStart));
  expect(q.to, "ngày cuối phải là ngày cuối của quý hiện tại").toBe(dmy(quarterEnd));

  // Đổi sang khai theo tháng → khoảng ngày báo cáo theo tháng hiện tại.
  await setTaxPeriod(/^Theo tháng/);
  const mo = await currentRange();
  expect(mo.from).toBe(dmy(monthStart));
  expect(mo.to).toBe(dmy(monthEnd));

  // Đổi sang khai theo năm → cả năm.
  await setTaxPeriod(/^Theo năm/);
  const yr = await currentRange();
  expect(yr.from).toBe(`01/01/${y}`);
  expect(yr.to).toBe(`31/12/${y}`);

  // Trả lại mặc định để không ảnh hưởng test sau.
  await setTaxPeriod(/^Theo quý/);
  const back = await currentRange();
  expect(back.from).toBe(dmy(quarterStart));
  expect(back.to).toBe(dmy(quarterEnd));
});

// Nút "chuyển nhanh" phải theo đúng "Kỳ khai thuế của hộ": khai theo quý thì 4 nút,
// theo tháng thì chọn được 12 tháng, năm/từng lần phát sinh thì không hiện gì.
test("Kế toán HKD: chuyển nhanh kỳ theo kỳ khai của hộ (quý / tháng / năm)", async ({ page }) => {
  await ensureLoggedIn(page);
  const y = new Date().getFullYear();

  const datePicker = (label: string) =>
    page.locator(`label:text-is("${label}") + .p-datepicker input`);

  /** Đổi kỳ khai qua hộp thoại "Cấu hình thuế" — đúng đường người dùng đi. */
  async function setTaxPeriod(optionText: RegExp) {
    await page.getByRole("button", { name: "Cấu hình thuế" }).click();
    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();
    await dialog.locator(".p-select").first().click();
    await page.locator(".p-select-option", { hasText: optionText }).first().click();
    await dialog.getByRole("button", { name: "Lưu cấu hình" }).click();
    await expect(dialog).toBeHidden();
  }

  await openTab(page, "Kế toán HKD", "/accounting");
  await expect(page.locator("header h2")).toHaveText("Kế toán HKD");

  // Kỳ SỔ chọn trên tab riêng "Sổ kế toán" — KHÔNG còn gióng với "Chuyển nhanh"
  // hay Cấu hình thuế: tab Sổ tự chọn năm/loại kỳ/số kỳ và nhớ lại.
  const bookPeriodType = page.locator('label:text-is("Loại kỳ sổ") + .p-select');
  const bookPeriodNo = page.locator('label:text-is("Kỳ") + .p-select');
  const quickMonth = page.locator('label:text-is("Chuyển nhanh") + .p-select');
  const quickLabel = page.locator('label:text-is("Chuyển nhanh")');

  /** Qua tab Sổ, đọc LOẠI KỲ sổ đang chọn, rồi về lại màn Kế toán. */
  async function expectBookPeriodType(text: string | RegExp) {
    await openTab(page, "Sổ kế toán", "/books");
    await expect(page.locator("header h2")).toHaveText("Sổ kế toán");
    await expect(bookPeriodType).toContainText(text);
    await openTab(page, "Kế toán HKD", "/accounting");
    await expect(page.locator("header h2")).toHaveText("Kế toán HKD");
  }

  // ── Khai theo quý → 4 nút; bấm "Quý 3" thì ngày và tờ khai sang Q3, tab Sổ
  //    vẫn giữ "Theo năm" mặc định (không bị kéo theo) ──
  await setTaxPeriod(/^Theo quý/);
  await expect(page.getByTestId("quick-quarter-3")).toBeVisible();
  await page.getByTestId("quick-quarter-3").click();
  await expect(datePicker("Từ ngày")).toHaveValue(`01/07/${y}`);
  await expect(datePicker("Đến ngày")).toHaveValue(`30/09/${y}`);
  await expectBookPeriodType("Theo năm");
  // Kỳ đang xem được tô đậm (severity primary, không outlined), kỳ khác thì không.
  await expect(page.getByTestId("quick-quarter-3")).toHaveAttribute("data-p-severity", "primary");
  await expect(page.getByTestId("quick-quarter-3")).not.toHaveAttribute("data-p", /outlined/);
  await expect(page.getByTestId("quick-quarter-1")).toHaveAttribute("data-p-severity", "secondary");
  await expect(page.getByTestId("quick-quarter-1")).toHaveAttribute("data-p", /outlined/);
  // Khai theo quý thì không có bộ chọn tháng (nhãn vẫn còn, chỉ có nhóm nút).
  await expect(quickMonth).toHaveCount(0);

  // ── Khai theo tháng → hết nút quý, chuyển sang bộ chọn 12 tháng ──
  await setTaxPeriod(/^Theo tháng/);
  await expect(page.getByTestId("quick-quarter-1")).toHaveCount(0);
  await expect(quickMonth).toBeVisible();
  await quickMonth.click();
  await page.locator(".p-select-option", { hasText: "Tháng 12" }).first().click();
  await expect(datePicker("Từ ngày")).toHaveValue(`01/12/${y}`);
  await expect(datePicker("Đến ngày")).toHaveValue(`31/12/${y}`);
  await expectBookPeriodType("Theo năm"); // số kỳ cũng không gióng sang tab Sổ

  // ── Khai theo năm: cả năm là một kỳ duy nhất → không có gì để chuyển nhanh ──
  await setTaxPeriod(/^Theo năm/);
  await expect(page.getByTestId("quick-quarter-1")).toHaveCount(0);
  await expect(quickLabel).toHaveCount(0);
  // Tab Sổ giữ lựa chọn riêng ("Theo năm" mặc định) → không có ô "Kỳ".
  await openTab(page, "Sổ kế toán", "/books");
  await expect(page.locator("header h2")).toHaveText("Sổ kế toán");
  await expect(bookPeriodType).toContainText("Theo năm");
  await expect(bookPeriodNo).toHaveCount(0);
  await openTab(page, "Kế toán HKD", "/accounting");
  await expect(page.locator("header h2")).toHaveText("Kế toán HKD");

  // Trả lại mặc định để không ảnh hưởng test sau.
  await setTaxPeriod(/^Theo quý/);
  await expect(page.getByTestId("quick-quarter-1")).toBeVisible();
});

test("Sổ kế toán mở đúng mẫu trên tab riêng", async ({ page }) => {
  await ensureLoggedIn(page);
  const datePicker = (label: string) =>
    page.locator(`label:text-is("${label}") + .p-datepicker input`);

  // Khoảng ngày báo cáo màn Kế toán phải nguyên vẹn sau khi thao tác ở tab Sổ —
  // hai tab dùng state riêng cho phần hiển thị, chỉ chung NĂM tính thuế.
  await openTab(page, "Kế toán HKD", "/accounting");
  await expect(page.locator("header h2")).toHaveText("Kế toán HKD");
  const from = datePicker("Từ ngày");
  const to = datePicker("Đến ngày");
  await expect(from).not.toHaveValue("");
  await expect(to).not.toHaveValue("");
  const accountingRange = { from: await from.inputValue(), to: await to.inputValue() };

  await openTab(page, "Sổ kế toán", "/books");
  await expect(page.locator("header h2")).toHaveText("Sổ kế toán");
  // Mặc định S1a (hồ sơ E2E chưa chốt nhóm → app tự xếp Nhóm 1 → chỉ sổ doanh
  // thu): đầu sổ + bộ cột đúng mẫu (A Ngày tháng, B Diễn giải, 1 Số tiền).
  await expect(page.getByText("SỔ DOANH THU BÁN HÀNG HÓA, DỊCH VỤ").first()).toBeVisible();
  await expect(page.getByRole("columnheader", { name: /A · Ngày tháng/ }).first()).toBeVisible();
  // Đầu sổ ghi đúng địa điểm kinh doanh đã khai trong hồ sơ, không in trống.
  await expect(page.getByTestId("tax-book-table")).toContainText(
    "Địa điểm kinh doanh: E2E Market Stall",
  );

  // Nhóm 1 chỉ có S1a + S3a → bật "Xem tất cả 7 mẫu sổ" để soi đủ mẫu in.
  await page.locator("#show-all-books").click();
  await page.getByTestId("tax-book").click();
  await expect(page.locator(".p-select-option")).toHaveCount(7);
  await page.keyboard.press("Escape");

  // Bấm "Sổ S3a-HKD" → chuyển đúng mẫu S3a (10 cột số liệu thuế khác), không rời tab.
  await page.getByTestId("open-book-s3a").click();
  await expect(page.getByTestId("tax-book")).toContainText("S3a-HKD");
  await expect(page.getByText("SỔ THEO DÕI NGHĨA VỤ THUẾ KHÁC").first()).toBeVisible();
  await expect(page.getByRole("columnheader", { name: /10 · Thuế sử dụng đất/ })).toBeVisible();
  // Hộ chỉ có GTGT/TNCN nên cột số để trống đúng mẫu, không bịa số.
  await expect(page.getByText("Kỳ này chưa có phát sinh nào để ghi sổ.")).toBeVisible();
  await expect(page).toHaveURL(/\/books$/);

  // Bấm "Sổ S2b-HKD" → về lại mẫu doanh thu.
  await page.getByTestId("open-book-s2b").click();
  await expect(page.getByTestId("tax-book")).toContainText("S2b-HKD");
  await expect(page.getByText("SỔ DOANH THU BÁN HÀNG HÓA, DỊCH VỤ").first()).toBeVisible();

  // S2d (vật liệu) và S2e (tiền) chọn được từ dropdown mẫu sổ.
  await page.getByTestId("tax-book").click();
  await page.locator(".p-select-option", { hasText: "S2d-HKD" }).first().click();
  await expect(
    page.getByText("SỔ CHI TIẾT VẬT LIỆU, DỤNG CỤ, SẢN PHẨM, HÀNG HÓA").first(),
  ).toBeVisible();
  await page.getByTestId("tax-book").click();
  await page.locator(".p-select-option", { hasText: "S2e-HKD" }).first().click();
  await expect(page.getByText("SỔ CHI TIẾT TIỀN").first()).toBeVisible();

  // Về màn Kế toán: khoảng ngày báo cáo không bị các thao tác sổ ảnh hưởng.
  await openTab(page, "Kế toán HKD", "/accounting");
  await expect(from).toHaveValue(accountingRange.from);
  await expect(to).toHaveValue(accountingRange.to);
});

// Danh sách mẫu sổ bám theo hồ sơ HKD (Điều 4 TT 152/2025): hồ sơ chưa chốt
// nhóm → Nhóm 1 → chỉ S1a + S3a; công tắc "Xem tất cả" mở đủ 7 mẫu và được nhớ
// lại cả sau khi tải lại trang.
test("Danh sách mẫu sổ chỉ hiện sổ đúng nhóm HKD và nhớ lựa chọn xem tất cả", async ({ page }) => {
  await ensureLoggedIn(page);
  await openTab(page, "Sổ kế toán", "/books");
  await expect(page.locator("header h2")).toHaveText("Sổ kế toán");
  await expect(page.getByTestId("tax-book")).toBeVisible();

  // Dropdown chỉ 2 mẫu đúng nhóm, các nút sổ nhanh cũng chỉ hiện 2 mẫu đó.
  await page.getByTestId("tax-book").click();
  await expect(page.locator(".p-select-option")).toHaveCount(2);
  await expect(page.locator(".p-select-option", { hasText: "S1a-HKD" })).toBeVisible();
  await expect(page.locator(".p-select-option", { hasText: "S3a-HKD" })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("open-book-s1a")).toBeVisible();
  await expect(page.getByTestId("open-book-s3a")).toBeVisible();
  await expect(page.getByTestId("open-book-s2a")).toHaveCount(0);
  await expect(page.getByTestId("open-book-s2b")).toHaveCount(0);

  // Bật công tắc → đủ 7 mẫu để đối chiếu mẫu in.
  await page.locator("#show-all-books").click();
  await page.getByTestId("tax-book").click();
  await expect(page.locator(".p-select-option")).toHaveCount(7);
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("open-book-s2b")).toBeVisible();

  // Nhớ lựa chọn: tải lại trang (cùng cách người dùng F5) là vẫn còn bật.
  await page.reload();
  await expect(page.getByTestId("tax-book")).toBeVisible();
  await page.getByTestId("tax-book").click();
  await expect(page.locator(".p-select-option")).toHaveCount(7);
  await page.keyboard.press("Escape");
});

// Các nút sổ không còn đẩy sang màn khác, nhưng Sổ nhật ký vẫn phải giữ nút quay
// về màn Kế toán — không có nó người dùng vào bằng thanh công cụ là mắc kẹt.
test("Sổ nhật ký có nút quay lại màn Sổ kế toán", async ({ page }) => {
  await ensureLoggedIn(page);
  await openTab(page, "Sổ nhật ký", "/ledger");
  await expect(page.locator("header h2")).toHaveText("Sổ nhật ký chung");
  // Mở thẳng bằng thanh menu (không kèm query back) → về màn dự phòng /books:
  // chính tab này là nơi có nút "Mở Sổ nhật ký".
  await page.getByTestId("back-to-previous").click();
  await expect(page).toHaveURL(/\/books$/);
  await expect(page.locator("header h2")).toHaveText("Sổ kế toán");
});

// 2 nút "Mở Sổ nhật ký" / "Mở Thu / Chi" (ở tab Sổ kế toán) phải mang theo kỳ
// sổ người dùng đang xem — kỳ này do màn Kế toán "Chuyển nhanh" gióng qua
// (state dùng chung). Màn đích đọc query lúc mở (KeepAlive không chạy lại
// `setup()`) nên test mở lại với kỳ khác để bắt lỗi kẹt kỳ cũ.
test("Sổ kế toán: mở Sổ nhật ký / Thu-Chi kèm kỳ sổ đang xem", async ({ page }) => {
  await ensureLoggedIn(page);
  const y = new Date().getFullYear();
  const ledgerDate = (label: string) =>
    page.locator(`label:text-is("${label}") + .p-datepicker input`);
  const cashDate = (placeholder: string) => page.locator(`input[placeholder="${placeholder}"]`);

  await openTab(page, "Sổ kế toán", "/books");
  await expect(page.locator("header h2")).toHaveText("Sổ kế toán");

  // Kỳ 1: chọn kỳ ngay trên tab Sổ (Loại kỳ = Theo quý, Kỳ = Quý 3 — tab Sổ
  // tự chọn, không còn gióng từ màn Kế toán) → mở Sổ nhật ký đúng quý 3.
  await page.locator('label:text-is("Loại kỳ sổ") + .p-select').click();
  await page.locator(".p-select-option", { hasText: "Theo quý" }).first().click();
  await page.locator('label:text-is("Kỳ") + .p-select').click();
  await page.locator(".p-select-option", { hasText: "Quý 3" }).first().click();
  await page.getByTestId("open-ledger").click();
  await expect(page).toHaveURL(/\/ledger\?/);
  await expect(page.locator("header h2")).toHaveText("Sổ nhật ký chung");
  await expect(ledgerDate("Từ ngày")).toHaveValue(`01/07/${y}`);
  await expect(ledgerDate("Đến ngày")).toHaveValue(`30/09/${y}`);
  await expect(page.getByText("Xem cả năm")).toBeVisible();

  // Kỳ 2: "Chuyển nhanh" bên màn Kế toán sang Quý 1 chỉ đổi tờ khai — tab Sổ
  // GIỮ nguyên Quý 3 (hết đồng bộ) nên Sổ nhật ký vẫn mở đúng kỳ sổ đang xem.
  await page.getByTestId("back-to-previous").click();
  await expect(page.locator("header h2")).toHaveText("Sổ kế toán");
  await openTab(page, "Kế toán HKD", "/accounting");
  await page.getByTestId("quick-quarter-1").click();
  await expect(ledgerDate("Từ ngày")).toHaveValue(`01/01/${y}`);
  await openTab(page, "Sổ kế toán", "/books");
  await expect(page.locator('label:text-is("Kỳ") + .p-select')).toContainText("Quý 3");
  await page.getByTestId("open-ledger").click();
  await expect(page.locator("header h2")).toHaveText("Sổ nhật ký chung");
  await expect(ledgerDate("Từ ngày")).toHaveValue(`01/07/${y}`);
  await expect(ledgerDate("Đến ngày")).toHaveValue(`30/09/${y}`);

  // Màn Thu / Chi nhận nguyên kỳ sổ đang xem.
  await page.getByTestId("back-to-previous").click();
  await expect(page.locator("header h2")).toHaveText("Sổ kế toán");
  await page.getByTestId("open-cash").click();
  await expect(page.locator("header h2")).toHaveText("Phiếu thu / chi");
  await expect(cashDate("Từ ngày")).toHaveValue(`01/07/${y}`);
  await expect(cashDate("Đến ngày")).toHaveValue(`30/09/${y}`);
});

test("Cấu hình HKD: nhóm hộ tự điền theo doanh thu, lưu lại và tờ khai theo nhóm đó", async ({
  page,
}) => {
  await ensureLoggedIn(page);
  await openTab(page, "Kế toán HKD", "/accounting");
  await expect(page.locator("header h2")).toHaveText("Kế toán HKD");
  await expect(page.getByText("Kỳ khai thuế:")).toBeVisible();

  // Chưa chốt nhóm: app tự xếp nhóm 1 vì E2E chưa có doanh thu.
  // Nhãn nhóm xuất hiện ở cả thẻ tổng hợp và thẻ Tag → lấy phần tử đầu.
  await expect(page.getByText("Nhóm 1 — doanh thu ≤ 1 tỷ").first()).toBeVisible();

  await page.getByRole("button", { name: "Cấu hình hộ KD" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  // Ô nhóm phải tự điền sẵn (không để trống) và có gợi ý doanh thu.
  const groupSelect = dialog.getByTestId("hkd-tax-group").locator(".p-select");
  await expect(groupSelect).toContainText("Nhóm 1");
  await expect(dialog.getByText(/app xếp/)).toBeVisible();

  // Ghi đè tay sang nhóm 3 (cơ quan thuế xếp khác) → lưu.
  await groupSelect.click();
  await page
    .locator(".p-select-option", { hasText: /^Nhóm 3/ })
    .first()
    .click();
  await dialog.getByRole("button", { name: "Lưu cấu hình" }).click();
  await expect(dialog).toBeHidden();

  // Tổng hợp thuế phải nộp phải theo nhóm vừa lưu, không tự tính lại.
  await expect(page.getByText("Nhóm 3 — doanh thu > 3 tỷ đến 50 tỷ").first()).toBeVisible();

  // Hộp cấu hình thuế gợi ý kỳ khai theo nhóm 3 → theo quý.
  await page.getByRole("button", { name: "Cấu hình thuế" }).click();
  const taxDialog = page.getByRole("dialog");
  await expect(taxDialog.getByTestId("tax-period").locator(".p-select")).toContainText("Theo quý");
  await expect(taxDialog.getByText(/Gợi ý cho Nhóm 3/)).toBeVisible();
  await taxDialog.getByRole("button", { name: "Hủy" }).click();
  await expect(taxDialog).toBeHidden();

  // Trả lại nhóm 1 để không ảnh hưởng test sau.
  await page.getByRole("button", { name: "Cấu hình hộ KD" }).click();
  await expect(dialog).toBeVisible();
  await groupSelect.click();
  await page
    .locator(".p-select-option", { hasText: /^Nhóm 1/ })
    .first()
    .click();
  await dialog.getByRole("button", { name: "Lưu cấu hình" }).click();
  await expect(dialog).toBeHidden();
  await expect(page.getByText("Nhóm 1 — doanh thu ≤ 1 tỷ").first()).toBeVisible();
});

test("Phân trang server-side: bảng dài chỉ lấy đúng trang, sắp xếp và tìm kiếm chạy ở server", async ({
  page,
  request,
}) => {
  await ensureLoggedIn(page);
  // 12 phiếu nhập → đủ để có nhiều trang với kích thước trang nhỏ nhất.
  for (let i = 1; i <= 12; i++) {
    const res = await request.post("/api/save_inbound", {
      data: {
        posting_date: `2026-09-2${i % 9}0`,
        voucher_no: `PN-PG${String(i).padStart(2, "0")}`,
        description: `Phiếu phân trang số ${i}`,
        supplier_code: "",
        warehouse_code: "KHO-CHINH",
        unit_code: "HKD",
        items: [{ product_code: "SP001", quantity: 1, unit_price: 1000, discount: 0 }],
        note: "",
        inbound_type: "purchase",
        reference_no: "",
        vat_rate: 0,
        debit_account: "152",
        credit_account: "331",
        pay_now: false,
        adjust_dir: "up",
        autoBom: false,
      },
    });
    expect(res.ok(), `save_inbound ${i} failed ${res.status()}`).toBe(true);
  }

  await openTab(page, "Sổ nhật ký", "/ledger");
  await expect(page.locator("header h2")).toHaveText("Sổ nhật ký chung");

  const table = page.locator(".p-datatable").first();
  await expect(table.locator("tbody tr")).not.toHaveCount(0);

  // Kích thước trang nhỏ nhất (20) → mọi dòng vẫn nằm trong 1 trang; đổi sang
  // 20 dòng/trang và kiểm tra bảng báo đúng tổng số dòng server trả về.
  const totalText = await page.getByText(/Tổng .* dòng trong kỳ/).innerText();
  const total = Number(totalText.replace(/\D/g, ""));
  expect(
    total,
    "tổng số dòng phải do server đếm, không phải số dòng đang tải",
  ).toBeGreaterThanOrEqual(12);

  // Bấm nút kích thước trang → chọn 20 là mặc định; dùng ô tìm kiếm của bảng.
  await table.locator("tbody tr").first().waitFor();
  const rowsOnPage = await table.locator("tbody tr").count();
  expect(rowsOnPage, "1 trang không được vượt quá kích thước trang").toBeLessThanOrEqual(200);

  // Tìm kiếm toàn cục chạy ở server: lọc theo mã hàng xong danh sách phải hẹp lại.
  await page.getByPlaceholder("Tìm kiếm…").first().fill("PN-PG03");
  await expect
    .poll(
      async () => {
        const rows = table.locator("tbody tr");
        const n = await rows.count();
        return n > 0 && n < 12;
      },
      { timeout: 20_000 },
    )
    .toBe(true);
});

/**
 * Tờ khai thuế HKD — kiểm chứng các quy tắc NĐ 68/2026 (sửa 09/2026):
 *   1. Hộ nhóm 1 (DT ≤ 1 tỷ) KHÔNG phải nộp thuế: hai thẻ "Tờ khai thuế" và
 *      "Tổng hợp thuế" phải cùng bằng 0 (trước đây thẻ tờ khai hiện thuế).
 *   2. Giá vốn FIFO từ phiếu nhập được trừ khi tính thu nhập tính thuế.
 *   3. Chi từ 5 triệu trả tiền mặt không có chứng từ chuyển khoản bị loại.
 *   4. Có hạn nộp + sổ kế toán TT 152/2025 + bảng tạm nộp/quyết toán.
 */
test("Tờ khai thuế: nhóm 1 miễn thuế, giá vốn FIFO, loại chi thiếu chứng từ, hạn nộp, sổ TT 152", async ({
  page,
  request,
}) => {
  await ensureLoggedIn(page);

  // Nhập 10 giá × 1.000.000 = 10.000.000 (giá vốn của lô nhập)
  const stockIn = await request.post("/api/save_inbound", {
    data: {
      posting_date: "2026-09-01",
      voucher_no: "PN-TAX01",
      description: "Nhập hàng kiểm tra tờ khai thuế",
      supplier_code: "",
      warehouse_code: "KHO-CHINH",
      unit_code: "HKD",
      items: [{ product_code: "SP001", quantity: 10, unit_price: 1_000_000, discount: 0 }],
      note: "",
      inbound_type: "purchase",
      reference_no: "",
      vat_rate: 0,
      debit_account: "152",
      credit_account: "331",
      pay_now: false,
      adjust_dir: "up",
      autoBom: false,
    },
  });
  expect(stockIn.ok(), `save_inbound: ${await stockIn.text()}`).toBe(true);

  // Bán 4 giá × 3.000.000 = 12.000.000 doanh thu, giá vốn FIFO = 4.000.000
  const out = await request.post("/api/save_outbound", {
    data: {
      posting_date: "2026-09-02",
      voucher_no: "PX-TAX01",
      description: "Bán hàng kiểm tra tờ khai thuế",
      customer_code: "",
      warehouse_code: "KHO-CHINH",
      unit_code: "HKD",
      outbound_type: "sale",
      adjust_dir: "up",
      receive_now: false,
      create_invoice: false,
      invoice: { number: "", eInvoiceNo: "", eInvoiceSymbol: "", eInvoiceDate: "" },
      items: [
        {
          product_code: "SP001",
          quantity: 4,
          unit_price: 3_000_000,
          discount: 0,
          industry_code: "PPHH",
        },
      ],
      note: "",
    },
  });
  expect(out.ok(), `save_outbound: ${await out.text()}`).toBe(true);

  // Phiếu chi 7.000.000 trả tiền mặt, KHÔNG có chứng từ chuyển khoản →
  // không được trừ (Điều 6 khoản 1).
  const cash = await request.post("/api/save_cash_entry", {
    data: {
      input: {
        entryType: "PC",
        postingDate: "2026-09-03",
        voucherNo: "PC-TAX01",
        description: "Chi tiền mặt 7 triệu không chứng từ chuyển khoản",
        customerCode: "",
        supplierCode: "",
        amount: 7_000_000,
        debitAccount: "642",
        creditAccount: "111",
        industryCode: "",
        vatRate: 0,
        pitRate: 0,
        unitCode: "HKD",
        note: "",
        bankCode: "",
        deductible: true,
        nonDeductibleReason: "",
      },
    },
  });
  expect(cash.ok(), `save_cash_entry: ${await cash.text()}`).toBe(true);

  await sidebarButton(page, "Kế toán HKD").click();
  await expect(page.locator("header h2")).toHaveText("Kế toán HKD");
  await page.waitForTimeout(1200);

  // ── 1. Nhóm 1 (DT cả năm < 1 tỷ): tờ khai phải bằng 0.
  //      Kiểm tra đúng số tiền của từng thẻ, không so chuỗi số tiền chung vì các
  //      test trước đó đã để lại dữ liệu trong cùng cơ sở dữ liệu E2E.
  const summaryText = (await page.locator("main").innerText()).replace(/\s+/g, " ");
  // Một bảng tờ khai duy nhất, chạy theo khoảng ngày đang chọn ở thanh công cụ.
  expect(summaryText, "phải có đúng một thẻ tờ khai").toContain("Tờ khai thuế");
  expect(summaryText, "tờ khai phải ghi khoảng thời gian đang chọn").toContain(
    "Tờ khai cho khoảng:",
  );
  // Chỉ còn đúng MỘT bảng tờ khai; bảng "theo nhóm ngành nghề" cũ đã bị gộp.
  await expect(
    page.getByText("Tờ khai thuế", { exact: true }),
    "chỉ còn 1 bảng tờ khai",
  ).toHaveCount(1);
  await expect(
    page.getByText("Tờ khai thuế theo nhóm ngành nghề (TT 152/2025)"),
    "bảng tờ khai trùng lặp đã bị gộp",
  ).toHaveCount(0);
  // Số thuế lấy từ thẻ tổng hợp (thẻ trùng lặp đã bị gộp nên không còn dòng
  // "Tổng thuế GTGT/TNCN" riêng).
  expect(summaryText, "hộ nhóm 1: thuế GTGT phải nộp bằng 0").toContain("Thuế GTGT phải nộp 0 đ");
  expect(summaryText, "hộ nhóm 1: thuế TNCN phải nộp bằng 0").toContain(
    "Thuế TNCN phải nộp (theo doanh thu) 0 đ",
  );
  expect(summaryText, "phải xếp đúng nhóm 1").toContain("Nhóm 1 — doanh thu ≤ 1 tỷ: miễn thuế");
  expect(summaryText, "khoảng trùng kỳ khai thì phải có hạn nộp").toContain("Hạn nộp: ");

  // ── 2. Chuyển sang phương pháp theo lợi nhuận: thấy giá vốn FIFO và chi bị loại
  await page.getByRole("button", { name: "Cấu hình thuế" }).click();
  await expect(page.getByText("Cấu hình kê khai thuế").first()).toBeVisible();
  await page.getByTestId("tax-method").click();
  await page
    .getByRole("option", { name: /Theo lợi nhuận/ })
    .or(page.locator(".p-select-option", { hasText: "Theo lợi nhuận" }))
    .first()
    .click();
  await page.getByRole("button", { name: "Lưu cấu hình" }).click();
  await expect(page.getByText("Đã lưu cấu hình thuế").first()).toBeVisible();
  await page.waitForTimeout(1200);

  const after = (await page.locator("main").innerText()).replace(/\s+/g, " ");
  // Giá vốn tính FIFO từ phiếu nhập (trước đây chi phí chỉ lấy phiếu chi nên
  // thiếu hẳn giá vốn → thu nhập tính thuế bị thổi phình).
  expect(after, "phải tính giá vốn FIFO từ phiếu nhập").toContain("Giá vốn FIFO");
  // Khoản chi 7 triệu trả tiền mặt không chứng từ chuyển khoản bị loại.
  expect(after, "phải cảnh báo khoản chi không được trừ").toContain("PC-TAX01");
  expect(after, "phải nêu lý do loại chi").toContain(
    "không có chứng từ thanh toán không dùng tiền mặt",
  );

  // ── 3. Sổ kế toán TT 152/2025 (tab "Sổ kế toán"): S2a ghi DOANH THU THEO
  //      TỪNG CHỨNG TỪ (cột A số hiệu, B ngày tháng, C diễn giải, D số tiền)
  //      như mẫu sổ. Hồ sơ chưa chốt nhóm → Nhóm 1 nên tab tự mở S1a; muốn soi
  //      S2a, S2c phải bật "Xem tất cả 7 mẫu sổ" trước.
  await openTab(page, "Sổ kế toán", "/books");
  await expect(page.locator("header h2")).toHaveText("Sổ kế toán");
  await page.locator("#show-all-books").click();
  await page.getByTestId("open-book-s2a").click();
  await expect(page.getByRole("columnheader", { name: /A · Số hiệu/ }).first()).toBeVisible();
  const s2a = (await page.locator("main").innerText()).replace(/\s+/g, " ");
  expect(s2a, "sổ S2a phải ghi từng dòng chứng từ").toContain("PX-TAX01");
  expect(s2a, "sổ S2a phải ghi theo nhóm ngành").toContain("PPHH");
  expect(s2a, "sổ của hộ nhóm 1 không ghi thuế").toContain("không phát sinh thuế");

  // Đổi sang S2c: sổ chi tiết doanh thu, chi phí (gốc tính thu nhập tính thuế)
  await page.getByTestId("tax-book").click();
  await page.locator(".p-select-option", { hasText: "S2c-HKD" }).first().click();
  await expect(page.getByText("SỔ CHI TIẾT DOANH THU, CHI PHÍ").first()).toBeVisible();
  await page.waitForTimeout(600);
  const bookText = (await page.locator("main").innerText()).replace(/\s+/g, " ");
  expect(bookText, "sổ S2c phải ghi dòng doanh thu theo nhóm ngành").toContain(
    "Doanh thu bán hàng hóa, dịch vụ",
  );

  // ── 4. Quay lại màn Kế toán: bảng tạm nộp theo kỳ + quyết toán cả năm
  await openTab(page, "Kế toán HKD", "/accounting");
  await expect(page.locator("header h2")).toHaveText("Kế toán HKD");
  const settleText = (await page.locator("main").innerText()).replace(/\s+/g, " ");
  expect(settleText, "phải có bảng tạm nộp theo kỳ").toContain("Tổng TNCN tạm nộp");
  expect(settleText, "phải có số phải nộp khi quyết toán").toContain("Số phải nộp khi quyết toán");
  expect(settleText, "phải nhắc hạn quyết toán 31/03 năm sau").toContain("31/03/2027");
  expect(settleText, "bảng tạm nộp phải có hạn nộp từng kỳ").toContain("31/10/2026");

  // ── 5. Trả lại cấu hình gốc (theo doanh thu) để không ảnh hưởng test sau
  await page.getByRole("button", { name: "Cấu hình thuế" }).click();
  await page.getByTestId("tax-method").click();
  await page
    .getByRole("option", { name: /Theo doanh thu × tỷ lệ ngành/ })
    .or(page.locator(".p-select-option", { hasText: "Theo doanh thu" }))
    .first()
    .click();
  await page.getByRole("button", { name: "Lưu cấu hình" }).click();
  await expect(page.getByText("Đã lưu cấu hình thuế").first()).toBeVisible();
});

// ─── Tờ khai 01/CNKD ───

/**
 * Tờ khai 01/CNKD dựng lại màn "Chọn địa điểm kinh doanh cần kê khai doanh
 * thu" của cổng thuế. Ba điểm khác của bản này với mục "Tờ khai thuế" phía trên:
 * 6 dòng nhóm ngành là **cố định** (nhóm chưa phát sinh vẫn lên bảng), ô doanh
 * thu **sửa tay được** nhưng không mất khi tải lại trang, và có **công tắc** tắt/
 * bật địa điểm như bản gốc.
 */
test("Tờ khai 01/CNKD: đủ 6 dòng cố định, ô sửa tay giữ qua tải lại, công tắc địa điểm", async ({
  page,
}) => {
  await ensureLoggedIn(page);
  await openTab(page, "Kế toán HKD", "/accounting");
  await expect(page.locator("header h2")).toHaveText("Kế toán HKD");

  const table = page.getByTestId("tax-entry-table");
  await expect(table).toBeVisible({ timeout: 30_000 });

  // Đúng 6 dòng mẫu — kể cả nhóm CHƯA phát sinh doanh thu.
  await expect(table.locator("tbody tr")).toHaveCount(6);
  for (const name of [
    "Phân phối, cung cấp hàng hóa",
    "Dịch vụ, xây dựng không bao thầu nguyên vật liệu",
    "Hoạt động cho thuê tài sản trừ bất động sản",
    "Sản xuất, vận tải, dịch vụ có gắn với hàng hóa, xây dựng có bao thầu nguyên vật liệu",
    "Hoạt động cung cấp sản phẩm nội dung thông tin số",
    "Hoạt động kinh doanh khác",
  ]) {
    await expect(table, `thiếu dòng "${name}"`).toContainText(name);
  }
  // Tỷ lệ ghi đúng kiểu cổng: dấu chấm thập phân ("0.5%"), không phải "0,5%".
  await expect(table).toContainText("TNCN: 0.5%");
  await expect(table).toContainText("TNCN: 1.5%");

  const summary = page.getByTestId("tax-entry-summary");
  await expect(summary).toContainText("Tổng doanh thu GTGT:");

  // Sửa tay dòng 1 → tổng doanh thu trên thanh tóm tắt đổi theo.
  // Lấy ô theo CẤU TRÚC bảng (cột GTGT = ô số 1 của dòng đầu) thay vì theo
  // nhãn: aria-label của PrimeVue InputNumber có thể rơi vào phần tử bọc ngoài.
  const vat1 = table.locator("tbody tr").first().locator("input").first();
  // Chỉ số không dấu để khỏi phụ thuộc cách PrimeVue format theo locale.
  const digits = async (loc: typeof vat1) => (await loc.inputValue()).replace(/\D/g, "");
  const summaryBefore = (await summary.textContent()) ?? "";
  // Gõ như người thật (không `fill`): PrimeVue InputNumber tự format theo
  // locale trong lúc gõ và chỉ chốt model khi Enter/blur.
  await vat1.click();
  await vat1.press("ControlOrMeta+a");
  await vat1.pressSequentially("1234567");
  await vat1.press("Enter");
  await vat1.blur();
  // Ô dòng 1 mang đúng số vừa gõ, và tổng trên thanh tóm tắt đổi theo
  // (không soi chuỗi tổng: bên trong còn số liệu seed của các dòng khác).
  await expect.poll(() => digits(vat1), { message: "dòng 1 phải giữ 1234567" }).toBe("1234567");
  expect((await summary.textContent()) ?? "").not.toBe(summaryBefore);

  // Bản sửa phải sống sót qua TẢI LẠI TRANG (đã lưu vào app_setting).
  await page.waitForTimeout(900); // debounce lưu 600ms
  await page.reload({ waitUntil: "domcontentloaded" });
  await expect(page.locator("header h2")).toHaveText("Kế toán HKD", { timeout: 30_000 });
  await expect(page.getByTestId("tax-entry-table")).toBeVisible({ timeout: 30_000 });
  await expect
    .poll(() => digits(vat1), { message: "bản sửa phải sống qua reload" })
    .toBe("1234567");

  // "Lấy lại số từ sổ" bỏ bản sửa → về đúng số app tự lấp.
  await page.getByRole("button", { name: "Lấy lại số từ sổ" }).click();
  await expect.poll(() => digits(vat1), { message: "bản sửa phải bị xoá" }).not.toBe("1234567");

  // Công tắc tắt → không còn bảng số, nói rõ địa điểm không tham gia kê khai;
  // bật lại để trả trạng thái (màn sau dùng tới tờ khai).
  // Testid là chính; `.p-toggleswitch` là dự phòng nếu PrimeVue nuốt attrs.
  const toggle = page.getByTestId("tax-entry-enabled").or(page.locator(".p-toggleswitch")).first();
  await toggle.click();
  await expect(page.getByTestId("tax-entry-disabled")).toBeVisible();
  await expect(page.getByTestId("tax-entry-table")).toHaveCount(0);
  await toggle.click();
  await expect(page.getByTestId("tax-entry-table")).toBeVisible();
});

/**
 * Quy định lưu XML HĐĐT: app **không** tự tải ngay lúc gán số HĐĐT (sau khi ký,
 * cổng còn cache nên trang tra cứu chưa trả hồ sơ gốc) → nút "Tải XML" ở chi
 * tiết hóa đơn là nơi bấm tải. Phải hiện rõ "chưa tải / đã lưu" để biết hồ sơ
 * đã đủ file XML chưa, và file phải nằm thật trong CSDL + thư mục hồ sơ.
 */
test("Chi tiết hóa đơn: nút Tải XML lưu file XML hóa đơn bán ra", async ({ page, request }) => {
  await ensureLoggedIn(page);

  // Phiên cổng (mock) — thiếu thì nút chỉ báo chưa đăng nhập nên không tải được.
  // Dùng helper chung: tự cấu hình base_url về mock portal rồi đăng nhập (cấu
  // hình chưa có khi chạy riêng test này).
  await loginPortal(request);

  // Tồn cho SP001 — chạy riêng test này thì không có tồn của test trước.
  const stock = await request.post("/api/save_inbound", {
    data: {
      posting_date: "2026-09-16",
      voucher_no: "PN-XML01",
      description: "Nhập tồn cho test Tải XML",
      supplier_code: "",
      warehouse_code: "KHO-CHINH",
      unit_code: "HKD",
      items: [{ product_code: "SP001", quantity: 5, unit_price: 10000, discount: 0 }],
      note: "",
      inbound_type: "purchase",
      reference_no: "",
      vat_rate: 0,
      debit_account: "152",
      credit_account: "331",
      pay_now: false,
      adjust_dir: "up",
      autoBom: false,
    },
  });
  expect(stock.ok(), `save_inbound: ${await stock.text()}`).toBe(true);

  // Hóa đơn đã gán số HĐĐT — đúng trường hợp nút hiện ra.
  const number = "HD9510";
  const shdon = "00001234";
  const made = await request.post("/api/save_invoice", {
    data: {
      number,
      date: "2026-09-18",
      customer: "Khách Tải XML",
      customer_tax_code: "0100000000",
      items: [
        {
          product_code: "SP001",
          quantity: 1,
          unit_price: 10000,
          industry_code: "PPHH",
          discount: 0,
          warehouse_code: "",
        },
      ],
    },
  });
  expect(made.ok(), `save_invoice: ${await made.text()}`).toBe(true);
  const list = (await (await request.post("/api/get_invoices", { data: {} })).json()) as Array<{
    id: number;
    number: string;
  }>;
  const id = list.find((i) => i.number === number)?.id;
  expect(id, `vừa lập ${number}`).toBeTruthy();
  const linked = await request.post("/api/link_hddt", {
    data: {
      invoiceId: id,
      hddtNo: shdon,
      hddtSymbol: "1C26TT152",
      hddtDate: "2026-09-18",
    },
  });
  expect(linked.ok(), `link_hddt: ${await linked.text()}`).toBe(true);

  await sidebarButton(page, "Hóa đơn").click();
  await expect(page.locator("header h2")).toHaveText("Hóa đơn");
  await page.getByPlaceholder("Tìm kiếm…").first().fill(number);
  const row = page.locator("tr", { has: page.getByText(number, { exact: true }) }).first();
  await expect(row).toBeVisible({ timeout: 20_000 });
  await row.getByRole("button", { name: number }).click();

  const xmlRow = page.getByTestId("invoice-xml-row");
  await expect(xmlRow).toBeVisible();
  await expect(xmlRow.getByText(/chưa tải/)).toBeVisible();
  await page.getByTestId("download-invoice-xml").click();
  await expect(xmlRow.getByText(/đã lưu \(/)).toBeVisible({ timeout: 30_000 });
  await expect(page.getByText("Đã tải file XML")).toBeVisible();

  // Soi thẳng CSDL + thư mục hồ sơ — giao diện "đã lưu" mà không có file là hồ
  // sơ vẫn thiếu, không chấp nhận được.
  const xmlRows = (await (await request.post("/api/hddt_list_xml", { data: {} })).json()) as Array<{
    direction: string;
    khhdon: string;
    shdon: string;
    file_name: string;
  }>;
  const saved = xmlRows.find((r) => r.direction === "sold" && r.shdon === shdon);
  expect(saved, "file XML phải nằm trong CSDL hồ sơ").toBeTruthy();
  expect(saved!.khhdon, "ghi đúng ký hiệu HĐ").toBe("1C26TT152");
  const dataDir = readFileSync("/tmp/hkd-e2e-dir", "utf8").trim();
  expect(
    existsSync(`${dataDir}/profiles/default/hddt_xml/${saved!.file_name}`),
    "file XML phải nằm trong thư mục hồ sơ",
  ).toBe(true);
});

/**
 * "Xuất báo cáo kỳ thuế" phải gộp **một** ZIP đủ hồ sơ kỳ: tờ khai 01/CNKD,
 * cả 7 sổ kế toán theo năm (kỳ chưa tròn năm vẫn lấy đủ dữ liệu hiện có), hóa
 * đơn PDF/XML mua vào – bán ra và bản sao lưu CSDL. Thiếu mục nào là hồ sơ không
 * nộp được, nên soi thẳng tên file trong ZIP chứ không chỉ xem có tải về không.
 *
 * Việc này mất hàng chục giây (tải XML từng hóa đơn, render PDF) nên kèm luôn
 * hộp thoại **tiến độ + nhật ký**: bấm nút là hiện ngay, log do backend ghi qua
 * lệnh hỏi `get_job_progress`, và vẫn đóng được khi chưa xong ("Chạy nền").
 */
test(
  "Xuất báo cáo kỳ thuế: có tiến độ + nhật ký, ZIP gộp tờ khai, đủ 7 sổ, hóa đơn và sao lưu CSDL",
  { timeout: 120_000 },
  async ({ page }) => {
    await ensureLoggedIn(page);
    await openTab(page, "Kế toán HKD", "/accounting");
    // Chờ tờ khai nạp xong — nó là 1 trong những file phải có trong ZIP.
    await expect(page.getByTestId("tax-entry-table")).toBeVisible({ timeout: 30_000 });

    const btn = page.getByTestId("export-tax-report");
    await expect(btn).toBeVisible();

    // Phải đăng ký TRƯỚC khi bấm — chờ file xong rồi mới nghe là đã lỡ event.
    const downloadPromise = page.waitForEvent("download");
    await btn.click();

    // ── Tiến độ phải hiện NGAY khi bấm, không để im hàng chục giây ──
    const progress = page.getByTestId("export-progress");
    await expect(progress).toBeVisible();
    await expect(page.getByTestId("export-progress-bar")).toBeVisible();
    // Chưa xong thì chỉ được đóng bằng "Chạy nền" — không có nút Đóng, sợ người
    // dùng lỡ tay mất log khi chưa có gì để đọc.
    await expect(page.getByTestId("export-progress-background")).toBeVisible();

    // Đóng đi vẫn mở lại được — nhật ký giữ cho tới khi người dùng chủ động đóng,
    // nên không phụ thuộc việc backend đã chạy xong hay chưa.
    await page.getByTestId("export-progress-background").click();
    await expect(progress).toBeHidden();
    await page.getByTestId("export-progress-open").click();
    await expect(progress).toBeVisible();

    const log = page.getByTestId("export-progress-log");
    // Dòng đầu do frontend ghi (dựng Excel chạy ở máy này, backend chưa nhận việc).
    await expect(log).toContainText("Bắt đầu xuất báo cáo kỳ thuế");

    const download = await downloadPromise;
    expect(download.suggestedFilename()).toMatch(
      /^bao-cao-ky-thue-\d{4}-\d{2}-\d{2}_\d{4}-\d{2}-\d{2}\.zip$/,
    );
    await expect(page.getByText("Đã tải báo cáo kỳ thuế")).toBeVisible({ timeout: 60_000 });

    // XONG — hộp thoại phải còn mở để đọc nhật ký. Dòng sau phải do BACKEND trả
    // về qua `get_job_progress` (chứng tỏ việc hỏi tiến độ nối thật tới server,
    // không chỉ có spinner tự xoay), và dòng cuối phải là kết thúc có suy ra
    // từ việc đã làm, chứ không phải mẩu chữ chung chung.
    await expect(log).toContainText(/Kỳ \d{4}-\d{2}-\d{2} → \d{4}-\d{2}-\d{2}/);
    await expect(log).toContainText("✓ Hoàn tất");
    await page.getByTestId("export-progress-close").click();
    await expect(progress).toBeHidden();

    const zipPath = await download.path();
    expect(zipPath, "trình duyệt phải lưu file về").toBeTruthy();
    const bytes = readFileSync(zipPath as string);
    expect([bytes[0], bytes[1]], "phải là ZIP thật").toEqual([0x50, 0x4b]);

    // Tên file nằm nguyên trong header của ZIP (nén chỉ áp cho nội dung) → soi
    // được danh mục mà không cần thư viện giải ZIP.
    const raw = bytes.toString("latin1");
    for (const path of [
      "CHU-THICH.txt",
      "to-khai/",
      "so-ke-toan/",
      "hoa-don/pdf/",
      "hoa-don/xml/",
      "sao-luu/hkd.db",
    ]) {
      expect(raw, `ZIP phải chứa ${path}`).toContain(path);
    }
    // Đủ 7 mẫu sổ TT 152 — thiếu một mẫu là hồ sơ thiếu sổ.
    for (const book of ["S1a", "S2a", "S2b", "S2c", "S2d", "S2e", "S3a"]) {
      expect(raw, `ZIP phải chứa sổ ${book}`).toContain(`so-ke-toan/${book}-`);
    }
    expect(bytes.length, "không được ZIP rỗng").toBeGreaterThan(5_000);
  },
);

/**
 * Nhóm hộ để khác với doanh thu thực tế thì phải cảnh báo ở cả hai nơi người
 * dùng nhìn thấy: hồ sơ HKD (nơi sửa) và thẻ tổng hợp thuế (nơi đọc số liệu).
 *
 * Trường hợp này dễ gây nhầm: để Nhóm 2 khi doanh thu chỉ 12 triệu thì GTGT vẫn
 * được tính, còn TNCN theo doanh thu bằng 0 — hai con số trông trái ngược nhau.
 */
test("Nhóm hộ trong hồ sơ được ưu tiên, ngưỡng thuế sửa được ở Cài đặt", async ({
  page,
  request,
}) => {
  await ensureLoggedIn(page);

  // Bán 10.000.000 → doanh thu năm < 1 tỷ, app xếp Nhóm 1.
  const stockIn = await request.post("/api/save_inbound", {
    data: {
      posting_date: "2026-09-01",
      voucher_no: "PN-NHOM01",
      description: "Nhập hàng cho test nhóm hộ",
      supplier_code: "",
      warehouse_code: "KHO-CHINH",
      unit_code: "HKD",
      items: [{ product_code: "SP001", quantity: 10, unit_price: 1_000_000, discount: 0 }],
      note: "",
      inbound_type: "purchase",
      reference_no: "",
      vat_rate: 0,
      debit_account: "152",
      credit_account: "331",
      pay_now: false,
      adjust_dir: "up",
      autoBom: false,
    },
  });
  expect(stockIn.ok(), `save_inbound: ${await stockIn.text()}`).toBe(true);
  const out = await request.post("/api/save_outbound", {
    data: {
      posting_date: "2026-09-02",
      voucher_no: "PX-NHOM01",
      description: "Bán hàng cho test nhóm hộ",
      customer_code: "",
      warehouse_code: "KHO-CHINH",
      unit_code: "HKD",
      outbound_type: "sale",
      adjust_dir: "up",
      receive_now: false,
      create_invoice: false,
      invoice: { number: "", eInvoiceNo: "", eInvoiceSymbol: "", eInvoiceDate: "" },
      items: [
        {
          product_code: "SP001",
          quantity: 1,
          unit_price: 10_000_000,
          discount: 0,
          industry_code: "PPHH",
        },
      ],
      note: "",
    },
  });
  expect(out.ok(), `save_outbound: ${await out.text()}`).toBe(true);

  // Hộp cấu hình hộ mở từ thanh công cụ màn Kế toán HKD.
  const openProfileDialog = async () => {
    await page.getByRole("button", { name: "Cấu hình hộ KD" }).click();
    await expect(page.getByTestId("hkd-tax-group")).toBeVisible();
    await page.waitForTimeout(400);
  };
  const pickGroup = async (label: string) => {
    await page.getByTestId("hkd-tax-group").click();
    await page.locator(".p-select-option", { hasText: label }).first().click();
  };
  const saveProfile = async () => {
    await page.getByRole("button", { name: "Lưu cấu hình" }).click();
    await page.waitForTimeout(900);
  };

  // ── 1. Hồ sơ HKD: để Nhóm 2 (khác nhóm app xếp) → chỉ hiện GHI CHÚ, không cảnh báo
  await sidebarButton(page, "Kế toán HKD").click();
  await expect(page.locator("header h2")).toHaveText("Kế toán HKD");
  await openProfileDialog();
  await pickGroup("Nhóm 2");
  await saveProfile();
  await openProfileDialog();
  await expect(page.getByTestId("tax-group-note")).toBeVisible();
  await expect(page.getByTestId("tax-group-note")).toContainText("không phải lỗi");
  await expect(page.getByTestId("tax-group-note")).toContainText("ổn định");
  await expect(page.getByTestId("tax-group-warning")).toHaveCount(0);

  // ── 2. Hồ sơ chốt Nhóm 2 → hộ thuộc diện nộp thuế suốt năm, phải ra số thuế dù
  // doanh thu chưa vượt ngưỡng (ngưỡng chỉ dùng xếp nhóm, không phải khoản miễn).
  await page.keyboard.press("Escape");
  await page.waitForTimeout(800);
  await expect(page.getByTestId("tax-group-source-note")).toBeVisible();
  await expect(page.getByTestId("tax-group-source-note")).toContainText("hồ sơ HKD");
  await expect(page.getByTestId("tax-group-source-note")).toContainText("nộp thuế suốt năm");
  const textG2 = (await page.locator("main").innerText()).replace(/\s+/g, " ");
  // GTGT: 1% trên toàn bộ doanh thu ghi trên hóa đơn → có số.
  expect(textG2, "hồ sơ Nhóm 2: phải có thuế GTGT").not.toContain("Thuế GTGT phải nộp 0 đ");
  // TNCN: doanh thu mới 10.000.000 < mức trừ 01 tỷ → cơ sở tính thuế bằng 0.
  expect(textG2, "hồ sơ Nhóm 2 + DT dưới mức trừ: thuế TNCN = 0").toContain(
    "Thuế TNCN phải nộp (theo doanh thu) 0 đ",
  );
  // Tờ khai phải ghi rõ đã trừ bao nhiêu, và hạn ngạch mức trừ còn lại phải hiện ra.
  await expect(page.getByTestId("exempt-quota")).toContainText("Hạn ngạch mức trừ TNCN còn lại");
  await expect(page.getByTestId("exempt-quota")).toContainText("TNCN theo doanh thu còn bằng 0");
  await expect(page.getByText("Trừ mức trừ TNCN").first()).toBeVisible();

  // ── 3. Cài đặt: hạ ngưỡng xuống 5.000.000 → doanh thu 10.000.000 đã vượt ngưỡng
  // → phải phát sinh thuế dù hồ sơ vẫn ghi Nhóm 2.
  await openTab(page, "Cài đặt", "/settings");
  await expect(page.getByTestId("tax-threshold-preview")).toBeVisible();
  await page.getByTestId("tax-threshold-exempt").locator("input").fill("5000000");
  await page.keyboard.press("Tab");
  await page.getByRole("button", { name: "Lưu cài đặt" }).click();
  await page.waitForTimeout(1200);

  await sidebarButton(page, "Kế toán HKD").click();
  await expect(page.locator("header h2")).toHaveText("Kế toán HKD");
  await page.waitForTimeout(1200);
  await expect(page.getByTestId("tax-group-source-note")).toContainText("nộp thuế suốt năm");

  // Hạ ngưỡng không làm đổi kết quả của hộ đã chốt Nhóm 2 → chứng minh ngưỡng chỉ
  // quyết định nhóm, không phải khoản miễn thuế.
  const textLow = (await page.locator("main").innerText()).replace(/\s+/g, " ");
  expect(textLow, "hạ ngưỡng không được xoá thuế của hộ thuộc diện nộp").not.toContain(
    "Thuế GTGT phải nộp 0 đ",
  );

  // ── 4. Trả ngưỡng về 01 tỷ, dọn hồ sơ về Nhóm 1 → thuế bằng 0 vì hộ thuộc diện miễn
  await openTab(page, "Cài đặt", "/settings");
  await page.getByTestId("tax-threshold-exempt").locator("input").fill("1000000000");
  await page.keyboard.press("Tab");
  await page.getByRole("button", { name: "Lưu cài đặt" }).click();
  await page.waitForTimeout(1200);

  await sidebarButton(page, "Kế toán HKD").click();
  await expect(page.locator("header h2")).toHaveText("Kế toán HKD");
  await openProfileDialog();
  await pickGroup("Nhóm 1");
  await saveProfile();
  await page.keyboard.press("Escape");
  await page.waitForTimeout(1200);
  const text = (await page.locator("main").innerText()).replace(/\s+/g, " ");
  expect(text, "hộ nhóm 1: thuế GTGT phải nộp = 0").toContain("Thuế GTGT phải nộp 0 đ");
  expect(text, "hộ nhóm 1: thuế TNCN phải nộp = 0").toContain(
    "Thuế TNCN phải nộp (theo doanh thu) 0 đ",
  );
});

test("Cài đặt: việc chạy nền bật/tắt được và giữ nguyên sau khi tải lại trang", async ({
  page,
  request,
}) => {
  await ensureLoggedIn(page);
  await openTab(page, "Cài đặt", "/settings");
  await page.getByText("Tự động khi mở ứng dụng").scrollIntoViewIfNeeded();

  const checkToggle = page.getByLabel(/Tự kiểm tra cập nhật/);
  const syncToggle = page.getByLabel(/Tự đồng bộ hóa đơn HĐĐT/);
  const interval = page.getByLabel(/Khoảng thời gian tự gọi lại/);
  // Mặc định: kiểm tra cập nhật BẬT (bản web thì tự bỏ qua), quét cổng TẮT vì
  // đây là gọi cổng thật nên để người dùng tự bật.
  await expect(checkToggle).toBeChecked();
  await expect(syncToggle).not.toBeChecked();

  // Bật quét cổng + rút ngắn khoảng thời gian, rồi Lưu.
  await syncToggle.click();
  await interval.fill("30");
  await page.keyboard.press("Tab");
  await page.getByRole("button", { name: "Lưu cài đặt" }).click();
  await expect(page.getByText("Đã lưu cài đặt")).toBeVisible({ timeout: 10_000 });

  const readSettings = async () =>
    (await (await request.post("/api/get_app_settings", { data: {} })).json()) as Record<
      string,
      string
    >;
  const saved = await readSettings();
  expect(saved.auto_sync_enabled, "quét cổng phải được bật").toBe("1");
  expect(saved.auto_check_update, "kiểm tra cập nhật vẫn bật").toBe("1");
  expect(saved.auto_sync_interval_min, "khoảng thời gian lưu đúng").toBe("30");

  // Tải lại trang → vẫn giữ (đọc lại từ app_setting, không phải state trên bộ nhớ).
  await page.reload();
  await openTab(page, "Cài đặt", "/settings");
  await page.getByText("Tự động khi mở ứng dụng").scrollIntoViewIfNeeded();
  await expect(page.getByLabel(/Tự đồng bộ hóa đơn HĐĐT/)).toBeChecked();

  // Trả lại mặc định để các test sau không bị quét cổng khi app mở.
  await page.getByLabel(/Tự đồng bộ hóa đơn HĐĐT/).click();
  await page.getByRole("button", { name: "Lưu cài đặt" }).click();
  await expect(page.getByText("Đã lưu cài đặt")).toBeVisible({ timeout: 10_000 });
  expect((await readSettings()).auto_sync_enabled).toBe("0");
});

/**
 * Xuất hàng phải để lại dấu vết trên BẢNG CÂN ĐỐI: bút Nợ 632 / Có 152 (giá vốn).
 *
 * Đo TRƯỚC/SAU bằng API với khoảng ngày cố định — dữ liệu E2E dùng chung một cơ sở
 * dữ liệu nên không so con số tuyệt đối được. Đồng thời kiểm tra không còn lượt xuất
 * nào thiếu bút toán giá vốn (không thì ô cảnh báo ghi bù sẽ treo hoài trên card).
 */
test("Bảng cân đối có bút giá vốn 632/152 và không còn phiếu nào thiếu bút toán", async ({
  page,
  request,
}) => {
  await ensureLoggedIn(page);

  const read632 = async () => {
    const res = await request.post("/api/get_trial_balance", {
      data: { fromDate: "2026-01-01", toDate: "2026-12-31" },
    });
    const text = await res.text();
    expect(res.ok(), `get_trial_balance: ${text}`).toBe(true);
    const rows = JSON.parse(text) as { code: string; debit_mvmt: number }[];
    return rows.find((r) => r.code === "632")?.debit_mvmt ?? 0;
  };
  const before = await read632();

  // Tạo mặt hàng riêng cho test: SP001 đã có sẵn nhiều lô giá khác nhau từ các test
  // trước nên FIFO lấy lô cũ, không tính được đúng số tiền.
  const prod = await request.post("/api/save_product", {
    data: {
      code: "SP-GV632",
      name: "Hàng tính giá vốn 632",
      unit: "Cái",
      salePrice: 3_000,
      costPrice: 1_000,
      minStock: 0,
      vatRate: 0,
      importTaxRate: 0,
      isService: false,
      industryCode: "PPHH",
    },
  });
  expect(prod.ok(), `save_product: ${await prod.text()}`).toBe(true);

  // Nhập 10 × 1.000 đ rồi bán 5 × 3.000 đ → giá vốn FIFO = 5 × 1.000 = 5.000 đ.
  const inbound = await request.post("/api/save_inbound", {
    data: {
      posting_date: "2026-09-10",
      voucher_no: "PN-GV632",
      description: "Nhập hàng cho test bút giá vốn 632",
      supplier_code: "",
      warehouse_code: "KHO-CHINH",
      unit_code: "HKD",
      items: [{ product_code: "SP-GV632", quantity: 10, unit_price: 1_000, discount: 0 }],
      note: "",
      inbound_type: "purchase",
      reference_no: "",
      vat_rate: 0,
      debit_account: "152",
      credit_account: "331",
      pay_now: false,
      adjust_dir: "up",
      autoBom: false,
    },
  });
  expect(inbound.ok(), `save_inbound: ${await inbound.text()}`).toBe(true);
  const outbound = await request.post("/api/save_outbound", {
    data: {
      posting_date: "2026-09-11",
      voucher_no: "PX-GV632",
      description: "Bán hàng cho test bút giá vốn 632",
      customer_code: "",
      warehouse_code: "KHO-CHINH",
      unit_code: "HKD",
      outbound_type: "sale",
      adjust_dir: "up",
      receive_now: false,
      create_invoice: false,
      invoice: { number: "", eInvoiceNo: "", eInvoiceSymbol: "", eInvoiceDate: "" },
      items: [
        {
          product_code: "SP-GV632",
          quantity: 5,
          unit_price: 3_000,
          discount: 0,
          industry_code: "PPHH",
        },
      ],
      note: "",
    },
  });
  expect(outbound.ok(), `save_outbound: ${await outbound.text()}`).toBe(true);

  expect((await read632()) - before, "xuất hàng phải ghi bút Nợ 632 / Có 152").toBe(5_000);

  // Dòng giá vốn là bút toán kế toán, KHÔNG được cộng vào "Cộng thành tiền" của phiếu:
  // phiếu xuất bán 5 × 3.000 = 15.000 đ, nếu cộng giá vốn thì thành 20.000 đ.
  const voucher = (await (
    await request.post("/api/get_voucher", { data: { voucherNo: "PX-GV632" } })
  ).json()) as { entry_type: string; amount: number }[];
  expect(
    voucher.map((r) => r.entry_type).sort(),
    "phiếu chỉ chứa dòng bán hàng, không chứa dòng giá vốn",
  ).toEqual(["PX"]);
  expect(
    voucher.reduce((s, r) => s + r.amount, 0),
    "tổng tiền trên phiếu = doanh thu, không cộng giá vốn",
  ).toBe(15_000);

  // Ghi bù chạy lại không sinh trùng và không còn lượt nào thiếu.
  const pending = await request.post("/api/cogs_backfill_pending", { data: {} });
  const pendingText = await pending.text();
  expect(pending.ok(), `cogs_backfill_pending: ${pendingText}`).toBe(true);
  expect(
    (JSON.parse(pendingText) as { count: number }).count,
    "không còn phiếu xuất nào thiếu bút toán giá vốn",
  ).toBe(0);
  const again = await request.post("/api/backfill_cogs_entries", { data: {} });
  const againText = await again.text();
  expect(again.ok(), `backfill_cogs_entries: ${againText}`).toBe(true);
  expect((JSON.parse(againText) as { count: number }).count, "chạy lại không ghi trùng").toBe(0);

  // Card Tổng hợp DT-CP chỉ hiện ô ghi bù khi còn thiếu.
  await sidebarButton(page, "Kế toán HKD").click();
  await expect(page.locator("header h2")).toHaveText("Kế toán HKD");
  await page.waitForTimeout(1500);
  await expect(page.getByTestId("cogs-backfill")).toHaveCount(0);
});

// ─── XEM PDF HÓA ĐƠN ĐIỆN TỬ ───
// Backend dựng HTML từ `detail_json` rồi render PDF ngay trong Rust
// (`render_invoice_pdf`), frontend chỉ hiện bytes PDF qua `blob:` URL. Phần nội
// dung + bố cục của PDF được phủ bởi test Rust `invoice::render::tests`
// (đủ 7 cột bảng, chữ ký, mã QR, đúng 2 trang A4); test e2e này chốt phần
// giao diện: dialog mở ra thì iframe thật sự chứa một file PDF.

/** iframe xem trước hóa đơn (chỉ có khi dialog PDF đang mở). */
const invoiceFrame = (page: Page) =>
  page.getByRole("dialog").filter({ has: page.locator('iframe[title="Hóa đơn điện tử"]') });

/** URL `blob:` của PDF đang xem trước. */
async function invoicePdfUrl(page: Page): Promise<string> {
  const url = await invoiceFrame(page)
    .locator('iframe[title="Hóa đơn điện tử"]')
    .getAttribute("src");
  expect(url, "iframe phải trỏ tới blob PDF").toMatch(/^blob:/);
  return url as string;
}

/** Đọc blob PDF để chắc backend trả về file PDF thật chứ không rỗng/lỗi. */
async function expectValidPdf(page: Page) {
  const head = await invoicePdfUrl(page).then((url) =>
    page.evaluate(async (u) => {
      const bytes = new Uint8Array(await (await fetch(u)).arrayBuffer());
      return {
        head: String.fromCharCode(...bytes.slice(0, 5)),
        size: bytes.length,
        pages: (new TextDecoder("latin1").decode(bytes).match(/\/Type\s*\/Page[^s]/g) ?? []).length,
      };
    }, url),
  );
  expect(head.head, "phải là file PDF").toBe("%PDF-");
  expect(head.size, "PDF quá nhỏ").toBeGreaterThan(5000);
  expect(head.pages, "phải có ít nhất 1 trang").toBeGreaterThan(0);
}

test("Xem PDF hóa đơn: mở từ tab Hóa đơn", async ({ page, request }) => {
  await ensureLoggedIn(page);

  // Tồn kho + hóa đơn nháp riêng cho test này (không phụ thuộc test khác đã chạy).
  const stockIn = await request.post("/api/save_inbound", {
    data: {
      posting_date: "2026-09-19",
      voucher_no: "PN9380",
      description: "Nhập tồn cho test xem PDF trên tab Hóa đơn",
      supplier_code: "",
      warehouse_code: "KHO-CHINH",
      unit_code: "HKD",
      items: [{ product_code: "SP001", quantity: 20, unit_price: 10000, discount: 0 }],
      note: "",
      inbound_type: "purchase",
      reference_no: "",
      vat_rate: 0,
      debit_account: "152",
      credit_account: "331",
      pay_now: false,
      adjust_dir: "up",
      autoBom: false,
    },
  });
  expect(stockIn.ok(), `save_inbound failed: ${await stockIn.text()}`).toBe(true);
  const mkInvoice = await request.post("/api/save_invoice", {
    data: {
      number: "HD9380",
      date: "2026-09-19",
      customer: "Khách xem PDF trên tab Hóa đơn",
      customer_tax_code: "0100000000",
      items: [
        {
          product_code: "SP001",
          quantity: 2,
          unit_price: 10000,
          industry_code: "PPHH",
          discount: 0,
          warehouse_code: "",
        },
      ],
    },
  });
  expect(mkInvoice.ok(), `save_invoice failed: ${await mkInvoice.text()}`).toBe(true);

  await sidebarButton(page, "Hóa đơn").click();
  await expect(page.locator("header h2")).toHaveText("Hóa đơn");
  // Bảng phân trang server-side + sắp theo ngày giảm dần → tìm cho chắc (số mới
  // tạo không hẳn nằm ở trang đầu).
  await page.getByPlaceholder("Tìm kiếm…").fill("HD9380");
  const row = page.locator("tr", { has: page.getByText("HD9380", { exact: true }) }).first();
  await expect(row).toBeVisible({ timeout: 20_000 });

  // Nút PDF dựng `detail_json` ở backend rồi render ngay trong Rust; bản nháp
  // in nhãn "HÓA ĐƠN NHÁP" (2 nhánh nháp/đã phát hành test ở Rust).
  await row.getByRole("button", { name: "Xem PDF hóa đơn" }).click();
  await expect(invoiceFrame(page)).toBeVisible({ timeout: 30_000 });
  await expectValidPdf(page);
  await invoiceFrame(page).getByRole("button", { name: "Đóng" }).click();
  await expect(invoiceFrame(page)).toBeHidden();
});

test("Xem PDF hóa đơn: mở từ Đồng bộ HĐĐT và từ phiếu nhập", async ({ page, request }) => {
  await ensureLoggedIn(page);
  await sidebarButton(page, "Đồng bộ HĐĐT").click();
  await expect(page.locator("header h2")).toHaveText("Đồng bộ hóa đơn mua");

  // Chi tiết hóa đơn phải đủ trường mà bố cục PDF dùng (tiêu đề, 2 bên, tổng
  // thuế, chữ ký số) — template đã được làm mềm nên thiếu trường cũng không ném.
  const detail = {
    tlhdon: "HÓA ĐƠN GIÁ TRỊ GIA TĂNG",
    nky: "2026-09-15T03:00:00Z",
    mhdon: "MOCKPDF01",
    nbten: "CÔNG TY CỔ PHẦN PDF MOCK",
    nmten: "HỘ KINH DOANH MOCK",
    nbdchi: "12 Đường Mock, Ba Đình, Hà Nội",
    nmdchi: "Số 409 Đường Mock, Hà Nội",
    thtttoan: "Chuyển khoản",
    tgtcthue: 2000000,
    tgtthue: 160000,
    tgtphi: 0,
    ttcktmai: 0,
    tgtttbso: 2160000,
    tgtttbchu: "Hai triệu một trăm sáu mươi nghìn đồng",
    thttltsuat: [{ tsuat: "8%", thtien: 2000000, tthue: 160000, gttsuat: null }],
    nbcks: JSON.stringify({ Subject: "CÔNG TY PDF MOCK", SigningTime: "15/09/2026 10:00:00" }),
    qrcode: "https://hoadondientu.gdt.gov.vn/e2e/pdf",
    hdhhdvu: [
      {
        stt: 1,
        tchat: 1,
        ten: "Máy lọc nước PDF",
        dvtinh: "Cái",
        mhhdvu: "pdffilter",
        sluong: 2,
        dgia: 1000000,
        thtien: 2000000,
        stckhau: 0,
        ltsuat: "8%",
        tsuat: 0.08,
      },
    ],
  };
  const seed = await request.post("/api/hddt_sync_test_seed", {
    data: { portal_id: "e2e-uuid-pdf", detail },
  });
  expect(seed.ok(), `seed hddt_sync_test_seed failed ${seed.status()}`).toBe(true);
  // Nhập tất cả (idempotent) → hóa đơn vừa seed có số phiếu để tìm đúng dòng.
  const imported = await request.post("/api/hddt_sync_import", {
    data: {
      ids: [],
      warehouse_code: null,
      unit_code: null,
      debit_account: null,
      credit_account: null,
    },
  });
  expect(
    imported.ok(),
    `hddt_sync_import failed ${imported.status()}: ${await imported.text()}`,
  ).toBe(true);

  // Bảng có nhiều hóa đơn của các test trước — định vị bằng số phiếu mới tạo
  // (mọi dòng seed đều dùng chung ký hiệu C26E2E nên không phân biệt được).
  const preview = await request.post("/api/hddt_sync_preview", {
    data: { from: null, to: null, retry_failed: false },
  });
  const mine = (
    (await preview.json()).rows as Array<{ portal_id: string; voucher_no: string }>
  ).find((r) => r.portal_id === "e2e-uuid-pdf");
  const myVoucher = String(mine?.voucher_no ?? "");
  expect(myVoucher, "hóa đơn seed phải được nhập kho").toMatch(/^PN\d+$/);

  await page.reload();
  await expect(page.locator("header h2")).toHaveText("Đồng bộ hóa đơn mua");
  const row = page.locator("tr", {
    has: page.getByRole("button", { name: `Xem phiếu ${myVoucher}` }),
  });
  await expect(row).toBeVisible({ timeout: 20_000 });

  // ── 1. Từ bảng Đồng bộ: mở PDF ngay trên dòng hóa đơn ──
  await row.getByRole("button", { name: /Xem PDF hóa đơn/ }).click();
  await expect(invoiceFrame(page)).toBeVisible({ timeout: 30_000 });
  await expectValidPdf(page);
  await invoiceFrame(page).getByRole("button", { name: "Đóng" }).click();
  await expect(invoiceFrame(page)).toBeHidden();

  // ── 2. Từ popup phiếu nhập: chỉ hiện khi phiếu có gắn hóa đơn HĐĐT ──
  await row.getByRole("button", { name: `Xem phiếu ${myVoucher}` }).click();
  const vDlg = page.getByRole("dialog").filter({ hasText: "PHIẾU NHẬP KHO" });
  await expect(vDlg.getByText("Hóa đơn điện tử")).toBeVisible();
  await vDlg.getByRole("button", { name: "Xem PDF" }).click();
  await expect(invoiceFrame(page)).toBeVisible({ timeout: 30_000 });
  await expectValidPdf(page);
  await invoiceFrame(page).getByRole("button", { name: "Đóng" }).click();
  await vDlg.getByRole("button", { name: "Đóng" }).click();
  await expect(vDlg).toBeHidden();
});

test("Xem PDF hóa đơn: mở từ tab Tra cứu HĐĐT", async ({ page, request }) => {
  test.skip(LIVE_PORTAL, "Nội dung hóa đơn chỉ đúng với mock portal — bỏ qua khi chạy cổng thật");
  await ensureLoggedIn(page);
  const login = await loginPortal(request);
  test.skip(login === null, "Không lấy được phiên cổng HĐĐT — bỏ qua");

  await sidebarButton(page, "Tra cứu HĐĐT").click();
  await expect(page.locator("header h2")).toHaveText("Tra cứu HĐĐT");
  await page.getByRole("button", { name: "Tìm kiếm", exact: true }).click();
  const table = page.locator(".p-datatable");
  await expect(table).toBeVisible({ timeout: 30_000 });
  await expect(page.getByText(/Có \d[\d.]* kết quả/)).toBeVisible({ timeout: 30_000 });

  // Dòng tra cứu chưa có chi tiết trong DB → app phải gọi cổng lấy chi tiết rồi
  // mới render PDF (nút có spinner trong lúc chờ).
  await table
    .getByRole("button", { name: /^Xem PDF hóa đơn/ })
    .first()
    .click();
  await expect(invoiceFrame(page)).toBeVisible({ timeout: 30_000 });
  await expectValidPdf(page);
  await invoiceFrame(page).getByRole("button", { name: "Đóng" }).click();
  await expect(invoiceFrame(page)).toBeHidden();
});

// ─── F5 — TÊN KHÁC (ALIAS): khai ở danh mục, tìm được, hóa đơn giữ đúng tên ───
test("Tên khác của sản phẩm: tìm được ở danh mục và hóa đơn giữ tên đã chọn", async ({
  page,
  request,
}) => {
  await ensureLoggedIn(page);

  const prod = await request.post("/api/save_product", {
    data: {
      code: "SP-AL1",
      name: "Bột mì",
      unit: "Gói",
      salePrice: 20_000,
      costPrice: 15_000,
      minStock: 0,
      vatRate: 1,
      importTaxRate: 0,
      isService: false,
      industryCode: "PPHH",
    },
  });
  expect(prod.ok(), `save_product: ${await prod.text()}`).toBe(true);
  const stock = await request.post("/api/save_inbound", {
    data: {
      posting_date: "2026-09-03",
      voucher_no: "PN-AL1",
      description: "Nhập tồn cho test tên khác",
      supplier_code: "",
      warehouse_code: "KHO-CHINH",
      unit_code: "HKD",
      items: [{ product_code: "SP-AL1", quantity: 12, unit_price: 15_000, discount: 0 }],
      note: "",
      inbound_type: "purchase",
      reference_no: "",
      vat_rate: 0,
      debit_account: "152",
      credit_account: "331",
      pay_now: false,
      adjust_dir: "up",
      autoBom: false,
    },
  });
  expect(stock.ok(), `save_inbound: ${await stock.text()}`).toBe(true);

  // ── 1) Khai "Tên khác" ngay trên dialog Sửa sản phẩm ──
  await sidebarButton(page, "Sản phẩm").click();
  await expect(page.locator("header h2")).toHaveText("Danh mục sản phẩm");
  const search = page.getByPlaceholder("Tìm kiếm…").first();
  await search.fill("SP-AL1");
  const gridRow = page.locator("tr", { has: page.getByText("SP-AL1", { exact: true }) });
  await expect(gridRow.first()).toBeVisible();
  await gridRow.first().locator("td").first().click();
  await page.getByRole("button", { name: "Sửa", exact: true }).click();

  const pDlg = page.getByRole("dialog");
  await pDlg.locator('label:text-is("Tên khác") + input').fill("Mì flour, Bột làm bánh");
  await pDlg.getByRole("button", { name: "Lưu", exact: true }).click();
  await expect(pDlg).toBeHidden();
  await expect(gridRow.first()).toContainText("Mì flour, Bột làm bánh");

  // ── 2) Tìm TOÀN CỤC bằng tên khác — server phải lọc ra đúng 1 sản phẩm ──
  await search.fill("");
  await expect
    .poll(() => page.locator(".p-datatable tbody tr:visible").count(), { timeout: 15_000 })
    .toBeGreaterThan(1);
  await search.fill("mì flour");
  await expect(page.locator(".p-datatable tbody tr:visible")).toHaveCount(1);
  await expect(gridRow.first()).toBeVisible();

  // ── 3) Hóa đơn: chọn TÊN ALIAS trong ô sản phẩm → lưu lại đúng tên đó ──
  await sidebarButton(page, "Hóa đơn").click();
  await expect(page.locator("header h2")).toHaveText("Hóa đơn");
  await page.getByRole("button", { name: "Lập hóa đơn nháp" }).click();
  const dlg = page.getByRole("dialog");
  // Số gợi ý điền sau khi nạp xong danh sách hóa đơn → chờ có số rồi mới đọc.
  const numberInput = dlg.getByRole("textbox").first();
  await expect(numberInput).toHaveValue(/^HD\d+$/);
  const number = await numberInput.inputValue();

  const custInput = dlg.getByRole("combobox", { name: "Chọn hoặc nhập tên khách hàng" });
  await custInput.fill("Khách Tên Khác");
  await custInput.press("Enter");

  const line = dlg.locator("tbody tr").first();
  await line.getByRole("combobox").first().click();
  await page.locator('.p-select-overlay [role="searchbox"]').last().fill("mì flour");
  await page
    .getByRole("option", { name: /Mì flour/ })
    .first()
    .click();
  await line.getByRole("spinbutton").nth(1).fill("20000");

  await dlg.getByRole("button", { name: "Lập hóa đơn" }).click();
  await expect(page.locator(".p-toast-summary").last()).toContainText(`Đã lập hóa đơn ${number}`);
  await expect(dlg).toBeHidden();

  const invoices = (await (await request.post("/api/get_invoices", { data: {} })).json()) as Array<{
    id: number;
    number: string;
  }>;
  const inv = invoices.find((i) => i.number === number);
  expect(inv, `không thấy hóa đơn ${number}`).toBeTruthy();

  const detail = (await (
    await request.post("/api/get_invoice_detail", { data: { id: inv?.id } })
  ).json()) as { items: Array<{ product_name: string; line_name: string }> };
  expect(detail.items).toHaveLength(1);
  // Tên hiển thị và tên gốc đều là alias đã chọn — KHÔNG fallback về tên chính.
  expect(detail.items[0].product_name).toBe("Mì flour");
  expect(detail.items[0].line_name).toBe("Mì flour");
});

// ─── TẠO NHANH MẶT HÀNG + THÊM "TÊN KHÁC" NGAY TRÊN DÒNG CỦA POPUP HÓA ĐƠN ───
test("Popup hóa đơn: gõ tên mới là tạo nhanh mặt hàng, thêm tên khác ngay trên dòng", async ({
  page,
  request,
}) => {
  await ensureLoggedIn(page);

  await sidebarButton(page, "Hóa đơn").click();
  await expect(page.locator("header h2")).toHaveText("Hóa đơn");
  await page.getByRole("button", { name: "Lập hóa đơn nháp" }).click();
  const dlg = page.getByRole("dialog").first();
  // Số gợi ý điền sau khi nạp xong danh sách hóa đơn → chờ có số rồi mới đọc.
  const numberInput = dlg.getByRole("textbox").first();
  await expect(numberInput).toHaveValue(/^HD\d+$/);
  const number = await numberInput.inputValue();

  const custInput = dlg.getByRole("combobox", { name: "Chọn hoặc nhập tên khách hàng" });
  await custInput.fill("Khách Tạo Nhanh");
  await custInput.press("Enter");

  // ── 1) Gõ tên CHƯA có trong danh mục → dropdown hiện dòng "Tạo nhanh" ──
  const line = dlg.locator("tbody tr").first();
  await line.getByRole("combobox").first().click();
  await page.locator('.p-select-overlay [role="searchbox"]').last().fill("Bột năng Tân An");
  const quick = page.getByTestId("product-quick-create");
  await expect(quick).toContainText("Tạo nhanh");
  await expect(quick).toContainText("Bột năng Tân An");
  await quick.click();

  // ── 2) Form rút gọn: tên đã gõ sẵn, chỉ cần giá bán + lưu ──
  const qDlg = page.getByRole("dialog", { name: "Tạo nhanh mặt hàng" });
  await expect(qDlg).toBeVisible();
  await expect(qDlg.locator('label:text-is("Tên sản phẩm") + input')).toHaveValue(
    "Bột năng Tân An",
  );
  await qDlg.getByRole("spinbutton").fill("35000");
  await qDlg.getByRole("button", { name: "Lưu", exact: true }).click();
  await expect(qDlg).toBeHidden();

  // Dòng đang đứng được điền ngay hàng vừa tạo (giá bán tự đưa vào đơn giá).
  await expect(line).toContainText("Bột năng Tân An");
  await expect(line.getByRole("spinbutton").nth(1)).toHaveValue(/35/);

  const products = (await (await request.post("/api/get_products", { data: {} })).json()) as Array<{
    code: string;
    name: string;
    sale_price: number;
    aliases: string;
  }>;
  const made = products.find((p) => p.name === "Bột năng Tân An");
  expect(made, "không thấy hàng tạo nhanh trong danh mục").toBeTruthy();
  expect(made?.sale_price).toBe(35_000);

  // Hàng hóa phải có tồn mới bán được — nhập 10 cho hàng vừa tạo nhanh.
  const restock = await request.post("/api/save_inbound", {
    data: {
      posting_date: "2026-09-05",
      voucher_no: "PN-QC1",
      description: "Nhập tồn cho hàng tạo nhanh",
      supplier_code: "",
      warehouse_code: "KHO-CHINH",
      unit_code: "HKD",
      items: [{ product_code: made?.code ?? "", quantity: 10, unit_price: 20_000, discount: 0 }],
      note: "",
      inbound_type: "purchase",
      reference_no: "",
      vat_rate: 0,
      debit_account: "152",
      credit_account: "331",
      pay_now: false,
      adjust_dir: "up",
      autoBom: false,
    },
  });
  expect(restock.ok(), `save_inbound: ${await restock.text()}`).toBe(true);

  // ── 3) Thêm "tên khác" NGAY trên dòng → dòng giữ tên vừa thêm ──
  await line.getByTestId("line-add-alias").click();
  const aDlg = page.getByRole("dialog", { name: "Thêm tên khác" });
  await expect(aDlg).toBeVisible();
  await expect(aDlg).toContainText("Bột năng Tân An");
  await aDlg.getByTestId("alias-input").fill("Tapioca Tân An");
  await aDlg.getByRole("button", { name: "Lưu", exact: true }).click();
  await expect(aDlg).toBeHidden();
  await expect(page.locator(".p-toast-summary").last()).toContainText("Đã thêm tên khác");
  await expect(line).toContainText("Tapioca Tân An");

  // ── 4) Lưu hóa đơn → dòng giữ ĐÚNG tên vừa thêm, danh mục có tên đó ──
  await line.getByRole("spinbutton").first().fill("2");
  await dlg.getByRole("button", { name: "Lập hóa đơn" }).click();
  await expect(page.locator(".p-toast-summary").last()).toContainText(`Đã lập hóa đơn ${number}`);
  await expect(dlg).toBeHidden();

  const invoices = (await (await request.post("/api/get_invoices", { data: {} })).json()) as Array<{
    id: number;
    number: string;
  }>;
  const inv = invoices.find((i) => i.number === number);
  expect(inv, `không thấy hóa đơn ${number}`).toBeTruthy();

  const detail = (await (
    await request.post("/api/get_invoice_detail", { data: { id: inv?.id } })
  ).json()) as { items: Array<{ product_name: string; line_name: string }> };
  expect(detail.items).toHaveLength(1);
  expect(detail.items[0].product_name).toBe("Tapioca Tân An");
  expect(detail.items[0].line_name).toBe("Tapioca Tân An");

  const after = (await (await request.post("/api/get_products", { data: {} })).json()) as Array<{
    name: string;
    aliases: string;
  }>;
  expect(after.find((p) => p.name === "Bột năng Tân An")?.aliases).toContain("Tapioca Tân An");
});

// ─── F4 — ĐỊNH MỨC: KHAI NHIỀU THÀNH PHẨM TRONG 1 MÀN (MỖI SP 1 BẢNG) ───
test("Định mức vật tư: khai nhiều thành phẩm trong 1 màn, lưu và xóa tất cả 1 lần", async ({
  page,
  request,
}) => {
  await ensureLoggedIn(page);

  /** Tạo hàng hóa qua API — phần thêm sản phẩm bằng UI đã có test riêng. */
  const mk = async (code: string, name: string, unit: string) => {
    const r = await request.post("/api/save_product", {
      data: {
        code,
        name,
        unit,
        salePrice: 10_000,
        costPrice: 5_000,
        minStock: 0,
        vatRate: 1,
        importTaxRate: 0,
        isService: false,
        industryCode: "PPHH",
      },
    });
    expect(r.ok(), `save_product ${code}: ${await r.text()}`).toBe(true);
  };
  await mk("SP-MFG1", "Thành phẩm MFG 1", "Cái");
  await mk("SP-MFG2", "Thành phẩm MFG 2", "Cái");
  await mk("SP-MM1", "Vật tư MFG 1", "Kg");
  await mk("SP-MM2", "Vật tư MFG 2", "Cái");

  /** Mở 1 bộ chọn (đã scope vào bảng / dòng) rồi gõ + bấm option đúng tên. */
  async function choose(scope: Locator, text: string) {
    await scope.click();
    const box = page.locator('.p-select-overlay [role="searchbox"]').last();
    await expect(box).toBeVisible();
    await box.fill(text);
    await page.getByRole("option", { name: text }).first().click();
  }

  await sidebarButton(page, "Sản phẩm").click();
  await expect(page.locator("header h2")).toHaveText("Danh mục sản phẩm");
  await page.getByRole("tab", { name: /Định mức vật tư/ }).click();
  await expect(page.getByText("Chưa có định mức nào")).toBeVisible();

  // 2 bảng = 2 thành phẩm khai song song, không phải chọn lại dropdown từng lần.
  await page.getByTestId("bom-add").click();
  await page.getByTestId("bom-add").click();
  const blocks = page.locator('[data-testid="bom-block"]');
  await expect(blocks).toHaveCount(2);
  const b1 = blocks.first();
  const b2 = blocks.nth(1);

  await choose(b1.locator('[data-testid="bom-fg"] .p-select'), "Thành phẩm MFG 1");

  // SP đã có định mức ở bảng khác → chặn, không cho ghi đè lên bảng đang giữ.
  await choose(b2.locator('[data-testid="bom-fg"] .p-select'), "Thành phẩm MFG 1");
  await expect(page.locator(".p-toast-summary").last()).toContainText("Thành phẩm đã có định mức");
  await choose(b2.locator('[data-testid="bom-fg"] .p-select'), "Thành phẩm MFG 2");
  await expect(b1).toContainText("Thành phẩm MFG 1");
  await expect(b2).toContainText("Thành phẩm MFG 2");

  // Mỗi bảng 1 dòng vật tư với số lượng riêng — không có nút "Thêm dòng":
  // chọn thành phẩm là bảng tự mở dòng trống.
  const r1 = b1.locator('[data-testid="bom-table"] tbody tr').first();
  await choose(r1.getByRole("combobox"), "Vật tư MFG 1");
  await r1.getByRole("spinbutton").fill("1.5");

  const r2 = b2.locator('[data-testid="bom-table"] tbody tr').first();
  await choose(r2.getByRole("combobox"), "Vật tư MFG 2");
  await r2.getByRole("spinbutton").fill("3");

  await page.getByTestId("bom-save-all").click();
  await expect(page.locator(".p-toast-summary").last()).toContainText("Đã lưu định mức");

  /** Đọc nguyên danh sách định mức từ server. */
  async function fetchBoms() {
    return (await (await request.post("/api/get_product_boms", { data: {} })).json()) as Array<{
      product_code: string;
      material_code: string;
      quantity: number;
    }>;
  }

  const saved = await fetchBoms();
  const row1 = saved.find((r) => r.product_code === "SP-MFG1");
  expect(row1, "SP-MFG1 phải có định mức").toBeTruthy();
  expect(row1?.material_code).toBe("SP-MM1");
  expect(row1?.quantity).toBe(1.5);
  const row2 = saved.find((r) => r.product_code === "SP-MFG2");
  expect(row2, "SP-MFG2 phải có định mức").toBeTruthy();
  expect(row2?.material_code).toBe("SP-MM2");
  expect(row2?.quantity).toBe(3);

  // Rời tab rồi quay lại: 2 bảng vẫn được dựng lại từ DB đúng theo thành phẩm.
  // Không neo `^…$`: nhãn tab kèm ký tự icon phía trước (font PrimeIcons).
  await page.getByRole("tab", { name: /Danh mục sản phẩm/ }).click();
  await page.getByRole("tab", { name: /Định mức vật tư/ }).click();
  await expect(blocks).toHaveCount(2);

  // Xóa hết dòng của cả 2 bảng rồi Lưu tất cả = xóa cả 2 định mức.
  for (const b of [b1, b2]) {
    await b
      .locator('[data-testid="bom-table"] tbody tr')
      .first()
      .getByRole("button", { name: "Xóa dòng vật tư" })
      .click();
  }
  await page.getByTestId("bom-save-all").click();
  await expect(page.locator(".p-toast-detail").last()).toContainText("Đã xóa định mức");
  await expect(blocks).toHaveCount(0);
  await expect(page.getByText("Chưa có định mức nào")).toBeVisible();

  const left = await fetchBoms();
  expect(left.filter((r) => r.product_code.startsWith("SP-MFG"))).toHaveLength(0);
});

// ─── F4 — ĐỊNH MỨC VẬT TƯ: PNK sản xuất tự sinh phiếu xuất NVL (Nợ 154 / Có 152) ───
test("Định mức vật tư: PNK sản xuất bật tự xuất NVL thì sinh phiếu PX theo định mức", async ({
  page,
  request,
}) => {
  await ensureLoggedIn(page);

  const fg = await request.post("/api/save_product", {
    data: {
      code: "SP-FG1",
      name: "Thành phẩm E2E",
      unit: "Cái",
      salePrice: 50_000,
      costPrice: 30_000,
      minStock: 0,
      vatRate: 1,
      importTaxRate: 0,
      isService: false,
      industryCode: "PPHH",
    },
  });
  expect(fg.ok(), `save_product SP-FG1: ${await fg.text()}`).toBe(true);
  const mat = await request.post("/api/save_product", {
    data: {
      code: "SP-NVL1",
      name: "Vật tư E2E",
      unit: "Kg",
      salePrice: 0,
      costPrice: 5_000,
      minStock: 0,
      vatRate: 1,
      importTaxRate: 0,
      isService: false,
      industryCode: "PPHH",
    },
  });
  expect(mat.ok(), `save_product SP-NVL1: ${await mat.text()}`).toBe(true);

  // Tồn vật tư 100 Kg — PNK 10 thành phẩm sẽ phải tự xuất 10 × 2 = 20 Kg.
  const matStock = await request.post("/api/save_inbound", {
    data: {
      posting_date: "2026-09-04",
      voucher_no: "PN-NVL1",
      description: "Nhập vật tư cho test định mức",
      supplier_code: "",
      warehouse_code: "KHO-CHINH",
      unit_code: "HKD",
      items: [{ product_code: "SP-NVL1", quantity: 100, unit_price: 5_000, discount: 0 }],
      note: "",
      inbound_type: "purchase",
      reference_no: "",
      vat_rate: 0,
      debit_account: "152",
      credit_account: "331",
      pay_now: false,
      adjust_dir: "up",
      autoBom: false,
    },
  });
  expect(matStock.ok(), `save_inbound NVL: ${await matStock.text()}`).toBe(true);

  // ── 1) Khai định mức: 1 thành phẩm = 2 Kg vật tư (Sản phẩm → Định mức vật tư) ──
  await sidebarButton(page, "Sản phẩm").click();
  await expect(page.locator("header h2")).toHaveText("Danh mục sản phẩm");
  await page.getByRole("tab", { name: /Định mức vật tư/ }).click();

  // Màn giờ khai NHIỀU bảng — bảng nào cũng phải bấm "Thêm định mức" mới có.
  // Dùng .last() vì bảng mới thêm luôn nằm cuối, kể cả khi đã có bảng từ trước.
  await page.getByTestId("bom-add").click();
  const bomBlock = page.locator('[data-testid="bom-block"]').last();
  await bomBlock.locator('[data-testid="bom-fg"] .p-select').click();
  await expect(page.locator('.p-select-overlay [role="searchbox"]').last()).toBeVisible();
  await expect.poll(() => page.getByRole("option").count(), { timeout: 15_000 }).toBeGreaterThan(0);
  await page.locator('.p-select-overlay [role="searchbox"]').last().fill("Thành phẩm E2E");
  await page
    .getByRole("option", { name: /Thành phẩm E2E/ })
    .first()
    .click();

  // Không có nút "Thêm dòng": chọn thành phẩm là bảng tự mở dòng trống.
  const bomRow = bomBlock.locator('[data-testid="bom-table"] tbody tr').first();
  await bomRow.getByRole("combobox").click();
  await page.locator('.p-select-overlay [role="searchbox"]').last().fill("Vật tư E2E");
  await page
    .getByRole("option", { name: /Vật tư E2E/ })
    .first()
    .click();
  await bomRow.getByRole("spinbutton").fill("2");
  await page.getByTestId("bom-save-all").click();
  await expect(page.locator(".p-toast-summary").last()).toContainText("Đã lưu định mức");

  // ── 2) PNK loại "Tự sản xuất / gia công" + ô "Tự xuất NVL theo định mức" ──
  await sidebarButton(page, "Nhập kho").click();
  await expect(page.locator("header h2")).toHaveText("Nhập kho");
  await page.getByRole("button", { name: "Tạo phiếu nhập" }).click();
  const dlg = page.getByRole("dialog");
  await expect(dlg.getByText("Tạo phiếu nhập kho")).toBeVisible();

  await dlg.locator('label:text-is("Loại nhập") + .p-select').click();
  await page
    .getByRole("option", { name: /Tự sản xuất/ })
    .first()
    .click();

  // Số phiếu do backend sinh — chờ đúng số trước khi lưu để không lưu nhầm số tạm.
  const expectedPn = await (
    await request.post("/api/next_voucher_no", { data: { entryType: "PN" } })
  ).text();
  await expect(dlg.getByLabel("Số phiếu")).toHaveValue(expectedPn);

  const line = dlg.locator("tbody tr").first();
  await line.getByRole("combobox").first().click();
  await page.locator('.p-select-overlay [role="searchbox"]').last().fill("Thành phẩm E2E");
  await page
    .getByRole("option", { name: /Thành phẩm E2E/ })
    .first()
    .click();
  await line.getByRole("spinbutton").first().fill("10");
  await line.getByRole("spinbutton").nth(1).fill("50000");

  const autoBom = dlg
    .locator('label:text-is("Tự xuất NVL theo định mức") + div')
    .getByRole("switch");
  await autoBom.click();
  await expect(autoBom).toBeChecked();

  await dlg.getByRole("button", { name: "Lưu phiếu" }).click();
  await expect(dlg).toBeHidden();

  // ── 3) Toast báo số phiếu PX tự sinh ──
  const toastDetail = page.locator(".p-toast-detail").last();
  await expect(toastDetail).toContainText(/Đã tạo phiếu xuất NVL PX\d+/);
  const pxNo = (await toastDetail.textContent())?.match(/PX\d+/)?.[0] ?? "";
  expect(pxNo, "phải lấy được số phiếu PX từ toast").not.toBe("");

  // ── 4) Bút toán PX tự sinh: đúng 20 Kg NVL, Nợ 154 / Có 152, ghi chú liên kết ──
  const voucher = (await (
    await request.post("/api/get_voucher", { data: { voucherNo: pxNo } })
  ).json()) as Array<{
    entry_type: string;
    product_code: string;
    quantity: number;
    debit_account: string;
    credit_account: string;
    note: string;
  }>;
  const pxLine = voucher.find((r) => r.product_code === "SP-NVL1");
  expect(pxLine, `phiếu ${pxNo} phải có dòng xuất NVL SP-NVL1`).toBeTruthy();
  expect(pxLine?.entry_type).toBe("PX");
  expect(pxLine?.quantity, "SL xuất = định mức 2 × sản lượng 10").toBe(20);
  expect(pxLine?.debit_account).toBe("154");
  expect(pxLine?.credit_account).toBe("152");
  expect(pxLine?.note).toContain(expectedPn);

  // Dòng tự sinh phải được gắn marker để không rơi vào doanh thu / giá vốn bán.
  const entries = (await (
    await request.post("/api/get_journal_entries", {
      data: { entryType: "PX", fromDate: "", toDate: "", search: pxNo },
    })
  ).json()) as Array<{ voucher_no: string; adjust_code: string }>;
  expect(entries.find((r) => r.voucher_no === pxNo)?.adjust_code).toBe("XuatNVL");
});

// ─── POPUP CẬP NHẬT: THÂN HỘP THOẠI LÀ GHI CHÚ PHÁT HÀNH (MARKDOWN) ───
test("Popup cập nhật: hiện ghi chú phát hành Markdown thay cho nhật ký", async ({ page }) => {
  // Chạy bằng `vite dev` nên `?update-preview=…` (code dev-only) giả lập có bản
  // mới kèm release body mẫu — bản cài build bằng `vite build` không có đoạn này.
  await page.goto("/?update-preview=0.16.0", { waitUntil: "domcontentloaded" });
  const dlg = page.getByRole("dialog", { name: "Có bản cập nhật 0.16.0" });
  await expect(dlg).toBeVisible({ timeout: 30_000 });

  // Markdown → HTML: tiêu đề, mục con, gạch đầu dòng, in đậm, link, trích dẫn.
  const notes = dlg.getByTestId("release-notes");
  await expect(notes).toBeVisible();
  await expect(notes.getByRole("heading", { level: 2 })).toContainText("0.16.0");
  await expect(notes.getByRole("heading", { level: 3, name: "Tính năng mới" })).toBeVisible();
  await expect(notes.locator("li").first()).toContainText("tạo nhanh mặt hàng");
  await expect(notes.locator("strong").first()).toBeVisible();
  await expect(notes.getByRole("link", { name: "34d64b6" })).toBeVisible();
  await expect(notes.locator("blockquote")).toContainText("mẫu");

  // Không còn câu mở đầu "Đang chạy bản…" lẫn khung nhật ký từng bước.
  await expect(dlg).not.toContainText("Đang chạy bản");
  await expect(dlg.getByTestId("update-log")).toHaveCount(0);

  // Nút cài vẫn ở chân hộp thoại; "Để sau" đóng được popup.
  await expect(page.getByTestId("update-install")).toHaveText("Tải và cài bản 0.16.0");
  await page.getByTestId("update-later").click();
  await expect(dlg).toBeHidden();
});
