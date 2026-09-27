import { expect, test } from "@playwright/test";
import type { APIRequestContext, Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
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
    ["Người dùng", "Người dùng & phân quyền", "/users"],
    ["Hồ sơ HKD", "Hồ sơ HKD", "/profiles"],
    ["Cài đặt", "Cài đặt", "/settings"],
  ];
  for (const [tab, title, path] of cases) {
    // Sidebar buttons carry an icon in their accessible name (e.g. " Tổng quan") → match substring.
    await sidebarButton(page, tab).click();
    // Chờ URL đổi TRƯỚC: đây là tín hiệu router đã điều hướng xong. Bấm tab kế
    // tiếp khi còn đang chuyển trang thì lần điều hướng mới bị bỏ qua — đó là
    // nguyên nhân test này fail ngẫu nhiên trên runner CI chậm.
    await expect(page).toHaveURL(new RegExp(`${path}$`));
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
    },
  });
  expect(stockIn.ok(), `save_inbound failed ${stockIn.status()}: ${await stockIn.text()}`).toBe(
    true,
  );

  await page.getByRole("button", { name: "Lập hóa đơn nháp" }).click();
  const dlg = page.getByRole("dialog");
  const number = await dlg.getByRole("textbox").first().inputValue();
  expect(number).toMatch(/^HD\d+$/);

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
