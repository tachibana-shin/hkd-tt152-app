/** Bố cục cột + dấu tách cột của khối dán sang máy tính tiền bên kia. */
export interface PasteFormat {
  pasteLayout: "full" | "no_discount" | "no_unit" | "amount_only";
  pasteSep: string;
}

// Typed Tauri invoke wrappers — single entry point cho mọi backend call.
import { invoke } from "@tauri-apps/api/core";

// Chạy trong app Tauri hay trong trình duyệt thường (Chrome)?
// - App desktop: dùng IPC invoke như cũ.
// - Trình duyệt: production app tự chạy web server local (127.0.0.1) — gọi REST API
//   /api/<cmd> cùng tham số JSON (camelCase), kết quả/`Err` giống hệt invoke.
const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export { inTauri };

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (inTauri) {
    return invoke<T>(cmd, args);
  }
  let res: Response;
  try {
    res = await fetch(`/api/${cmd}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(args ?? {}),
    });
  } catch (e) {
    throw new Error(`Không kết nối được app (${String(e)})`);
  }
  const text = await res.text();
  if (!res.ok) {
    let msg = text;
    try {
      msg = (JSON.parse(text) as { error?: string }).error ?? text;
    } catch {
      /* giữ nguyên text */
    }
    throw new Error(msg);
  }
  // Trả nguyên chuỗi text — khớp hợp đồng của invoke (Tauri trả String) để caller dùng
  // `parse()` JSON.parse đúng MỘT lần. Nếu parse ở đây sẽ thành "double-parse" (JSON.parse
  // trên object → `"[object Object]" is not valid JSON`) làm mọi API qua `parse()` vỡ.
  return text as T;
}
import type {
  BusinessConfig,
  AppSettings,
  Account,
  Product,
  IndustryGroup,
  Warehouse,
  Supplier,
  Customer,
  TaxSummaryRow,
  RevenueExpenseRow,
  TrialBalanceRow,
  InventoryRow,
  StockLot,
  JournalEntryRow,
  LedgerRow,
  Invoice,
  InvoiceItem,
  InventoryCount,
  CountItemRow,
  Employee,
  PayrollRow,
  AuditEntry,
  ImportResult,
  AttendanceRow,
  AttendanceWorkDay,
  TaxDeclarationRow,
  TaxOverview,
  VoucherRow,
  CurrentUser,
  AppUser,
  Role,
  Profile,
  ProfilePrefs,
  TaxInfo,
  LookupOutcome,
  HddtConfig,
  HddtStatus,
  HddtLoginResult,
  HddtInvoiceList,
  HddtInvoiceFilter,
  HddtSyncScanSummary,
  HddtSyncPreview,
  HddtSyncImport,
  HddtSyncClearCache,
  InvoiceEvent,
  InvoiceExportPack,
} from "@/types";

// ─── parse helper: backend trả JSON string ───
const parse = <T>(s: string): T => JSON.parse(s) as T;

// Cấu hình mặc định nếu thiếu key trong app_setting.
const SETTING_DEFAULTS: AppSettings = {
  product_code_prefix: "SP",
  product_code_start: 1,
  product_code_digits: 4,
  product_vat_rate_options: [5, 8, 10],
  product_vat_rate_default: 10,
  product_import_tax_options: [0, 5, 8, 10, 15, 20, 25, 30],
  product_import_tax_default: 0,
  product_unit: "Cái",
  product_min_stock: 0,
  tax_period: "quarter", // Kỳ khai thuế: year | quarter | month | per_occurrence
  tax_method: "revenue", // Phương pháp TNCN: revenue (theo doanh thu) | profit (theo lợi nhuận)
  vat_deduct: false, // Khấu trừ GTGT đầu vào — mặc định TẮT (theo doanh thu)
};

// Backend lưu mọi thứ dưới dạng chuỗi → chuẩn hóa về kiểu AppSettings.
function normalizeAppSettings(raw: Record<string, string>): AppSettings {
  const num = (k: keyof AppSettings, d: number): number => {
    const n = Number(raw[k]);
    return Number.isFinite(n) ? n : d;
  };
  const list = (k: keyof AppSettings, d: number[]): number[] => {
    try {
      const a: unknown = JSON.parse(raw[k] ?? "[]");
      return Array.isArray(a) ? a.filter((x): x is number => typeof x === "number") : d;
    } catch {
      return d;
    }
  };
  return {
    product_code_prefix: raw.product_code_prefix ?? SETTING_DEFAULTS.product_code_prefix,
    product_code_start: num("product_code_start", SETTING_DEFAULTS.product_code_start),
    product_code_digits: num("product_code_digits", SETTING_DEFAULTS.product_code_digits),
    product_vat_rate_options: list(
      "product_vat_rate_options",
      SETTING_DEFAULTS.product_vat_rate_options,
    ),
    product_vat_rate_default: num(
      "product_vat_rate_default",
      SETTING_DEFAULTS.product_vat_rate_default,
    ),
    product_import_tax_options: list(
      "product_import_tax_options",
      SETTING_DEFAULTS.product_import_tax_options,
    ),
    product_import_tax_default: num(
      "product_import_tax_default",
      SETTING_DEFAULTS.product_import_tax_default,
    ),
    product_unit: raw.product_unit ?? SETTING_DEFAULTS.product_unit,
    product_min_stock: num("product_min_stock", SETTING_DEFAULTS.product_min_stock),
    tax_period: raw.tax_period || SETTING_DEFAULTS.tax_period,
    tax_method: raw.tax_method || SETTING_DEFAULTS.tax_method,
    vat_deduct: raw.vat_deduct === "1",
  };
}

// ─── INFO / CONFIG ───
export const api = {
  getBusinessInfo: () => call<string>("get_business_info"),
  getBusinessConfig: async () => parse<BusinessConfig>(await call<string>("get_business_config")),
  saveBusinessConfig: (cfg: Partial<BusinessConfig>) =>
    call<string>("save_business_config", {
      name: cfg.name ?? "",
      taxCode: cfg.tax_code ?? "",
      address: cfg.address ?? "",
      shortName: cfg.short_name ?? "",
      ownership: cfg.ownership ?? "Tư nhân",
      province: cfg.province ?? "",
      taxCodeIssuedOn: cfg.tax_code_issued_on ?? "",
      phone: cfg.phone ?? "",
      email: cfg.email ?? "",
      hddtStartDate: cfg.hddt_start_date ?? "",
    }),

  // ─── CÀI ĐẶT MẶC ĐỊNH ───
  getAppSettings: async () =>
    normalizeAppSettings(parse<Record<string, string>>(await call<string>("get_app_settings"))),
  saveAppSettings: (settings: Record<string, string>) =>
    call<string>("save_app_settings", { settings }),
  nextProductCode: () => call<string>("next_product_code"),

  // ─── MASTER DATA ───
  getAccounts: async () => parse<Account[]>(await call<string>("get_accounts")),
  saveAccount: (p: Partial<Account>) =>
    call<string>("save_account", {
      code: p.code ?? "",
      name: p.name ?? "",
      openingDebit: p.opening_debit ?? 0,
      openingCredit: p.opening_credit ?? 0,
    }),
  deleteAccount: (id: number) => call<string>("delete_account", { id }),
  getProducts: async () => parse<Product[]>(await call<string>("get_products")),
  /** Lấy 1 trang sản phẩm theo event lazy của PrimeVue DataTable (server-side sort/filter/page). */
  getProductsPage: async (lazyEvent: unknown) =>
    parse<{ rows: Product[]; total: number }>(
      await call<string>("get_products_page", {
        lazyEvent: JSON.stringify(lazyEvent),
      }),
    ),
  saveProduct: (p: Partial<Product>) =>
    call<string>("save_product", {
      code: p.code ?? "",
      name: p.name ?? "",
      unit: p.unit ?? "Cái",
      salePrice: p.sale_price ?? 0,
      costPrice: p.cost_price ?? 0,
      minStock: p.min_stock ?? 0,
      vatRate: p.vat_rate ?? 0.1,
      importTaxRate: p.import_tax_rate ?? 0,
      isService: p.is_service ?? false,
      industryCode: p.industry_code ?? "",
    }),
  deleteProduct: (id: number) => call<string>("delete_product", { id }),
  deleteProducts: (ids: number[]) => call<string>("delete_products", { ids }),
  /**
   * Gán nhóm ngành "Phân phối, cung cấp hàng hóa" cho mọi hàng hóa chưa có
   * nhóm (dịch vụ và nhóm đã gán tay giữ nguyên).
   */
  assignGoodsIndustry: async () =>
    parse<{ updated: number; kept: number; code: string; name: string }>(
      await call<string>("assign_goods_industry", {}),
    ),

  getIndustryGroups: async () => parse<IndustryGroup[]>(await call<string>("get_industry_groups")),
  saveIndustryGroup: (g: { code: string; name: string; vat_rate: number; pit_rate: number }) =>
    call<string>("save_industry_group", {
      code: g.code,
      name: g.name,
      vatRate: g.vat_rate,
      pitRate: g.pit_rate,
    }),
  deleteIndustryGroup: (code: string) => call<string>("delete_industry_group", { code }),
  getWarehouses: async () => parse<Warehouse[]>(await call<string>("get_warehouses")),
  saveWarehouse: (code: string, name: string) => call<string>("save_warehouse", { code, name }),
  nextWarehouseCode: () => call<string>("next_warehouse_code"),
  nextCustomerCode: () => call<string>("next_customer_code"),
  nextSupplierCode: () => call<string>("next_supplier_code"),
  nextEmployeeCode: () => call<string>("next_employee_code"),
  getSuppliers: async () => parse<Supplier[]>(await call<string>("get_suppliers")),
  saveSupplier: (s: Partial<Supplier>) =>
    call<string>("save_supplier", {
      code: s.code ?? "",
      name: s.name ?? "",
      address: s.address ?? "",
      taxCode: s.tax_code ?? "",
      phone: s.phone ?? "",
    }),
  getCustomers: async () => parse<Customer[]>(await call<string>("get_customers")),
  saveCustomer: (c: Partial<Customer>) =>
    call<string>("save_customer", {
      code: c.code ?? "",
      name: c.name ?? "",
      address: c.address ?? "",
      taxCode: c.tax_code ?? "",
      phone: c.phone ?? "",
    }),

  // ─── TRA CỨU MST (masothue.com qua backend) ───
  /** Tra cứu MST → 1 kết quả (single), danh sách (multiple) hoặc không thấy. */
  lookupTaxCode: async (mst: string): Promise<LookupOutcome> =>
    parse<LookupOutcome>(await call<string>("lookup_tax_code", { mst })),
  /** Lấy chi tiết từ URL masothue (dùng khi chọn trong danh sách). */
  lookupTaxDetail: async (url: string): Promise<TaxInfo> =>
    parse<TaxInfo>(await call<string>("lookup_tax_detail", { url })),

  // ─── NHÂN SỰ & BẢNG LƯƠNG ───
  getEmployees: async () => parse<Employee[]>(await call<string>("get_employees")),
  saveEmployee: (e: Partial<Employee>) =>
    call<string>("save_employee", {
      input: {
        code: e.code ?? "",
        name: e.name ?? "",
        department: e.department ?? "",
        position: e.position ?? "",
        job: e.job ?? "",
        basicSalary: e.basic_salary ?? 0,
        allowanceCv: e.allowance_cv ?? 0,
        allowanceXx: e.allowance_xx ?? 0,
        allowancePhone: e.allowance_phone ?? 0,
        bhSalary: e.bh_salary ?? 0,
        hiredOn: e.hired_on ?? "",
        leftOn: e.left_on ?? "",
        dependents: e.dependents ?? 0,
      },
    }),
  getPayrollPeriods: async () => parse<string[]>(await call<string>("get_payroll_periods")),
  getPayroll: async (period: string) =>
    parse<PayrollRow[]>(await call<string>("get_payroll", { period })),
  savePayroll: (
    period: string,
    items: Array<{
      employee_code: string;
      work_days: number;
      bonus: number;
      advance: number;
      pit_amount: number;
      note: string;
    }>,
  ) =>
    call<string>("save_payroll", {
      period,
      items: items.map((i) => ({
        employeeCode: i.employee_code,
        workDays: i.work_days,
        bonus: i.bonus,
        advance: i.advance,
        pitAmount: i.pit_amount,
        note: i.note,
      })),
    }),

  // ─── INBOUND / OUTBOUND ───
  saveInbound: (args: {
    posting_date: string;
    voucher_no: string;
    description: string;
    supplier_code: string;
    warehouse_code: string;
    unit_code: string;
    note: string;
    items: {
      product_code: string;
      quantity: number;
      unit_price: number;
      discount: number; // số tiền CK (đ) — giá trị nhập kho = Thành tiền − Tiền CK
    }[];
    inbound_type: string; // purchase | production | other
    reference_no: string; // số hóa đơn / lệnh nhập kho (cột "Theo..." mẫu 03-VT)
    vat_rate: number; // % GTGT đầu vào (0-1); chỉ khấu trừ khi bật vat_deduct
    debit_account: string;
    credit_account: string;
    /** Trả tiền ngay: tự tạo phiếu chi (PC) thanh toán cho nhà cung cấp. */
    pay_now: boolean;
    /** Hướng điều chỉnh khi loại nhập = "adjust": up = bên bán thêm hàng, down = bên bán trừ bớt (trả lại NCC). */
    adjust_dir: "" | "up" | "down";
  }) =>
    call<string>("save_inbound", {
      postingDate: args.posting_date,
      voucherNo: args.voucher_no,
      description: args.description,
      supplierCode: args.supplier_code,
      warehouseCode: args.warehouse_code,
      unitCode: args.unit_code,
      items: args.items,
      note: args.note,
      inboundType: args.inbound_type,
      referenceNo: args.reference_no,
      vatRate: args.vat_rate,
      debitAccount: args.debit_account,
      creditAccount: args.credit_account,
      payNow: args.pay_now,
      adjustDir: args.adjust_dir,
    }),

  saveOutbound: (args: {
    posting_date: string;
    voucher_no: string;
    description: string;
    customer_code: string;
    unit_code: string;
    note: string;
    items: {
      product_code: string;
      quantity: number;
      unit_price: number;
      industry_code: string;
    }[];
    /** Thu tiền ngay: tự tạo phiếu thu (PT) của khách hàng khi lưu. */
    receive_now: boolean;
    /** Loại xuất: "sale" (bán hàng) | "adjust" (điều chỉnh hóa đơn bán). */
    outbound_type: "" | "sale" | "adjust";
    /** Hướng điều chỉnh khi loại xuất = "adjust": down = khách trả lại/giảm doanh thu, up = tăng. */
    adjust_dir: "" | "up" | "down";
    /** Lập kèm hóa đơn bán hàng (tab "Hóa đơn") — mặc định bật cho phiếu bán. */
    create_invoice?: boolean;
    /** Thông tin hóa đơn lập kèm: số mặc định = số phiếu xuất; khai HĐĐT → hóa đơn chuyển ngay thành "Đã liên kết HĐĐT". */
    invoice?: {
      number: string;
      e_invoice_no: string;
      e_invoice_symbol: string;
      e_invoice_date: string;
    };
  }) =>
    call<string>("save_outbound", {
      postingDate: args.posting_date,
      voucherNo: args.voucher_no,
      description: args.description,
      customerCode: args.customer_code,
      unitCode: args.unit_code,
      items: args.items,
      note: args.note,
      receiveNow: args.receive_now,
      outboundType: args.outbound_type,
      adjustDir: args.adjust_dir,
      createInvoice: args.create_invoice ?? true,
      invoice: {
        number: args.invoice?.number ?? "",
        eInvoiceNo: args.invoice?.e_invoice_no ?? "",
        eInvoiceSymbol: args.invoice?.e_invoice_symbol ?? "",
        eInvoiceDate: args.invoice?.e_invoice_date ?? "",
      },
    }),

  getJournalEntries: (entryType = "", fromDate = "", toDate = "") =>
    call<string>("get_journal_entries", { entryType, fromDate, toDate }).then(parse) as Promise<
      JournalEntryRow[]
    >,

  // ─── CASH (PHIẾU THU / CHI) ───
  saveCashEntry: (input: {
    entry_type: string;
    posting_date: string;
    voucher_no: string;
    description: string;
    customer_code: string;
    supplier_code: string;
    amount: number;
    debit_account: string;
    credit_account: string;
    industry_code: string;
    vat_rate: number;
    pit_rate: number;
    unit_code: string;
    note: string;
  }) =>
    call<string>("save_cash_entry", {
      input: {
        entryType: input.entry_type,
        postingDate: input.posting_date,
        voucherNo: input.voucher_no,
        description: input.description,
        customerCode: input.customer_code,
        supplierCode: input.supplier_code,
        amount: input.amount,
        debitAccount: input.debit_account,
        creditAccount: input.credit_account,
        industryCode: input.industry_code,
        vatRate: input.vat_rate,
        pitRate: input.pit_rate,
        unitCode: input.unit_code,
        note: input.note,
      },
    }),

  // ─── STOCK ───
  getStockLots: async (productCode = "") =>
    parse<StockLot[]>(await call<string>("get_stock_lots", { productCode })),
  getInventorySummary: async () =>
    parse<InventoryRow[]>(await call<string>("get_inventory_summary")),

  // ─── INVOICES ───
  getInvoices: async () => parse<Invoice[]>(await call<string>("get_invoices")),
  deleteInvoice: (id: number) => call<string>("delete_invoice", { id }),
  getInvoiceDetail: async (id: number) =>
    parse<{ invoice: Invoice; items: InvoiceItem[] }>(
      await call<string>("get_invoice_detail", { id }),
    ),
  saveInvoice: (args: {
    number: string;
    date: string;
    customer: string;
    customer_tax_code: string;
    items: {
      product_code: string;
      quantity: number;
      unit_price: number;
      industry_code?: string;
      discount?: number;
      warehouse_code?: string;
    }[];
  }) =>
    call<string>("save_invoice", {
      number: args.number,
      date: args.date,
      customer: args.customer,
      customerTaxCode: args.customer_tax_code,
      items: args.items,
    }),

  // ─── INVENTORY COUNT ───
  getInventoryCounts: async () =>
    parse<InventoryCount[]>(await call<string>("get_inventory_counts")),
  getInventoryCountDetail: async (id: number) =>
    parse<CountItemRow[]>(await call<string>("get_inventory_count_detail", { id })),
  saveInventoryCount: (args: {
    date: string;
    note: string;
    items: { product_code: string; warehouse_code: string; counted_qty: number }[];
  }) =>
    call<string>("save_inventory_count", {
      date: args.date,
      note: args.note,
      items: args.items,
    }),

  // ─── REPORTS ───
  getTaxSummary: async (fromDate: string, toDate: string, unitCode: string) =>
    parse<TaxSummaryRow[]>(await call<string>("get_tax_summary", { fromDate, toDate, unitCode })),
  getRevenueExpense: async (fromDate: string, toDate: string) =>
    parse<RevenueExpenseRow>(await call<string>("get_revenue_expense", { fromDate, toDate })),
  getTrialBalance: async (fromDate: string, toDate: string) =>
    parse<TrialBalanceRow[]>(await call<string>("get_trial_balance", { fromDate, toDate })),

  // ─── SỔ SÁCH ───
  getLedger: async (fromDate: string, toDate: string) =>
    parse<LedgerRow[]>(await call<string>("get_ledger", { fromDate, toDate })),

  // ─── BACKUP / RESTORE ───
  createBackup: () => call<string>("create_backup"),
  listBackups: async () => parse<string[]>(await call<string>("list_backups")),
  restoreBackup: (filename: string) => call<string>("restore_backup", { filename }),

  // ─── Xuất hóa đơn sang dịch vụ HĐĐT khác (chép tay) ───
  invoiceExportPack: async (invoiceId: number, args: PasteFormat) =>
    parse<InvoiceExportPack>(
      await call<string>("invoice_export_pack", {
        invoiceId,
        pasteLayout: args.pasteLayout,
        pasteSep: args.pasteSep,
      }),
    ),
  invoiceMarkExported: async (invoiceId: number, args: PasteFormat) =>
    parse<InvoiceExportPack>(
      await call<string>("invoice_mark_exported", {
        invoiceId,
        pasteLayout: args.pasteLayout,
        pasteSep: args.pasteSep,
      }),
    ),
  invoiceEvents: async (invoiceId: number) =>
    parse<InvoiceEvent[]>(await call<string>("invoice_events", { invoiceId })),
  invoiceSetStatus: (args: {
    invoiceId: number;
    toStatus: "cancelled" | "adjusted";
    reason: string;
    refInvoice?: string;
    adjustVoucherNo?: string;
  }) =>
    call<string>("invoice_set_status", {
      invoiceId: args.invoiceId,
      toStatus: args.toStatus,
      reason: args.reason,
      refInvoice: args.refInvoice ?? "",
      adjustVoucherNo: args.adjustVoucherNo ?? "",
    }),

  // ─── HĐĐT ───
  linkHddt: (args: { invoiceId: number; hddtNo: string; hddtSymbol: string; hddtDate: string }) =>
    call<string>("link_hddt", {
      invoiceId: args.invoiceId,
      hddtNo: args.hddtNo,
      hddtSymbol: args.hddtSymbol,
      hddtDate: args.hddtDate,
    }),
  // Tài khoản cổng HĐĐT
  hddtGetConfig: async () => parse<HddtConfig>(await call<string>("hddt_get_config")),
  /** Đọc mật khẩu HĐĐT đã lưu (Plaintext, chỉ trong app local). */
  hddtGetPassword: async () => parse<string>(await call<string>("hddt_get_password")),
  hddtSaveConfig: async (args: { username: string; password: string; baseUrl: string }) =>
    parse<HddtConfig>(
      await call<string>("hddt_save_config", {
        username: args.username,
        password: args.password,
        baseUrl: args.baseUrl,
      }),
    ),
  hddtStatus: async () => parse<HddtStatus>(await call<string>("hddt_status")),
  /** Đăng nhập (tự giải captcha); trả `need_manual=true` khi phải nhập tay. */
  hddtLogin: async () => parse<HddtLoginResult>(await call<string>("hddt_login")),
  /** Lấy captcha mới để người dùng nhập tay. */
  hddtCaptcha: async () => parse<{ key: string; svg: string }>(await call<string>("hddt_captcha")),
  hddtLoginManual: async (args: { captchaKey: string; captchaValue: string }) =>
    parse<HddtLoginResult>(
      await call<string>("hddt_login_manual", {
        captchaKey: args.captchaKey,
        captchaValue: args.captchaValue,
      }),
    ),
  hddtLogout: () => call<string>("hddt_logout"),
  hddtSetAutoChangePassword: async (enabled: boolean) =>
    parse<{ auto_change_password: boolean }>(
      await call<string>("hddt_set_auto_change_password", { enabled }),
    ),
  /** Đổi mật khẩu cổng (trả về mật khẩu mới để hiển thị 1 lần cho người dùng). */
  hddtChangePassword: async () =>
    parse<HddtLoginResult>(await call<string>("hddt_change_password")),
  hddtSendSimulated: (invoiceNo: string, symbol: string, total: number) =>
    call<string>("hddt_send_simulated", { invoiceNo, symbol, total }),

  /** Tra cứu hóa đơn trên cổng HĐĐT (trang `/tra-cuu/tra-cuu-hoa-don`).
   *  `direction` = "sold" | "purchase", `kind` = "regular" | "cash-register" quyết
   *  định endpoint; `from`/`to` theo `dd/MM/yyyy`; `state` = cursor phân trang từ
   *  response trước (null = trang đầu). Trả `{datas, state, total, time}`. */
  hddtListInvoices: async (f: HddtInvoiceFilter = {}) => {
    const r = JSON.parse(
      await call<string>("hddt_list_invoices", {
        direction: f.direction ?? "sold",
        kind: f.kind ?? "regular",
        from: f.from ?? null,
        to: f.to ?? null,
        state_filter: f.state ?? null,
        nmmst: f.nmmst ?? null,
        nbmst: f.nbmst ?? null,
        nmcmnd: f.nmcmnd ?? null,
        tthai: f.tthai ?? null,
        ttxly: f.ttxly ?? null,
        khmshdon: f.khmshdon ?? null,
        khhdon: f.khhdon ?? null,
        shdon: f.shdon ?? null,
        unhiem: f.unhiem ?? false,
        size: f.size ?? 15,
      }),
    );
    return r as HddtInvoiceList;
  },

  /** Đồng bộ HĐ nhập: quét cổng + cache theo ngày. `kinds` = "regular,cash-register". */
  hddtSyncScan: async (args: { from?: string; to?: string; kinds?: string[] }) => {
    const r = JSON.parse(
      await call<string>("hddt_sync_scan", {
        from: args.from ?? null,
        to: args.to ?? null,
        kinds: (args.kinds ?? ["regular", "cash-register"]).join(","),
      }),
    );
    return r as HddtSyncScanSummary;
  },

  /** Xem trước — đọc cache, không gọi cổng. `retryFailed` cho phép thử lại
   *  các hóa đơn lần trước chưa lấy được dòng hàng. */
  hddtSyncPreview: async (args: {
    from?: string;
    to?: string;
    retryFailed?: boolean;
  }): Promise<HddtSyncPreview> => {
    const r = JSON.parse(
      await call<string>("hddt_sync_preview", {
        from: args.from ?? null,
        to: args.to ?? null,
        retry_failed: args.retryFailed ?? false,
      }),
    );
    return { rows: r.rows ?? [], summary: r.summary };
  },

  /** Nhập kho hóa đơn đã cache (ids rỗng = tất cả đang chờ). Tạo mặt hàng/NCC
   *  còn thiếu, tạo phiếu nhập và liên kết hóa đơn chính thức với phiếu. */
  hddtSyncImport: async (args: {
    ids?: number[];
    warehouseCode?: string;
    unitCode?: string;
    debitAccount?: string;
    creditAccount?: string;
  }): Promise<HddtSyncImport> => {
    const r = JSON.parse(
      await call<string>("hddt_sync_import", {
        ids: args.ids ?? [],
        warehouse_code: args.warehouseCode ?? "",
        unit_code: args.unitCode ?? "HKD",
        debit_account: args.debitAccount ?? "",
        credit_account: args.creditAccount ?? "",
      }),
    );
    return { imported: r.imported, failed: r.failed, results: r.results ?? [] };
  },

  /** Xoá cache đồng bộ: hóa đơn CHƯA nhập kho + dấu ngày đã quét. Hóa đơn đã
   *  nhập kho giữ nguyên để không mất liên kết hóa đơn ↔ phiếu nhập. */
  hddtSyncClearCache: async (): Promise<HddtSyncClearCache> => {
    const r = JSON.parse(await call<string>("hddt_sync_clear_cache", {}));
    return { invoicesDeleted: r.invoices_deleted ?? 0, daysDeleted: r.days_deleted ?? 0 };
  },

  // ─── AUDIT LOG ───
  getAuditLog: async (limit = 200) =>
    parse<AuditEntry[]>(await call<string>("get_audit_log", { limit })),

  // ─── IMPORT KHỐI NHAP LIEU ───
  importNhapLieu: async (
    items: Array<{
      posting_date: string;
      voucher_no: string;
      doc_date: string;
      entry_type: string;
      description: string;
      product_code: string;
      supplier_code: string;
      customer_code: string;
      quantity: number;
      unit_price: number;
      amount: number;
      debit_account: string;
      credit_account: string;
      industry_code: string;
      vat_rate: number;
      pit_rate: number;
      tax_period: number;
      unit_code: string;
      adjust_code: string;
      note: string;
    }>,
  ) =>
    parse<ImportResult>(
      await call<string>("import_nhap_lieu", {
        items: items.map((i) => ({
          postingDate: i.posting_date,
          voucherNo: i.voucher_no,
          docDate: i.doc_date,
          entryType: i.entry_type,
          description: i.description,
          productCode: i.product_code,
          supplierCode: i.supplier_code,
          customerCode: i.customer_code,
          quantity: i.quantity,
          unitPrice: i.unit_price,
          amount: i.amount,
          debitAccount: i.debit_account,
          creditAccount: i.credit_account,
          industryCode: i.industry_code,
          vatRate: i.vat_rate,
          pitRate: i.pit_rate,
          taxPeriod: i.tax_period,
          unitCode: i.unit_code,
          adjustCode: i.adjust_code,
          note: i.note,
        })),
      }),
    ),

  // ─── CHẤM CÔNG THEO NGÀY ───
  getAttendance: async (period: string) =>
    parse<AttendanceRow[]>(await call<string>("get_attendance", { period })),
  saveAttendance: (
    period: string,
    entries: { employee_code: string; work_date: string; status: string }[],
  ) =>
    call<string>("save_attendance", {
      period,
      entries: entries.map((e) => ({
        employeeCode: e.employee_code,
        workDate: e.work_date,
        status: e.status,
      })),
    }),
  getAttendanceWorkDays: async (period: string) =>
    parse<AttendanceWorkDay[]>(await call<string>("get_attendance_work_days", { period })),

  // ─── TỜ KHAI THUẾ THEO KỲ ───
  // period: "year" | "quarter" | "month" | "occurrence"; periodNo: 1..12 (tháng) / 1..4 (quý) / 0 (năm)
  getTaxDeclaration: async (year: number, period: string, periodNo: number) =>
    parse<TaxDeclarationRow[]>(
      await call<string>("get_tax_declaration", { year, period, periodNo }),
    ),

  // Tổng hợp thuế phải nộp theo NĐ 68/2026 + NĐ 141/2026 (xếp nhóm hộ, GTGT/TNCN)
  getTaxOverview: async (year: number, period: string, periodNo: number) =>
    parse<TaxOverview>(await call<string>("get_tax_overview", { year, period, periodNo })),

  // ─── CHI TIẾT CHỨNG TỪ (in PNK/PXK) ───
  getVoucher: async (voucherNo: string) =>
    parse<VoucherRow[]>(await call<string>("get_voucher", { voucherNo })),

  // ─── ĐĂNG NHẬP / PHÂN QUYỀN ───
  login: async (username: string, password: string): Promise<CurrentUser> =>
    parse<CurrentUser>(await call<string>("login", { username, password })),
  logout: () => call<string>("logout"),
  // URL web server local (browser mode) — trả chuỗi rỗng nếu không chạy được.
  webUrl: () => call<string>("web_url"),
  // Bật/tắt web server (production: mặc định TẮT — bật trong màn Cài đặt).
  setWebServer: (enabled: boolean) => call<string>("set_web_server", { enabled }),
  getCurrentUser: async (): Promise<CurrentUser | null> => {
    const s = await call<string>("get_current_user");
    return s && s !== "null" ? parse<CurrentUser>(s) : null;
  },
  listUsers: async (): Promise<AppUser[]> => parse<AppUser[]>(await call<string>("list_users")),
  saveUser: (input: {
    username: string;
    display_name: string;
    password: string;
    role: Role;
    active: boolean;
  }) =>
    call<string>("save_user", {
      input: {
        username: input.username,
        displayName: input.display_name,
        password: input.password,
        role: input.role,
        active: input.active,
      },
    }),
  deleteUser: (id: number) => call<string>("delete_user", { id }),
  changePassword: (oldPassword: string, newPassword: string) =>
    call<string>("change_password", { oldPassword, newPassword }),

  // ─── HỒ SƠ HKD (multi-profile) ───
  getProfiles: async (): Promise<Profile[]> => parse<Profile[]>(await call<string>("get_profiles")),
  createProfile: async (name: string): Promise<Profile> =>
    parse<Profile>(await call<string>("create_profile", { name })),
  renameProfile: (key: string, name: string) => call<string>("rename_profile", { key, name }),
  deleteProfile: (key: string) => call<string>("delete_profile", { key }),
  switchProfile: async (key: string) =>
    parse<{ ok: boolean; key: string; name: string; active: boolean }>(
      await call<string>("switch_profile", { key }),
    ),
  /** Tùy chọn đăng nhập của mọi hồ sơ (đọc được trước khi đăng nhập). */
  getProfilePrefs: async (): Promise<Record<string, ProfilePrefs>> =>
    parse<Record<string, ProfilePrefs>>(await call<string>("get_profile_prefs")),
  saveProfilePref: (key: string, prefs: ProfilePrefs) =>
    call<string>("save_profile_pref", { key, prefs }),
  /** Mở hồ sơ từ màn hình chọn hồ sơ lúc khởi động (không cần đăng nhập). */
  selectProfile: async (key: string) =>
    parse<{ ok: boolean; key: string; name: string; active: boolean }>(
      await call<string>("select_profile", { key }),
    ),
};
