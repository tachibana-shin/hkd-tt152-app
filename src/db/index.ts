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
  BomItem,
  Product,
  IndustryGroup,
  Warehouse,
  Supplier,
  Customer,
  RevenueExpenseRow,
  CogsBackfillPending,
  TrialBalanceRow,
  InventoryRow,
  StockLot,
  JournalEntryRow,
  ProductionLotRow,
  StockVoucherRow,
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
  IndustryRevenue,
  TaxBook,
  TaxDeclarationRow,
  TaxOverview,
  TaxSettlement,
  TaxGroupInfo,
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
  HddtSaveXmlResult,
  HddtSavedXml,
  InvoiceEvent,
  InvoiceExportPack,
  InvoiceQueueItem,
  InvoiceLinkResult,
  InvoiceReplaceResult,
  InvoiceDuplicateNumber,
  JobProgress,
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
  pit_exempt_alloc: "", // Mức trừ ngưỡng theo nhóm ngành — rỗng = chia theo tỷ lệ DT
  tax_threshold_exempt: 1_000_000_000, // Ngưỡng DT năm không phải nộp GTGT + TNCN
  tax_threshold_group3: 3_000_000_000, // Mốc Nhóm 3 (TNCN theo thu nhập 17%)
  tax_threshold_group4: 50_000_000_000, // Mốc Nhóm 4 (TNCN theo thu nhập 20%)
  auto_check_update: true, // Tự kiểm tra cập nhật khi mở app (bản web thì tự bỏ qua)
  auto_sync_enabled: false, // Tự quét cổng HĐĐT — mặc định TẮT, để người dùng tự bật
  auto_sync_interval_min: 15, // Khoảng thời gian tự gọi lại (phút)
};

// Backend lưu mọi thứ dưới dạng chuỗi → chuẩn hóa về kiểu AppSettings.
function normalizeAppSettings(raw: Record<string, string>): AppSettings {
  const num = (k: keyof AppSettings, d: number): number => {
    const n = Number(raw[k]);
    return Number.isFinite(n) ? n : d;
  };
  const positive = (v: number, d: number): number => (v > 0 ? v : d);
  const list = (k: keyof AppSettings, d: number[]): number[] => {
    try {
      const a: unknown = JSON.parse(raw[k] ?? "[]");
      return Array.isArray(a) ? a.filter((x): x is number => typeof x === "number") : d;
    } catch {
      return d;
    }
  };
  const exempt = positive(
    num("tax_threshold_exempt", SETTING_DEFAULTS.tax_threshold_exempt),
    SETTING_DEFAULTS.tax_threshold_exempt,
  );
  const group3 = Math.max(
    exempt,
    positive(num("tax_threshold_group3", SETTING_DEFAULTS.tax_threshold_group3), 0),
  );
  const group4 = Math.max(
    group3,
    positive(num("tax_threshold_group4", SETTING_DEFAULTS.tax_threshold_group4), 0),
  );
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
    pit_exempt_alloc: raw.pit_exempt_alloc ?? SETTING_DEFAULTS.pit_exempt_alloc,
    // Ba mốc doanh thu: rác/số ≤ 0 thì lấy mặc định, và luôn giữ thứ tự tăng dần
    // để nhóm hộ xếp không bị lộn xộn (backend cũng chốt lại như vậy).
    tax_threshold_exempt: exempt,
    tax_threshold_group3: group3,
    tax_threshold_group4: group4,
    // Việc chạy nền: thiếu key thì lấy mặc định (bật kiểm tra cập nhật, tắt quét
    // cổng); số phút ép về 1–1440 để timer không nhận giá trị rác từ DB.
    auto_check_update: raw.auto_check_update === "0" ? false : true,
    auto_sync_enabled: raw.auto_sync_enabled === "1",
    auto_sync_interval_min: Math.min(
      1440,
      Math.max(1, num("auto_sync_interval_min", SETTING_DEFAULTS.auto_sync_interval_min)),
    ),
  };
}

// ─── INFO / CONFIG ───
/** Kiểu trả về chung của các lệnh phân trang server-side (`*_page`). */
export interface PageResult<T> {
  rows: T[];
  total: number;
}

