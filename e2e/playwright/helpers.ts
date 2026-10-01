import { request } from "@playwright/test";

export const ADMIN = { username: "admin", password: "admin123" };

/**
 * Prepare data for one E2E run. A fresh DB only contains: the admin user (seeded in
 * code) plus 17 chart-of-accounts rows and 1 default warehouse (seeded by migrations).
 * Missing pieces added here:
 * 1. Business info (name + tax code) — otherwise, right after a UI login the app would
 *    force the "update business info" dialog (App.vue forceBusinessConfig) and block.
 * 2. One sample product — so the Products tab renders real data (not an empty table).
 * 3. Logout — tests must log in from the login screen themselves.
 */
export async function seedApp(baseURL: string) {
  const rq = await request.newContext({ baseURL });
  try {
    const login = await rq.post("/api/login", { data: ADMIN });
    if (!login.ok()) {
      throw new Error(`seed: login API failed ${login.status()}`);
    }

    const biz = await rq.post("/api/save_business_config", {
      data: {
        name: "E2E TEST BUSINESS",
        taxCode: "010000000000",
        address: "123 Lang Road, Hanoi",
        // Địa điểm kinh doanh ghi trên đầu sổ TT 152 — gửi kèm luôn để test
        // đầu sổ không in ra dòng trống.
        location: "E2E Market Stall",
        shortName: "E2E TEST",
        ownership: "Household business",
        province: "Hanoi",
        taxCodeIssuedOn: "2024-01-01",
        phone: "0123456789",
        email: "e2e@test.local",
        // Bắt buộc — mốc lấp hóa đơn mua vào từ cổng HĐĐT.
        hddtStartDate: "2026-01-01",
        // Tuỳ chọn — ký hiệu HĐĐT của hộ (mặc định khi liên kết HĐĐT).
        hddtSymbol: "1C26TT152",
        // Nhóm hộ: null = để app tự xếp theo doanh thu. Phải gửi kèm — server
        // bắt buộc đủ tham số cho mọi trường của lệnh này.
        taxGroup: null,
      },
    });
    if (!biz.ok()) {
      throw new Error(`seed: save_business_config failed ${biz.status()}`);
    }

    const prod = await rq.post("/api/save_product", {
      data: {
        code: "SP001",
        name: "Bottled water",
        unit: "Bottle",
        salePrice: 15000,
        costPrice: 10000,
        minStock: 20,
        vatRate: 8,
        importTaxRate: 0,
        isService: false,
        industryCode: "",
      },
    });
    if (!prod.ok()) {
      throw new Error(`seed: save_product failed ${prod.status()}`);
    }

    await rq.post("/api/logout");
  } finally {
    await rq.dispose();
  }
}
