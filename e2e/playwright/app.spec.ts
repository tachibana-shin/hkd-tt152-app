import { expect, test } from "@playwright/test";
import type { APIRequestContext, Page } from "@playwright/test";
import { readFileSync } from "node:fs";
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
  request,
}) => {
  // Phiên đăng nhập của app là trạng thái chung trong tiến trình server, không
  // gắn cookie theo browser context → nếu instance cũ còn sống (hoặc test trước
  // đã login) thì trang mở ra là Dashboard. Đảm bảo đã đăng xuất trước.
  await request.post("/api/logout", { data: {} });
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
    ["Chờ xuất HĐĐT", "Chờ xuất HĐĐT"],
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
  const dlg = page.getByRole("dialog");
  await dlg.getByLabel("Số HĐĐT").fill("00009400");
  await dlg.getByLabel("Ký hiệu HĐĐT").fill("1C26TT152");
  // DatePicker: gõ ngày rồi Enter để chốt giá trị vào model (Escape sẽ hoàn tác).
  await dlg.getByLabel("Ngày HĐĐT").fill("30/09/2026");
  await dlg.getByLabel("Ngày HĐĐT").press("Enter");
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

test("Xuất hóa đơn sang dịch vụ khác: chép dữ liệu + lưu bản chốt + ghi nhận hủy", async ({
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
      items: [{ product_code: "SP001", quantity: 10, unit_price: 10000, discount: 0 }],
      note: "",
      inbound_type: "purchase",
      reference_no: "",
      vat_rate: 0,
      debit_account: "152",
      credit_account: "331",
      pay_now: false,
      adjust_dir: "up",
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

  // 5) Hủy HĐ đã phát hành: bắt buộc có phiếu xuất điều chỉnh.
  await row.getByRole("button", { name: "Ghi nhận HĐ đã hủy bên kia" }).click();
  const cancelDlg = page.getByRole("dialog");
  await expect(cancelDlg.getByText("đã phát hành", { exact: false }).first()).toBeVisible();
  await cancelDlg.getByLabel("Lý do hủy").fill("Khách trả lại hàng");
  await cancelDlg.getByRole("button", { name: "Ghi nhận" }).click();
  await expect(page.locator(".p-toast-summary").last()).toContainText("Cần phiếu điều chỉnh", {
    timeout: 10_000,
  });
  await cancelDlg.getByRole("button", { name: "Đóng" }).click();

  // Có số phiếu điều chỉnh → hủa được, trạng thái thành "Đã hủy" + ghi lịch sử.
  await row.getByRole("button", { name: "Ghi nhận HĐ đã hủy bên kia" }).click();
  await cancelDlg.getByLabel("Lý do hủy").fill("Khách trả lại hàng");
  await cancelDlg.getByLabel("Số phiếu xuất điều chỉnh").fill("PX9001");
  await cancelDlg.getByRole("button", { name: "Ghi nhận" }).click();
  await expect(row.getByText("Đã hủy", { exact: true })).toBeVisible();

  const events = await (
    await request.post("/api/invoice_events", { data: { invoiceId: (inv as { id: number }).id } })
  ).json();
  const steps = (events as Array<{ to_status: string }>).map((e) => e.to_status);
  expect(steps).toContain("exported");
  expect(steps).toContain("cancelled");
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