export const api = {
  getBusinessInfo: () => call<string>("get_business_info"),
  getBusinessConfig: async () => parse<BusinessConfig>(await call<string>("get_business_config")),
  saveBusinessConfig: (cfg: Partial<BusinessConfig>) =>
    call<string>("save_business_config", {
      name: cfg.name ?? "",
      taxCode: cfg.tax_code ?? "",
      address: cfg.address ?? "",
      location: cfg.location ?? "",
      shortName: cfg.short_name ?? "",
      ownership: cfg.ownership ?? "Tư nhân",
      province: cfg.province ?? "",
      taxCodeIssuedOn: cfg.tax_code_issued_on ?? "",
      phone: cfg.phone ?? "",
      email: cfg.email ?? "",
      hddtStartDate: cfg.hddt_start_date ?? "",
      hddtSymbol: cfg.hddt_symbol ?? "",
      // undefined = để app tự xếp nhóm theo doanh thu; 1–4 = người dùng chốt.
      taxGroup: cfg.tax_group ?? null,
    }),

  // ─── CÀI ĐẶT MẶC ĐỊNH ───
  getAppSettings: async () =>
    normalizeAppSettings(parse<Record<string, string>>(await call<string>("get_app_settings"))),
  saveAppSettings: (settings: Record<string, string>) =>
    call<string>("save_app_settings", { settings }),
  /** `app_setting` dạng chuỗi thô — cho các key nằm ngoài `AppSettings` (vd bản
   *  ghi tay của tờ khai kê khai). `getAppSettings` đã chuẩn hoá nên mất key lạ. */
  getRawAppSettings: async () =>
    parse<Record<string, string>>(await call<string>("get_app_settings")),
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
  /**
   * Tạo hàng loạt mặt hàng CHƯA có trong danh mục (nhập Excel — vài nghìn dòng
   * trong 1 lần gọi thay vì gọi save_product từng cái). Tên đã có giữ nguyên mã
   * cũ. Trả về { map: { "tên hàng": "mã hàng" }, created }.
   */
  saveProductsBulk: async (items: { name: string; unit: string; cost_price: number }[]) =>
    parse<{ map: Record<string, string>; created: number }>(
      await call<string>("save_products_bulk", { items }),
    ),
  /** Lưu danh sách "tên khác" (alias) của sản phẩm — thay toàn bộ danh sách cũ (F5). */
  saveProductAliases: (code: string, aliases: string[]) =>
    call<string>("save_product_aliases", { code, aliases }),
  /** Toàn bộ định mức vật tư của mọi sản phẩm (màn "Định mức" — F4). */
  getProductBoms: async () => parse<BomItem[]>(await call<string>("get_product_boms")),
  /** Lưu định mức vật tư của 1 sản phẩm — thay toàn bộ dòng cũ (rỗng = xóa). */
  saveProductBom: (productCode: string, items: { material_code: string; quantity: number }[]) =>
    call<string>("save_product_bom", { productCode, items }),
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
  getWarehousesPage: async (lazyEvent: unknown) =>
    parse<PageResult<Warehouse[]>>(
      await call<string>("get_warehouses_page", { lazyEvent: JSON.stringify(lazyEvent) }),
    ),
  getWarehouses: async () => parse<Warehouse[]>(await call<string>("get_warehouses")),
  saveWarehouse: (code: string, name: string) => call<string>("save_warehouse", { code, name }),
  nextWarehouseCode: () => call<string>("next_warehouse_code"),
  nextCustomerCode: () => call<string>("next_customer_code"),
  nextSupplierCode: () => call<string>("next_supplier_code"),
  nextEmployeeCode: () => call<string>("next_employee_code"),
  getSuppliersPage: async (lazyEvent: unknown) =>
    parse<PageResult<Supplier[]>>(
      await call<string>("get_suppliers_page", { lazyEvent: JSON.stringify(lazyEvent) }),
    ),
  getSuppliers: async () => parse<Supplier[]>(await call<string>("get_suppliers")),
  saveSupplier: (s: Partial<Supplier>) =>
    call<string>("save_supplier", {
      code: s.code ?? "",
      name: s.name ?? "",
      address: s.address ?? "",
      taxCode: s.tax_code ?? "",
      phone: s.phone ?? "",
    }),
  getCustomersPage: async (lazyEvent: unknown) =>
    parse<PageResult<Customer[]>>(
      await call<string>("get_customers_page", { lazyEvent: JSON.stringify(lazyEvent) }),
    ),
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
    /**
     * Ô tick "Tự xuất NVL theo định mức" (F4) — chỉ loại nhập `production`.
     * Bật → app tự tính NVL = định mức × SL, tự sinh phiếu xuất Nợ 154 / Có 152.
     */
    auto_bom?: boolean;
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
      autoBom: args.auto_bom ?? false,
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
      /** Tên hiển thị dòng (tên khác / alias người dùng chọn) — F5. */
      line_name?: string;
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

  getJournalEntriesPage: async (
    lazyEvent: unknown,
    entryType: string,
    fromDate: string,
    toDate: string,
  ) =>
    parse<PageResult<JournalEntryRow>>(
      await call<string>("get_journal_entries_page", {
        lazyEvent: JSON.stringify(lazyEvent),
        entryType,
        fromDate,
        toDate,
      }),
    ),
  getJournalEntries: (entryType = "", fromDate = "", toDate = "", search = "") =>
    call<string>("get_journal_entries", {
      entryType,
      fromDate,
      toDate,
      search,
    }).then(parse) as Promise<JournalEntryRow[]>,

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
    /** Chứng từ thanh toán không dùng tiền mặt (tài khoản / mã giao dịch). */
    bank_code?: string;
    /** false = khoản chi không được trừ khi tính thu nhập tính thuế (Điều 6 khoản 2). */
    deductible?: boolean;
    non_deductible_reason?: string;
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
        bankCode: input.bank_code ?? "",
        deductible: input.deductible ?? true,
        nonDeductibleReason: input.non_deductible_reason ?? "",
      },
    }),

  // ─── STOCK ───
  getStockLots: async (productCode = "") =>
    parse<StockLot[]>(await call<string>("get_stock_lots", { productCode })),
  getInventorySummary: async () =>
    parse<InventoryRow[]>(await call<string>("get_inventory_summary")),

  // ─── INVOICES ───
  getInvoicesPage: async (lazyEvent: unknown, onlyDuplicates = false) =>
    parse<PageResult<Invoice>>(
      await call<string>("get_invoices_page", {
        lazyEvent: JSON.stringify(lazyEvent),
        onlyDuplicates,
      }),
    ),
  getInvoices: async () => parse<Invoice[]>(await call<string>("get_invoices")),
  deleteInvoice: (id: number) => call<string>("delete_invoice", { id }),
  updateInvoice: (args: {
    id: number;
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
    call<string>("update_invoice", {
      id: args.id,
      number: args.number,
      date: args.date,
      customer: args.customer,
      customer_tax_code: args.customer_tax_code,
      items: args.items,
    }),
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

  getRevenueExpense: async (fromDate: string, toDate: string) =>
    parse<RevenueExpenseRow>(await call<string>("get_revenue_expense", { fromDate, toDate })),
  /** Số phiếu xuất lịch sử còn thiếu bút toán Nợ 632 / Có 152 (hết khi đã ghi bù). */
  getCogsBackfillPending: async () =>
    parse<CogsBackfillPending>(await call<string>("cogs_backfill_pending")),
  /** Ghi bút toán giá vốn cho các phiếu xuất lịch sử — chạy lại không ghi trùng. */
  backfillCogsEntries: async () =>
    parse<CogsBackfillPending>(await call<string>("backfill_cogs_entries")),
  getTrialBalance: async (fromDate: string, toDate: string) =>
    parse<TrialBalanceRow[]>(await call<string>("get_trial_balance", { fromDate, toDate })),

  // ─── SỔ SÁCH ───
  /** Sổ nhật ký: thêm `total_amount` = tổng tiền CẢ kỳ lọc (server tính). */
  getLedgerPage: async (lazyEvent: unknown, fromDate: string, toDate: string, entryType: string) =>
    parse<PageResult<LedgerRow> & { total_amount: number }>(
      await call<string>("get_ledger_page", {
        lazyEvent: JSON.stringify(lazyEvent),
        fromDate,
        toDate,
        entryType,
      }),
    ),
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
  invoiceQueuePage: async (lazyEvent: unknown, status = "", fromDate = "", toDate = "") =>
    parse<PageResult<InvoiceQueueItem>>(
      await call<string>("invoice_queue_page", {
        lazyEvent: JSON.stringify(lazyEvent),
        status,
        fromDate,
        toDate,
      }),
    ),
  invoiceQueue: async () => parse<InvoiceQueueItem[]>(await call<string>("invoice_queue", {})),
  /** Lập phiếu xuất cho hóa đơn chưa có phiếu (trừ tồn + ghi Nợ 131/Có 511). */
  createInvoiceOutbound: (invoiceId: number) =>
    call<string>("create_invoice_outbound", { invoiceId }),
  /** Số phiếu kế tiếp do backend sinh (không đoán từ danh sách đang lọc). */
  nextVoucherNo: (entryType: "PN" | "PX" | "PT" | "PC") =>
    call<string>("next_voucher_no", { entryType }),
  /** Số hóa đơn bị trùng (sinh ra khi bấm Lập nhiều lần) — app không tự sửa. */
  invoiceDuplicateNumbers: async () =>
    parse<InvoiceDuplicateNumber[]>(await call<string>("invoice_duplicate_numbers", {})),
  invoiceEvents: async (invoiceId: number) =>
    parse<InvoiceEvent[]>(await call<string>("invoice_events", { invoiceId })),
  invoiceSetStatus: (args: {
    invoiceId: number;
    toStatus: "adjusted";
    reason: string;
    refInvoice?: string;
  }) =>
    call<string>("invoice_set_status", {
      invoiceId: args.invoiceId,
      toStatus: args.toStatus,
      reason: args.reason,
      refInvoice: args.refInvoice ?? "",
    }),
  /**
   * Thay thế hóa đơn đã phát hành (TT 91/2026 Điều 10): đảo doanh thu bằng
   * Nợ 511 / Có 131 và tạo hóa đơn nháp mới sao chép dòng hàng.
   */
  invoiceReplace: async (invoiceId: number, reason: string) =>
    parse<InvoiceReplaceResult>(await call<string>("replace_invoice", { invoiceId, reason })),

  // ─── HĐĐT ───
  linkHddt: async (args: {
    invoiceId: number;
    hddtNo: string;
    hddtSymbol: string;
    hddtDate: string;
  }) =>
    parse<InvoiceLinkResult>(
      await call<string>("link_hddt", {
        invoiceId: args.invoiceId,
        hddtNo: args.hddtNo,
        hddtSymbol: args.hddtSymbol,
        hddtDate: args.hddtDate,
      }),
    ),
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

  // ─── XEM PDF HÓA ĐƠN ĐIỆN TỬ ───
  /** Chi tiết hóa đơn lấy trực tiếp từ cổng — màn Tra cứu HĐĐT chưa có trong DB. */
  hddtInvoiceDetail: (args: {
    nbmst: string;
    khmshdon: number;
    khhdon: string;
    shdon: string;
    id?: string;
  }) =>
    call<string>("hddt_invoice_detail", {
      nbmst: args.nbmst,
      khmshdon: args.khmshdon,
      khhdon: args.khhdon,
      shdon: args.shdon,
      id: args.id ?? null,
    }),

  /** Chi tiết hóa đơn đã lưu trong DB theo `portal_id` (màn Đồng bộ HĐĐT). */
  hddtSyncInvoiceDetail: (portalId: string) =>
    call<string>("hddt_sync_invoice_detail", { portalId }),

  /** Chi tiết hóa đơn đã gắn với 1 phiếu nhập (dùng trong popup xem phiếu). */
  hddtVoucherInvoiceDetail: (voucherNo: string) =>
    call<string>("hddt_voucher_invoice_detail", { voucherNo }),

  /** Sinh PDF hóa đơn từ `detail_json` (backend dựng HTML + render) — base64. */
  renderInvoicePdf: (detailJson: string) => call<string>("render_invoice_pdf", { detailJson }),

  // ─── LƯU FILE XML HÓA ĐƠN ĐIỆN TỬ ───
  /** Tải XML 1 hóa đơn từ cổng rồi ghi vào `…/profiles/<key>/hddt_xml/`.
   *  `direction` = "sold" | "purchase", `kind` = "regular" | "cash-register".
   *  Truyền `kind` rỗng → backend suy từ ký hiệu (chứ `CG` = máy tính tiền) —
   *  nút "Tải XML" ở chi tiết hóa đơn không biết hộ dùng loại nào. Web mode
   *  bắt buộc mọi khoá có mặt trong JSON nên không được bỏ trống khoá này.
   *  Đã lưu rồi → `already: true`. */
  hddtSaveXml: async (args: {
    direction?: "sold" | "purchase";
    kind?: "regular" | "cash-register";
    nbmst: string;
    khmshdon: number;
    khhdon: string;
    shdon: string;
    portalId?: string;
  }): Promise<HddtSaveXmlResult> =>
    parse<HddtSaveXmlResult>(
      await call<string>("hddt_save_xml", {
        direction: args.direction ?? "sold",
        kind: args.kind ?? "",
        nbmst: args.nbmst,
        khmshdon: args.khmshdon,
        khhdon: args.khhdon,
        shdon: args.shdon,
        portal_id: args.portalId ?? null,
      }),
    ),

  /** Toàn bộ file XML đã lưu — tra "hóa đơn nào đã có file" cho bảng UI. */
  hddtListXml: async (): Promise<HddtSavedXml[]> =>
    parse<HddtSavedXml[]>(await call<string>("hddt_list_xml", {})),

  /** Gộp các file XML đã lưu thành 1 ZIP để nộp/gửi — trả **base64**.
   *  `ids` rỗng = gộp tất cả (dùng `downloadZip` để tải về máy). */
  hddtExportXmlZip: async (ids: number[]): Promise<string> =>
    call<string>("hddt_export_xml_zip", { ids }),

  /** `detail_json` dựng từ hóa đơn nội bộ — để xem trước PDF ở Chờ xuất HĐĐT. */
  invoiceDraftDetail: (id: number) => call<string>("invoice_draft_detail", { id }),

  // ─── AUDIT LOG ───
  getAuditLog: async (limit = 200) =>
    parse<AuditEntry[]>(await call<string>("get_audit_log", { limit })),
  /** Phân trang server-side (mỗi lần đổi trang/lọc chỉ lấy đúng trang đó). */
  getAuditLogPage: async (lazyEvent: unknown) =>
    parse<PageResult<AuditEntry>>(
      await call<string>("get_audit_log_page", { lazyEvent: JSON.stringify(lazyEvent) }),
    ),

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

  // ─── TỜ KHAI THUẾ (theo khoảng ngày đang chọn ở thanh công cụ) ───
  /**
   * Một bảng duy nhất, chia theo nhóm ngành nghề.
   *
   * Đã gộp "theo nhóm ngành nghề" (bám khoảng ngày) với "theo kỳ khai" vì hai
   * bảng cho ra cùng một dữ liệu. Mức trừ ngưỡng TNCN vẫn tính lũy kế từ đầu
   * năm nên xem khoảng nào cũng không tính trùng.
   */
  getTaxDeclaration: async (fromDate: string, toDate: string, unitCode: string) =>
    parse<TaxDeclarationRow[]>(
      await call<string>("get_tax_declaration", { fromDate, toDate, unitCode }),
    ),

  /**
   * Tổng hợp thuế phải nộp theo NĐ 68/2026 + NĐ 141/2026 (xếp nhóm hộ, GTGT/TNCN)
   * — cùng khoảng ngày với tờ khai nên hai thẻ không mâu thuẫn nhau.
   * `deadline` chỉ có khi khoảng đang xem trùng đúng một kỳ khai.
   */
  getTaxOverview: async (fromDate: string, toDate: string, unitCode: string) =>
    parse<TaxOverview>(await call<string>("get_tax_overview", { fromDate, toDate, unitCode })),

  /** Doanh thu từng nhóm ngành trong khoảng ngày — **cả nhóm chưa phát sinh**,
   *  dùng cho màn kê khai 6 dòng theo mẫu (cùng khoảng ngày với tờ khai). */
  getRevenueDeclaration: async (fromDate: string, toDate: string, unitCode: string) =>
    parse<IndustryRevenue[]>(
      await call<string>("get_revenue_by_industry", { fromDate, toDate, unitCode }),
    ),
  /**
   * Đóng gói **báo cáo kỳ thuế** thành 1 file ZIP rồi trả về base64.
   *
   * `extraFiles` = tờ khai + sổ kế toán do frontend dựng (`{path, data}` base64);
   * backend tự thêm phần frontend không với tới được: PDF hóa đơn render trong
   * kỳ, XML đã lưu và bản sao lưu CSDL — xem `commands/tax_report.rs`.
   */
  exportTaxReport: (args: {
    /** UUID sinh riêng cho lần xuất này — để hỏi đúng tiến độ của mình. */
    jobId: string;
    fromDate: string;
    toDate: string;
    extraFiles: { path: string; data: string }[];
  }): Promise<string> =>
    call<string>("export_tax_report", {
      jobId: args.jobId,
      fromDate: args.fromDate,
      toDate: args.toDate,
      extraFiles: args.extraFiles,
    }),
  /**
   * Ảnh chụp tiến độ + nhật ký của việc đang chạy. Trả `null` khi việc này đã
   * bị việc khác thay — frontend ngừng hỏi khi thấy `finished`.
   */
  getJobProgress: async (jobId: string): Promise<JobProgress | null> =>
    parse<JobProgress | null>(await call<string>("get_job_progress", { jobId })),
  /** Nhóm hộ hiện hành + doanh thu cả năm dùng xếp nhóm (hộp cấu hình HKD tự điền). */
  getTaxGroup: async (year: number) =>
    parse<TaxGroupInfo>(await call<string>("get_tax_group", { year })),

  /**
   * Tạm nộp TNCN theo kỳ + quyết toán cả năm (Điều 10 khoản 2 NĐ 68/2026).
   * Hộ nộp theo tỷ lệ % doanh thu thì khai theo kỳ, không có quyết toán năm riêng.
   */
  getTaxSettlement: async (year: number) =>
    parse<TaxSettlement>(await call<string>("get_tax_settlement", { year })),

  /** Nội dung 1 mẫu sổ kế toán theo TT 152/2025/TT-BTC (S1a | S2a | S2b | S2c | S2d | S2e). */
  getTaxBooks: async (year: number, period: string, periodNo: number, book: string) =>
    parse<TaxBook>(await call<string>("get_tax_books", { year, period, periodNo, book })),

  /**
   * Mẫu sổ **áp dụng** cho hồ sơ đang mở theo Điều 4 TT 152/2025 — vd Nhóm 1 chỉ
   * có S1a, nhóm 2 nộp TNCN theo thu nhập thì S2b–S2e. Màn Kế toán chỉ hiện các
   * mẫu này (có công tắc xem đủ 7 mẫu khi đối chiếu).
   */
  getApplicableBooks: async (year: number) =>
    parse<string[]>(await call<string>("get_applicable_books", { year })),

  // ─── CHI TIẾT CHỨNG TỪ (in PNK/PXK) ───
  /**
   * Danh sách phiếu nhập / phiếu xuất gom theo phiếu — **1 phiếu = 1 dòng**
   * (kèm số loại mặt hàng, tổng chiết khấu, thành tiền). Chi tiết dòng hàng vẫn
   * xem qua `getVoucher` như trước.
   */
  getStockVouchersPage: async (
    lazyEvent: unknown,
    entryType: string,
    fromDate: string,
    toDate: string,
  ) =>
    parse<PageResult<StockVoucherRow>>(
      await call<string>("get_stock_vouchers_page", {
        lazyEvent: JSON.stringify(lazyEvent),
        entryType,
        fromDate,
        toDate,
      }),
    ),

  getVoucher: async (voucherNo: string) =>
    parse<VoucherRow[]>(await call<string>("get_voucher", { voucherNo })),

  /**
   * Danh sách lô sản xuất (phiếu nhập loại `production`) — 1 lô = 1 dòng kèm
   * tên thành phẩm + giá trị nhập kho, phân trang server-side.
   */
  getProductionLots: async (lazyEvent: unknown, fromDate: string, toDate: string) =>
    parse<PageResult<ProductionLotRow>>(
      await call<string>("get_production_lots", {
        lazyEvent: JSON.stringify(lazyEvent),
        fromDate,
        toDate,
      }),
    ),

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
