// Typed Tauri invoke wrappers — single entry point cho mọi backend call.
import { invoke } from "@tauri-apps/api/core";
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
    product_vat_rate_options: list("product_vat_rate_options", SETTING_DEFAULTS.product_vat_rate_options),
    product_vat_rate_default: num("product_vat_rate_default", SETTING_DEFAULTS.product_vat_rate_default),
    product_import_tax_options: list("product_import_tax_options", SETTING_DEFAULTS.product_import_tax_options),
    product_import_tax_default: num("product_import_tax_default", SETTING_DEFAULTS.product_import_tax_default),
    product_unit: raw.product_unit ?? SETTING_DEFAULTS.product_unit,
    product_min_stock: num("product_min_stock", SETTING_DEFAULTS.product_min_stock),
    tax_period: raw.tax_period || SETTING_DEFAULTS.tax_period,
    tax_method: raw.tax_method || SETTING_DEFAULTS.tax_method,
    vat_deduct: raw.vat_deduct === "1",
  };
}

// ─── INFO / CONFIG ───
export const api = {
  getBusinessInfo: () => invoke<string>("get_business_info"),
  getBusinessConfig: async () => parse<BusinessConfig>(await invoke<string>("get_business_config")),
  saveBusinessConfig: (cfg: Partial<BusinessConfig>) =>
    invoke<string>("save_business_config", {
      name: cfg.name ?? "",
      taxCode: cfg.tax_code ?? "",
      address: cfg.address ?? "",
      shortName: cfg.short_name ?? "",
      ownership: cfg.ownership ?? "Tư nhân",
      province: cfg.province ?? "",
      taxCodeIssuedOn: cfg.tax_code_issued_on ?? "",
      phone: cfg.phone ?? "",
      email: cfg.email ?? "",
    }),

  // ─── CÀI ĐẶT MẶC ĐỊNH ───
  getAppSettings: async () =>
    normalizeAppSettings(parse<Record<string, string>>(await invoke<string>("get_app_settings"))),
  saveAppSettings: (settings: Record<string, string>) =>
    invoke<string>("save_app_settings", { settings }),
  nextProductCode: () => invoke<string>("next_product_code"),

  // ─── MASTER DATA ───
  getAccounts: async () => parse<Account[]>(await invoke<string>("get_accounts")),
  saveAccount: (p: Partial<Account>) =>
    invoke<string>("save_account", {
      code: p.code ?? "",
      name: p.name ?? "",
      openingDebit: p.opening_debit ?? 0,
      openingCredit: p.opening_credit ?? 0,
    }),
  deleteAccount: (id: number) => invoke<string>("delete_account", { id }),
  getProducts: async () => parse<Product[]>(await invoke<string>("get_products")),
  /** Lấy 1 trang sản phẩm theo event lazy của PrimeVue DataTable (server-side sort/filter/page). */
  getProductsPage: async (lazyEvent: unknown) =>
    parse<{ rows: Product[]; total: number }>(
      await invoke<string>("get_products_page", {
        lazyEvent: JSON.stringify(lazyEvent),
      }),
    ),
  saveProduct: (p: Partial<Product>) =>
    invoke<string>("save_product", {
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
  deleteProduct: (id: number) => invoke<string>("delete_product", { id }),

  getIndustryGroups: async () =>
    parse<IndustryGroup[]>(await invoke<string>("get_industry_groups")),
  saveIndustryGroup: (g: {
    code: string;
    name: string;
    vat_rate: number;
    pit_rate: number;
  }) =>
    invoke<string>("save_industry_group", {
      code: g.code,
      name: g.name,
      vatRate: g.vat_rate,
      pitRate: g.pit_rate,
    }),
  deleteIndustryGroup: (code: string) =>
    invoke<string>("delete_industry_group", { code }),
  getWarehouses: async () => parse<Warehouse[]>(await invoke<string>("get_warehouses")),
  saveWarehouse: (code: string, name: string) =>
    invoke<string>("save_warehouse", { code, name }),
  nextWarehouseCode: () => invoke<string>("next_warehouse_code"),
  nextCustomerCode: () => invoke<string>("next_customer_code"),
  nextSupplierCode: () => invoke<string>("next_supplier_code"),
  nextEmployeeCode: () => invoke<string>("next_employee_code"),
  getSuppliers: async () => parse<Supplier[]>(await invoke<string>("get_suppliers")),
  saveSupplier: (s: Partial<Supplier>) =>
    invoke<string>("save_supplier", {
      code: s.code ?? "",
      name: s.name ?? "",
      address: s.address ?? "",
      taxCode: s.tax_code ?? "",
      phone: s.phone ?? "",
    }),
  getCustomers: async () => parse<Customer[]>(await invoke<string>("get_customers")),
  saveCustomer: (c: Partial<Customer>) =>
    invoke<string>("save_customer", {
      code: c.code ?? "",
      name: c.name ?? "",
      address: c.address ?? "",
      taxCode: c.tax_code ?? "",
      phone: c.phone ?? "",
    }),

  // ─── TRA CỨU MST (masothue.com qua backend) ───
  /** Tra cứu MST → 1 kết quả (single), danh sách (multiple) hoặc không thấy. */
  lookupTaxCode: async (mst: string): Promise<LookupOutcome> =>
    parse<LookupOutcome>(await invoke<string>("lookup_tax_code", { mst })),
  /** Lấy chi tiết từ URL masothue (dùng khi chọn trong danh sách). */
  lookupTaxDetail: async (url: string): Promise<TaxInfo> =>
    parse<TaxInfo>(await invoke<string>("lookup_tax_detail", { url })),

  // ─── NHÂN SỰ & BẢNG LƯƠNG ───
  getEmployees: async () => parse<Employee[]>(await invoke<string>("get_employees")),
  saveEmployee: (e: Partial<Employee>) =>
    invoke<string>("save_employee", {
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
  getPayrollPeriods: async () =>
    parse<string[]>(await invoke<string>("get_payroll_periods")),
  getPayroll: async (period: string) =>
    parse<PayrollRow[]>(await invoke<string>("get_payroll", { period })),
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
    invoke<string>("save_payroll", {
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
    invoke<string>("save_inbound", {
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
  }) =>
    invoke<string>("save_outbound", {
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
    }),

  getJournalEntries: (entryType = "", fromDate = "", toDate = "") =>
    invoke<string>("get_journal_entries", { entryType, fromDate, toDate }).then(parse) as Promise<
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
    invoke<string>("save_cash_entry", {
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
    parse<StockLot[]>(await invoke<string>("get_stock_lots", { productCode })),
  getInventorySummary: async () =>
    parse<InventoryRow[]>(await invoke<string>("get_inventory_summary")),

  // ─── INVOICES ───
  getInvoices: async () => parse<Invoice[]>(await invoke<string>("get_invoices")),
  getInvoiceDetail: async (id: number) =>
    parse<{ invoice: Invoice; items: InvoiceItem[] }>(
      await invoke<string>("get_invoice_detail", { id }),
    ),
  saveInvoice: (args: {
    number: string;
    date: string;
    customer: string;
    customer_tax_code: string;
    items: { product_code: string; quantity: number; unit_price: number }[];
  }) =>
    invoke<string>("save_invoice", {
      number: args.number,
      date: args.date,
      customer: args.customer,
      customerTaxCode: args.customer_tax_code,
      items: args.items,
    }),

  // ─── INVENTORY COUNT ───
  getInventoryCounts: async () =>
    parse<InventoryCount[]>(await invoke<string>("get_inventory_counts")),
  getInventoryCountDetail: async (id: number) =>
    parse<CountItemRow[]>(await invoke<string>("get_inventory_count_detail", { id })),
  saveInventoryCount: (args: {
    date: string;
    note: string;
    items: { product_code: string; warehouse_code: string; counted_qty: number }[];
  }) =>
    invoke<string>("save_inventory_count", {
      date: args.date,
      note: args.note,
      items: args.items,
    }),

  // ─── REPORTS ───
  getTaxSummary: async (fromDate: string, toDate: string, unitCode: string) =>
    parse<TaxSummaryRow[]>(
      await invoke<string>("get_tax_summary", { fromDate, toDate, unitCode }),
    ),
  getRevenueExpense: async (fromDate: string, toDate: string) =>
    parse<RevenueExpenseRow>(await invoke<string>("get_revenue_expense", { fromDate, toDate })),
  getTrialBalance: async (fromDate: string, toDate: string) =>
    parse<TrialBalanceRow[]>(
      await invoke<string>("get_trial_balance", { fromDate, toDate }),
    ),

  // ─── SỔ SÁCH ───
  getLedger: async (fromDate: string, toDate: string) =>
    parse<LedgerRow[]>(await invoke<string>("get_ledger", { fromDate, toDate })),

  // ─── BACKUP / RESTORE ───
  createBackup: () => invoke<string>("create_backup"),
  listBackups: async () => parse<string[]>(await invoke<string>("list_backups")),
  restoreBackup: (filename: string) => invoke<string>("restore_backup", { filename }),

  // ─── HĐĐT ───
  linkHddt: (args: {
    invoiceId: number;
    hddtNo: string;
    hddtSymbol: string;
    hddtDate: string;
  }) =>
    invoke<string>("link_hddt", {
      invoiceId: args.invoiceId,
      hddtNo: args.hddtNo,
      hddtSymbol: args.hddtSymbol,
      hddtDate: args.hddtDate,
    }),
  hddtStatus: async () => parse<{ mode: string; connected: boolean; note: string }>(
    await invoke<string>("hddt_status"),
  ),
  hddtSendSimulated: (invoiceNo: string, symbol: string, total: number) =>
    invoke<string>("hddt_send_simulated", { invoiceNo, symbol, total }),

  // ─── AUDIT LOG ───
  getAuditLog: async (limit = 200) =>
    parse<AuditEntry[]>(await invoke<string>("get_audit_log", { limit })),

  // ─── IMPORT KHỐI NHAP LIEU ───
  importNhapLieu: async (items: Array<{
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
  }>) =>
    parse<ImportResult>(
      await invoke<string>("import_nhap_lieu", {
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
    parse<AttendanceRow[]>(await invoke<string>("get_attendance", { period })),
  saveAttendance: (
    period: string,
    entries: { employee_code: string; work_date: string; status: string }[],
  ) =>
    invoke<string>("save_attendance", {
      period,
      entries: entries.map((e) => ({
        employeeCode: e.employee_code,
        workDate: e.work_date,
        status: e.status,
      })),
    }),
  getAttendanceWorkDays: async (period: string) =>
    parse<AttendanceWorkDay[]>(
      await invoke<string>("get_attendance_work_days", { period }),
    ),

  // ─── TỜ KHAI THUẾ THEO KỲ ───
  // period: "year" | "quarter" | "month" | "occurrence"; periodNo: 1..12 (tháng) / 1..4 (quý) / 0 (năm)
  getTaxDeclaration: async (year: number, period: string, periodNo: number) =>
    parse<TaxDeclarationRow[]>(
      await invoke<string>("get_tax_declaration", { year, period, periodNo }),
    ),

  // Tổng hợp thuế phải nộp theo NĐ 68/2026 + NĐ 141/2026 (xếp nhóm hộ, GTGT/TNCN)
  getTaxOverview: async (year: number, period: string, periodNo: number) =>
    parse<TaxOverview>(
      await invoke<string>("get_tax_overview", { year, period, periodNo }),
    ),

  // ─── CHI TIẾT CHỨNG TỪ (in PNK/PXK) ───
  getVoucher: async (voucherNo: string) =>
    parse<VoucherRow[]>(await invoke<string>("get_voucher", { voucherNo })),

  // ─── ĐĂNG NHẬP / PHÂN QUYỀN ───
  login: async (username: string, password: string): Promise<CurrentUser> =>
    parse<CurrentUser>(await invoke<string>("login", { username, password })),
  logout: () => invoke<string>("logout"),
  getCurrentUser: async (): Promise<CurrentUser | null> => {
    const s = await invoke<string>("get_current_user");
    return s && s !== "null" ? parse<CurrentUser>(s) : null;
  },
  listUsers: async (): Promise<AppUser[]> =>
    parse<AppUser[]>(await invoke<string>("list_users")),
  saveUser: (input: {
    username: string;
    display_name: string;
    password: string;
    role: Role;
    active: boolean;
  }) =>
    invoke<string>("save_user", {
      input: {
        username: input.username,
        displayName: input.display_name,
        password: input.password,
        role: input.role,
        active: input.active,
      },
    }),
  deleteUser: (id: number) => invoke<string>("delete_user", { id }),
  changePassword: (oldPassword: string, newPassword: string) =>
    invoke<string>("change_password", { oldPassword, newPassword }),

  // ─── HỒ SƠ HKD (multi-profile) ───
  getProfiles: async (): Promise<Profile[]> =>
    parse<Profile[]>(await invoke<string>("get_profiles")),
  createProfile: async (name: string): Promise<Profile> =>
    parse<Profile>(await invoke<string>("create_profile", { name })),
  renameProfile: (key: string, name: string) =>
    invoke<string>("rename_profile", { key, name }),
  deleteProfile: (key: string) => invoke<string>("delete_profile", { key }),
  switchProfile: async (key: string) =>
    parse<{ ok: boolean; key: string; name: string; active: boolean }>(
      await invoke<string>("switch_profile", { key }),
    ),
  /** Tùy chọn đăng nhập của mọi hồ sơ (đọc được trước khi đăng nhập). */
  getProfilePrefs: async (): Promise<Record<string, ProfilePrefs>> =>
    parse<Record<string, ProfilePrefs>>(await invoke<string>("get_profile_prefs")),
  saveProfilePref: (key: string, prefs: ProfilePrefs) =>
    invoke<string>("save_profile_pref", { key, prefs }),
  /** Mở hồ sơ từ màn hình chọn hồ sơ lúc khởi động (không cần đăng nhập). */
  selectProfile: async (key: string) =>
    parse<{ ok: boolean; key: string; name: string; active: boolean }>(
      await invoke<string>("select_profile", { key }),
    ),
};