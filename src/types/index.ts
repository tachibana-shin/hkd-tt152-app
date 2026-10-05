// ─── Danh mục ───

export interface Product {
  id: number;
  code: string;
  name: string;
  unit: string;
  sale_price: number;
  cost_price: number;
  min_stock: number;
  vat_rate: number;
  import_tax_rate: number;
  default_warehouse_id?: number;
  /** Sản phẩm dịch vụ (nhân công...) — không theo dõi tồn kho. */
  is_service?: boolean;
  /** Nhóm ngành mặc định (cơ sở tỷ lệ thuế bán ra). */
  industry_code?: string;
  /** Danh sách "tên khác" của sản phẩm, phân tách ", " (F5). */
  aliases?: string;
}

/** 1 dòng định mức vật tư (BOM) đã join tên — payload màn "Định mức" (F4). */
export interface BomItem {
  product_code: string;
  product_name: string;
  material_code: string;
  material_name: string;
  material_unit: string;
  quantity: number;
}

export interface Warehouse {
  id: number;
  code: string;
  name: string;
}

export interface Customer {
  id: number;
  code: string;
  name: string;
  address?: string;
  tax_code?: string;
  phone?: string;
}

export interface Supplier {
  id: number;
  code: string;
  name: string;
  address?: string;
  tax_code?: string;
  phone?: string;
}

// ─── Tra cứu MST (masothue) ───

export interface TaxInfo {
  name?: string;
  tax_code?: string;
  address?: string;
  representative?: string;
  status?: string;
  phone?: string;
  company_type?: string;
  managed_by?: string;
  issued_on?: string;
  international_name?: string;
  short_name?: string;
}

export interface TaxResult {
  mst: string;
  name: string;
  url: string;
}

export type LookupOutcome =
  | { kind: "single"; info: TaxInfo }
  | { kind: "multiple"; results: TaxResult[] }
  | { kind: "notFound"; message: string };

export interface BusinessUnit {
  id: number;
  code: string;
  name: string;
}

export interface IndustryGroup {
  id: number;
  code: string;
  name: string;
  vat_rate: number;
  pit_rate: number;
}

export interface Account {
  id: number;
  code: string;
  name: string;
  opening_debit: number;
  opening_credit: number;
}

// ─── FIFO ───

export interface StockLot {
  id: number;
  product_code: string;
  product_name: string;
  warehouse_code: string;
  quantity: number;
  unit_cost: number;
  received_at: string;
  depleted: boolean;
}

// ─── Giao dịch nhập/xuất ───

export interface JournalEntry {
  id: number;
  posting_date: string;
  voucher_no: string;
  entry_type: "PN" | "PX" | "PT" | "PC" | "OTHER";
  description: string;
  product_code?: string;
  supplier_code?: string;
  customer_code?: string;
  quantity: number;
  unit_price: number;
  amount: number;
  debit_account?: string;
  credit_account?: string;
  industry_code?: string;
  vat_rate: number;
  pit_rate: number;
  tax_period?: number;
  unit_code?: string;
  sale_location?: string;
  adjust_code?: string;
  note?: string;
}

/**
 * 1 dòng của bảng Phiếu nhập / Phiếu xuất = 1 phiếu (gom từ `journal_entry`).
 *
 * Màn kho trước đây in từng dòng hàng nên một phiếu 10 mặt hàng ra 10 dòng trùng
 * số phiếu; giờ gom lại 1 dòng 1 phiếu, bấm vào để xem chi tiết từng dòng.
 */
/** 1 lô sản xuất (phiếu nhập loại `production`) trên danh sách Lô sản xuất. */
export interface ProductionLotRow {
  voucher_no: string;
  posting_date: string;
  description: string;
  /** Thành phẩm của lô, gộp bằng ", " (thường 1 lô = 1 thành phẩm). */
  products: string;
  /** Số LOẠI thành phẩm của lô. */
  item_count: number;
  total_qty: number;
  /** Giá trị nhập kho = giá thành ước tính (đ). */
  amount: number;
  warehouse_code: string;
}

export interface StockVoucherRow {
  voucher_no: string;
  posting_date: string;
  entry_type: string;
  description: string;
  supplier_name?: string | null;
  customer_name?: string | null;
  /** Số dòng hàng của phiếu (kể cả dòng điều chỉnh). */
  line_count: number;
  /** SỐ LOẠI mặt hàng (COUNT DISTINCT product_code) — không phải tổng số lượng. */
  item_count: number;
  /** Tổng số lượng các loại cộng lại. */
  total_qty: number;
  /** Tổng tiền chiết khấu thương mại của phiếu (đ). */
  discount: number;
  /** Tổng thành tiền = giá trị SAU chiết khấu. */
  amount: number;
  note: string;
}

export interface JournalEntryRow {
  id: number;
  posting_date: string;
  voucher_no: string;
  entry_type: string;
  description: string;
  product_code: string;
  quantity: number;
  unit_price: number;
  amount: number;
  supplier_name: string | null;
  customer_name: string | null;
  industry_code: string;
  adjust_code: string;
  note: string;
}

// ─── Sổ cái / NKC ───

export interface LedgerRow {
  posting_date: string;
  voucher_no: string;
  entry_type: string;
  description: string;
  product_code: string;
  debit_account: string;
  credit_account: string;
  quantity: number;
  unit_price: number;
  amount: number;
}

// ─── Nhân sự & Bảng lương (Nhan Vien / Bang Luong) ───

export interface Employee {
  id: number;
  code: string;
  name: string;
  department: string;
  position: string;
  job: string;
  basic_salary: number;
  allowance_cv: number;
  allowance_xx: number;
  allowance_phone: number;
  bh_salary: number;
  hired_on: string;
  left_on: string;
  dependents: number;
}

export interface PayrollRow {
  period: string;
  employee_code: string;
  employee_name: string;
  department: string;
  work_days: number;
  bonus: number;
  advance: number;
  pit_amount: number;
  gross_salary: number;
  bh_employer: number;
  bh_employee: number;
  net_pay: number;
  note: string;
}

// ─── Audit log ───

export interface AuditEntry {
  id: number;
  ts: string;
  action: string;
  entity: string;
  detail: string;
  username?: string;
}

// ─── Người dùng / Phân quyền ───

export type Role = "admin" | "ketoan" | "kho" | "xem";

export interface CurrentUser {
  id: number;
  username: string;
  display_name: string;
  role: Role;
}

export interface AppUser {
  id: number;
  username: string;
  display_name: string;
  role: Role;
  active: boolean;
}

// ─── Hồ sơ HKD (multi-profile: mỗi hộ kinh doanh = 1 file DB riêng) ───

export interface Profile {
  key: string;
  name: string;
  created_at: string;
  active: boolean;
}

/** Tùy chọn đăng nhập theo hồ sơ (tên đăng nhập ghi nhớ + tự động đăng nhập). */
export interface ProfilePrefs {
  last_username?: string | null;
  auto_login: boolean;
  username?: string | null;
  password?: string | null;
}

// ─── Kết quả nhập khối NHAP LIEU ───

export interface ImportResult {
  ok: boolean;
  imported: number;
  skipped: number;
}

// ─── Hóa đơn ───

export interface Invoice {
  id: number;
  number: string;
  date: string;
  customer?: string;
  customer_tax_code?: string;
  total: number;
  vat_amount: number;
  /**
   * Vòng đời hóa đơn khi phát hành bằng tay qua dịch vụ khác:
   * draft (nháp) → exported (đã chép sang bên kia, chờ phát hành) →
   * official (đã có số HĐĐT); nhánh phụ adjusted (bên kia đã sửa, đã lập
   * HĐĐT điều chỉnh) và replaced (đã bị thay thế bởi hóa đơn khác).
   *
   * TT 91/2026/TT-BTC Điều 10: hóa đơn điện tử đã lập không được tự hủy —
   * sai sót thì điều chỉnh hoặc thay thế.
   */
  status: "draft" | "exported" | "pasted" | "official" | "cancelled" | "adjusted" | "replaced";
  e_invoice_no?: string;
  e_invoice_symbol?: string;
  e_invoice_date?: string;
  /** Số phiếu xuất nguồn khi hóa đơn được lập kèm phiếu xuất (rỗng nếu lập tay). */
  voucher_no?: string;
  /** Thời điểm bấm "Chép để xuất" sang dịch vụ khác. */
  exported_at?: string;
  /** Lý do bị thay thế (TT 91/2026 Điều 10). */
  replace_reason?: string;
  adjust_reason?: string;
  /** Số HĐĐT bên kia liên quan khi bị sửa, hoặc số hóa đơn nội bộ của HĐ thay thế. */
  ref_invoice?: string;
  /** Phiếu đảo doanh thu (Nợ 511 / Có 131) sinh kèm khi thay thế. */
  adjust_voucher_no?: string;
  /** Hóa đơn mà hóa đơn này thay thế (ngược lại với `ref_invoice`). */
  replaces_invoice_id?: number | null;
  items?: InvoiceItem[];
}

/** Một số hóa đơn bị dùng cho nhiều bản ghi — app báo để người dùng tự dọn. */
export interface InvoiceDuplicateNumber {
  number: string;
  count: number;
}

/** Kết quả thay thế hóa đơn: app trả về hóa đơn nháp mới để sửa rồi xuất lại. */
export interface InvoiceReplaceResult {
  ok: boolean;
  adjust_voucher_no: string;
  replacement_invoice_id: number;
  replacement_number: string;
}

/** Kết quả ghi nhận số HĐĐT — app đồng bộ ngày hóa đơn theo ngày HĐĐT khi
 *  hóa đơn không gắn phiếu xuất; nếu gắn phiếu xuất thì báo lệch kỳ. */
export interface InvoiceLinkResult {
  ok: boolean;
  symbol: string;
  date_synced: boolean;
  old_date: string;
  hddt_date: string;
  voucher_no: string;
  warning: string;
}

/** 1 dòng hàng trong gói chép sang dịch vụ HĐĐT khác. */
export interface InvoiceExportLine {
  stt: number;
  product_code: string;
  /** Tên hàng để dán sang bên kia (giữ nguyên, chỉ gộp khoảng trắng thừa). */
  product_name: string;
  unit: string;
  quantity: number;
  unit_price: number;
  discount: number;
  amount: number;
  industry_code: string;
  industry_name: string;
  vat_rate: number;
  /** Nhãn thuế sẵn để dán, vd "1% Phân phối, cung cấp hàng hóa". */
  vat_label: string;
  warnings: string[];
}

/** Một lỗi/cảnh báo trước khi chép. */
export interface InvoiceExportCheck {
  level: "error" | "warn" | "ok";
  message: string;
}

/** Gói dữ liệu chép sẵn (text thuần + TSV cho Excel + JSON). */
export interface InvoiceExportPack {
  header: {
    id: number;
    number: string;
    date: string;
    date_vn: string;
    customer: string;
    customer_tax_code: string;
    status: Invoice["status"];
    total: number;
    vat_amount: number;
    e_invoice_no: string;
    e_invoice_symbol: string;
    voucher_no: string;
    biz_name: string;
    biz_tax_code: string;
    biz_address: string;
    biz_phone: string;
  };
  lines: InvoiceExportLine[];
  checks: InvoiceExportCheck[];
  computed_total: number;
  /** Khối dán vào ô text tự do bên kia: mỗi dòng 1 dòng hàng, không kèm thuế suất. */
  paste: string;
  paste_layout: string;
  paste_sep: string;
  /** Nhãn các cột theo đúng thứ tự trong `paste`. */
  paste_columns: string[];
  text: string;
  json: unknown;
  export_count: number;
  /** Hóa đơn đã bị sửa sau lần chép gần nhất → cần sửa lại bên kia. */
  edited_after_export: boolean;
}

/** 1 dòng hàng chờ xuất: hóa đơn chưa phát hành + kết quả kiểm tra trước khi chép. */
export interface InvoiceQueueItem {
  invoice: Invoice;
  checks: InvoiceExportCheck[];
  error_count: number;
  warn_count: number;
  /** Đã chép sang bên kia ít nhất 1 lần. */
  copied: boolean;
  /** Đã chép xong mà hóa đơn bị sửa sau đó → phải sửa lại bên kia. */
  edited_after_export: boolean;
}

/** Một bước trong lịch sử trạng thái hóa đơn (đối chiếu lại với bên kia). */
export interface InvoiceEvent {
  id: number;
  at: string;
  from_status: string;
  to_status: string;
  reason: string;
  actor: string;
  ref: string;
}

export interface InvoiceItem {
  id: number;
  invoice_id: number;
  product_id?: number;
  product_code?: string;
  product_name?: string;
  unit?: string;
  quantity: number;
  unit_price: number;
  subtotal: number;
  /** Số tiền chiết khấu thương mại (đ) — giá trị dòng = Thành tiền − Tiền CK. */
  discount?: number;
  /** Kho xuất trên dòng (rỗng → kho mặc định của sản phẩm). */
  warehouse_code?: string;
  /** Nhóm ngành của dòng (cơ sở tỷ lệ thuế bán ra). */
  industry_code?: string;
  vat_rate?: number;
  pit_rate?: number;
  /**
   * Tên người dùng CHỌN trên dòng hóa đơn (có thể là tên khác / alias).
   * Rỗng → tên sản phẩm. In/chi tiết luôn giữ tên đã chọn (F5).
   */
  line_name?: string;
}

// ─── Kiểm kê ───
export interface InventoryCount {
  id: number;
  date: string;
  note?: string;
  items: InventoryCountItem[];
}

export interface InventoryCountItem {
  id: number;
  count_id: number;
  product_id: number;
  warehouse_id: number;
  counted_qty?: number;
  variance?: number;
}

export interface CountItemRow {
  id: number;
  count_id: number;
  product_code: string;
  product_name: string;
  warehouse_code: string;
  book_qty: number;
  counted_qty: number;
  variance: number;
}

// ─── Báo cáo ───

export interface RevenueExpenseRow {
  revenue_up: number;
  revenue_down: number;
  expense_up: number;
  expense_down: number;
  /** Giá vốn hàng đã xuất kho trong kỳ, tính theo FIFO (đơn vị: đồng). */
  cogs: number;
}

/**
 * Số bút toán giá vốn (Nợ 632 / Có 152) còn thiếu cho phiếu xuất lịch sử —
 * phiếu xuất trước khi app bắt đầu ghi giá vốn vẫn chưa có dòng 632/152 nên
 * bảng cân đối thiếu cột chi phí.
 */
export interface CogsBackfillPending {
  /** Số lượt xuất chưa có bút toán giá vốn. */
  count: number;
  /** Tổng giá vốn FIFO của các lượt đó (đơn vị: đồng). */
  amount: number;
}

/** Bảng cân đối số phát sinh — dư đầu kỳ + phát sinh + dư cuối kỳ theo tài khoản. */
export interface TrialBalanceRow {
  code: string;
  name: string;
  opening_debit: number;
  opening_credit: number;
  debit_mvmt: number;
  credit_mvmt: number;
  closing_debit: number;
  closing_credit: number;
}

export interface InventoryRow {
  product_code: string;
  product_name: string;
  unit: string;
  inbound: number;
  outbound: number;
  balance: number;
}

// ─── Cấu hình HKD ───

export interface BusinessConfig {
  id: number;
  name: string;
  tax_code: string;
  address: string;
  /**
   * Địa điểm kinh doanh ghi trên đầu sổ theo mẫu TT 152/2025 ("Địa điểm kinh
   * doanh: …"). Để trống thì sổ tự lấy `address` để không in ra dòng trống.
   */
  location: string;
  short_name: string;
  ownership: string;
  province: string;
  tax_code_issued_on: string;
  phone: string;
  email: string;
  /**
   * Ngày bắt đầu sử dụng hóa đơn điện tử (bắt buộc trong popup cấu hình HKD).
   * Mốc dùng để lấp hóa đơn mua vào từ cổng HĐĐT — nhập "Tất cả" nếu không nhớ.
   */
  hddt_start_date: string;
  /**
   * Ký hiệu (mẫu số) HĐĐT của hộ — tuỳ chọn, dùng làm mặc định khi liên kết HĐĐT
   * và được cập nhật theo lần dùng gần nhất.
   */
  hddt_symbol: string;
  /**
   * Nhóm hộ kinh doanh do người dùng xác nhận trong cấu hình HKD (1–4).
   * `null` = chưa chốt → app tự xếp theo tổng doanh thu cả năm.
   */
  tax_group: number | null;
}

/** Nhóm hộ hiện hành + doanh thu cả năm dùng để xếp nhóm. */
export interface TaxGroupInfo {
  /** Nhóm đang áp dụng (đã chốt trong hồ sơ, không thì tự xếp). */
  group: number;
  /** Nhóm app tự xếp từ doanh thu. */
  auto_group: number;
  /** Tổng doanh thu cả năm tính từ dữ liệu kế toán. */
  revenue_year: number;
  /** true = người dùng đã chốt trong hồ sơ HKD. */
  confirmed: boolean;
  /**
   * Nhóm app đã ghim cho năm khi hộ vượt ngưỡng mà hồ sơ chưa chốt — nhóm của
   * năm giữ nguyên, vượt mốc giữa năm không chuyển sang nhóm kế tiếp.
   */
  frozen_group?: number | null;
  /** Ba mốc doanh thu đang áp dụng (sửa được ở màn Cài đặt). */
  threshold_exempt: number;
  threshold_group3: number;
  threshold_group4: number;
  /** Nhóm đã chốt trong hồ sơ lệch với nhóm app xếp — chỉ để UI ghi chú. */
  mismatch?: boolean;
}

// ─── Cài đặt mặc định (bảng app_setting) ───

export interface AppSettings {
  product_code_prefix: string;
  product_code_start: number;
  product_code_digits: number;
  product_vat_rate_options: number[];
  product_vat_rate_default: number;
  product_import_tax_options: number[];
  product_import_tax_default: number;
  product_unit: string;
  product_min_stock: number;
  /** Kỳ khai thuế của hộ: "year" | "quarter" | "month" | "per_occurrence" */
  tax_period: string;
  /** Phương pháp tính thuế TNCN: "revenue" (theo doanh thu) | "profit" (theo lợi nhuận) */
  tax_method: string;
  /** Khấu trừ thuế GTGT đầu vào (hộ nộp thuế theo lợi nhuận mới được khấu trừ).
   *  Mặc định TẮT — theo doanh thu, giá nhập kho đã gồm thuế, không tách TK 133. */
  vat_deduct: boolean;
  /** Bố cục cột của khối dán sang máy tính tiền bên kia (xem hộp thoại chép hóa đơn). */
  invoice_paste_layout?: string;
  /** Dấu tách cột của khối dán: "\t" | "," | ";" | "|" */
  invoice_paste_sep?: string;
  /**
   * Mức trừ ngưỡng 01 tỷ phân bổ theo nhóm ngành, JSON
   * `{"PPHH": 800000000}` (NĐ 68/2026 Điều 4 khoản 3). Rỗng = app chia theo
   * tỷ lệ doanh thu.
   */
  pit_exempt_alloc?: string;
  /**
   * Ngưỡng doanh thu năm không phải nộp thuế GTGT và TNCN. Mặc định 1.000.000.000
   * (NĐ 141/2026/NĐ-CP nâng từ 500 triệu lên 01 tỷ, hiệu lực 01/01/2026).
   * Sửa được ở màn Cài đặt vì ngưỡng đã nhảy nhiều lần qua các văn bản.
   */
  tax_threshold_exempt: number;
  /** Mốc doanh thu năm của Nhóm 3 (thuế suất TNCN theo thu nhập 17%). Mặc định 3 tỷ. */
  tax_threshold_group3: number;
  /** Mốc doanh thu năm của Nhóm 4 (thuế suất TNCN theo thu nhập 20%). Mặc định 50 tỷ. */
  tax_threshold_group4: number;
  /**
   * Tự kiểm tra cập nhật ngay khi mở app (và lặp theo khoảng thời gian dưới).
   * Chỉ có tác dụng ở bản cài Tauri; lỗi mạng/không có bản mới thì im lặng.
   */
  auto_check_update: boolean;
  /**
   * Tự quét hóa đơn HĐĐT từ cổng khi mở app — chỉ kéo danh sách về cache,
   * việc **nhập kho** vẫn thao tác tay. Mặc định TẮT vì đây là gọi cổng thật.
   */
  auto_sync_enabled: boolean;
  /** Khoảng thời gian tự gọi lại (phút), áp dụng cho cả 2 việc trên. */
  auto_sync_interval_min: number;
}

// ─── Chấm công theo ngày (Cham Cong) ───

export type AttendanceStatus =
  | "X" // công đủ
  | "NC" // nửa công
  | "P" // phép
  | "H" // học tập
  | "CT" // công tác
  | "TS" // thai sản
  | "O" // ốm
  | "CO" // con ốm
  | "KL" // không lương
  | ""; // nghỉ / chưa chấm

export interface AttendanceRow {
  work_date: string; // YYYY-MM-DD
  employee_code: string;
  employee_name: string;
  status: string;
}

export interface AttendanceWorkDay {
  employee_code: string;
  employee_name: string;
  work_days: number;
}

// ─── Tờ khai thuế theo kỳ (To Khai Thue) ───

/** Doanh thu của **một nhóm ngành nghề** trong kỳ — nguồn của màn kê khai theo mẫu.
 *  Khác `TaxDeclarationRow`: không kèm số thuế, và trả cả nhóm chưa phát sinh
 *  (mẫu có 6 dòng cố định nên nhóm chưa có doanh thu vẫn phải lên bảng). */
export interface IndustryRevenue {
  industry_code: string;
  industry_name: string;
  vat_rate: number;
  pit_rate: number;
  /** Doanh thu tăng trong kỳ. */
  revenue_up: number;
  /** Doanh thu giảm trong kỳ (điều chỉnh). */
  revenue_down: number;
}

export interface TaxDeclarationRow {
  industry_code: string;
  industry_name: string;
  vat_rate: number;
  pit_rate: number;
  revenue_up: number; // DT tính thuế GTGT — tăng trong kỳ
  revenue_down: number; // DT tính thuế GTGT — giảm trong kỳ
  /**
   * Doanh thu tính thuế TNCN của kỳ = (DT tăng − DT giảm) − phần trừ mức ngưỡng
   * 01 tỷ (Luật TNCN 109/2025 Điều 7 khoản 3 điểm a). Trước kỳ vượt ngưỡng thì 0.
   */
  revenue_taxable: number;
  /**
   * Mức trừ ngưỡng TNCN đã khấu cho nhóm ngành này trong kỳ (lũy kế các kỳ trước +
   * kỳ này). Tờ khai ghi rõ để thấy doanh thu tính thuế đã trừ được bao nhiêu và vì
   * sao còn lại là bao nhiêu.
   */
  pit_deduction: number;
  vat_tax: number; // Số thuế GTGT = doanh thu × tỷ lệ ngành
  vat_payable: number; // Thuế GTGT phải nộp
  pit_tax: number; // Số thuế TNCN phải nộp (theo tỷ lệ doanh thu — nhóm 2 p.pháp doanh thu)
}

/** Một khoản chi bị loại khỏi chi phí khi tính thu nhập tính thuế (NĐ 68/2026 Điều 6). */
export interface TaxCostWarning {
  posting_date: string;
  voucher_no: string;
  description: string;
  amount: number;
  reason: string;
}

/** Một kỳ trong bảng tạm nộp TNCN. */
export interface TaxSettlementPeriod {
  period: string;
  no: number;
  label: string;
  revenue: number;
  taxable_revenue: number;
  pit_provisional: number;
  deadline: string | null;
}

/**
 * Tạm nộp + quyết toán thu nhập cá nhân theo năm.
 * Hộ nộp theo thuế suất × doanh thu thì khai theo kỳ, không có quyết toán năm
 * riêng (`by_revenue = true`, `difference = 0`).
 */
export interface TaxSettlement {
  year: number;
  group: number;
  method: string;
  by_revenue: boolean;
  period_kind: string;
  periods: TaxSettlementPeriod[];
  provisional_total: number;
  year_revenue: number;
  cogs: number;
  other_expense: number;
  non_deductible: number;
  expense: number;
  taxable_income: number;
  final_rate: number;
  final_tax: number;
  /** Quyết toán − tạm nộp: > 0 phải nộp bổ sung, < 0 xử lý nộp thừa. */
  difference: number;
  settle_deadline: string;
}

/**
 * Một ô của sổ: số, chữ, hoặc `null` (ô trống của mẫu).
 *
 * Backend trả về `null` thay vì chuỗi rỗng cho ô không có dữ liệu — phân biệt
 * "ô trống trong mẫu" với "ô có chữ rỗng" để không in ra khung trống vô nghĩa.
 */
export type BookCell = number | string | null;

/** Một cột của mẫu sổ. `key` là tên ô mà `BookRow.cells` dùng. */
export interface BookColumn {
  key: string;
  /** Tiêu đề kèm số thứ tự cột của mẫu gốc, ví dụ `1 · Số tiền`. */
  label: string;
  kind: "text" | "money" | "qty" | "rate";
  width: string;
}

/**
 * Một dòng của sổ.
 *
 * `kind` quyết định cách in: `detail` (ghi chép), `section` (tiêu đề nhóm, ví dụ
 * từng khối tiền trong S2e), `subtotal` (tổng nhóm), `total` (tổng cuối sổ).
 */
export interface BookRow {
  kind: "detail" | "section" | "subtotal" | "total";
  cells: Record<string, BookCell>;
}

/** Đầu sổ và chữ ký cuối sổ — giữ đúng bố cục mẫu gốc. */
export interface BookHeader {
  owner: string;
  address: string;
  tax_code: string;
  /** Giá trị dòng dưới tiêu đề; không có dòng này thì để rỗng. */
  location: string;
  /** Nhãn của dòng đó; rỗng = mẫu không có dòng này (S2e). */
  location_label: string;
  period: string;
  /** Khối "Mẫu số …-HKD (Kèm theo Thông tư số 152/2025/TT-BTC …)" góc phải. */
  form_ref: string;
  unit: string;
  sign_date: string;
  signer: string;
}

/**
 * Ghi chú dưới sổ. `action_book` có mặt thì hiện nút chuyển sang mẫu khác —
 * dùng cho cảnh báo chọn nhầm mẫu (ví dụ chọn S2a khi hồ sơ tính TNCN theo
 * thu nhập).
 */
export interface BookNote {
  level: "info" | "warn";
  text: string;
  action_book?: string;
  action_label?: string;
}

/**
 * Nội dung 1 mẫu sổ kế toán theo TT 152/2025/TT-BTC.
 *
 * Bộ cột khác nhau theo từng mẫu (S1a 3 cột, S2d 12 cột, S3a 12 cột…) nên
 * cấu trúc này mang theo mô tả cột thay vì một shape cứng.
 */
export interface TaxBook {
  book: string;
  title: string;
  period_label: string;
  header: BookHeader;
  columns: BookColumn[];
  rows: BookRow[];
  notes: BookNote[];
}

/** Tổng hợp thuế phải nộp theo NĐ 68/2026 + NĐ 141/2026 (Kế toán hộ KD). */
export interface TaxOverview {
  year: number;
  /** Tổng doanh thu cả năm — cơ sở xếp nhóm hộ */
  year_revenue: number;
  /** Nhóm hộ: 1 = ≤ 1 tỷ (miễn thuế), 2 = > 1–3 tỷ, 3 = > 3–50 tỷ, 4 = > 50 tỷ */
  group: number;
  /** Nhóm app tự xếp từ doanh thu cả năm (để so với nhóm đang áp dụng). */
  auto_group?: number;
  /** true = nhóm đang dùng lấy từ hồ sơ HKD (đã chốt, không tự xếp lại). */
  group_from_profile?: boolean;
  /** Nhóm app đã ghim cho năm này (đã vượt ngưỡng mà hồ sơ chưa chốt nhóm). */
  frozen_group?: number | null;
  /**
   * true = hồ sơ HKD chốt Nhóm 2/3/4: hộ thuộc diện nộp thuế suốt năm, mọi kỳ có
   * doanh thu đều phải nộp — không chờ doanh thu vượt ngưỡng.
   */
  taxed_from_start?: boolean;
  /** Phương pháp TNCN đang áp dụng: "revenue" | "profit" */
  method: string;
  period_revenue: number; // doanh thu kỳ khai
  /** Doanh thu tính thuế TNCN của kỳ (đã trừ phần mức ngưỡng 01 tỷ của năm). */
  taxable_revenue: number;
  /** Tổng mức trừ đã dùng trong năm = min(ngưỡng, doanh thu cả năm). */
  exempt_used?: number;
  /** Hạn ngạch mức trừ còn lại = ngưỡng − đã dùng. */
  exempt_remaining?: number;
  /** Mức trừ đã khấu cho riêng kỳ đang xem. */
  exempt_period?: number;
  /** Mức trừ ngưỡng áp dụng cho năm (01 tỷ đồng — NĐ 141/2026/NĐ-CP). */
  exempt_threshold: number;
  /** Mốc Nhóm 3 / Nhóm 4 đang áp dụng (sửa được ở màn Cài đặt). */
  group3_threshold?: number;
  group4_threshold?: number;
  /**
   * false = doanh thu lũy kế tới cuối kỳ vẫn chưa vượt mức ngưỡng 01 tỷ nên kỳ
   * này chưa phát sinh thuế — khai, nộp thuế kể từ kỳ phát sinh (Điều 8 khoản 1a
   * NĐ 68/2026), kể cả khi hộ đã thuộc nhóm 2 vì nhóm xếp theo cả năm.
   */
  taxable_period?: boolean;
  /** Giá vốn FIFO tính từ phiếu nhập. */
  cogs: number;
  /** Khoản chi khác được trừ (phiếu chi đủ chứng từ). */
  other_expense: number;
  expense: number; // tổng chi phí được trừ = cogs + other_expense
  /** Tổng khoản chi KHÔNG được trừ (Điều 6 NĐ 68/2026). */
  non_deductible: number;
  /** Các khoản chi bị loại, để người dùng bổ sung chứng từ. */
  cost_warnings: TaxCostWarning[];
  profit: number; // thu nhập tính thuế = DT − CP
  profit_rate: number; // thuế suất TNCN theo lợi nhuận (15/17/20%) hoặc 0
  vat_payable: number;
  pit_tax: number;
  /** Số TNCN tạm nộp theo tỷ lệ % × doanh thu kỳ (Điều 10 khoản 2b). */
  pit_provisional: number;
  /** Hạn nộp hồ sơ khai thuế của kỳ (dd/mm/yyyy). */
  deadline: string | null;
  total_tax: number;
}

// ─── HĐĐT (cổng hoadondientu.gdt.gov.vn) ───

/** Tài khoản HĐĐT đã lưu (mật khẩu KHÔNG bao giờ trả về frontend). */
export interface HddtConfig {
  username: string;
  base_url: string;
  has_password: boolean;
  configured: boolean;
  /** Khi `true`, app sẽ tự đổi mật khẩu trước khi hết hạn (≤ 7 ngày). */
  auto_change_password: boolean;
}

/** Trạng thái kết nối cổng HĐĐT. */
export interface HddtStatus {
  configured: boolean;
  username: string;
  logged_in: boolean;
  /** Unix seconds — 0 nếu chưa đăng nhập. */
  expires_at: number;
  /** Số giây còn lại của token (âm = đã hết hạn). */
  seconds_left: number;
  /** Ngưỡng tự đăng nhập lại trước khi token hết hạn (giây). */
  relogin_margin: number;
  /** Hồ sơ người nộp thuế trả về từ cổng (null nếu chưa có). */
  user: unknown | null;
  /** Cài đặt `auto_change_password` hiện tại. */
  auto_change_password: boolean;
}

/** Kết quả đăng nhập HĐĐT. `need_manual` = phải nhập captcha tay. */
export interface HddtLoginResult {
  ok: boolean;
  need_manual: boolean;
  reason?: string;
  status?: HddtStatus;
  /** Đã tự đổi mật khẩu thành công (mật khẩu mới đi kèm). */
  password_rotated?: boolean;
  /** Mật khẩu mới do hệ thống sinh — chỉ có khi `password_rotated` = true. */
  new_password?: string;
}

/** Trạng thái hóa đơn (`tthai`) — select "Trạng thái hóa đơn" của trang tra cứu. */
export const HDDT_TTHAI: Record<number, string> = {
  0: "Tất cả",
  1: "Hóa đơn mới",
  2: "Hóa đơn thay thế",
  3: "Hóa đơn điều chỉnh",
  4: "Hóa đơn đã bị thay thế",
  5: "Hóa đơn đã bị điều chỉnh",
  6: "Hóa đơn đã bị hủy",
};

/** Kết quả kiểm tra / trạng thái xử lý (`ttxly`) của trang tra cứu HĐĐT. */
export const HDDT_TTXLY: Record<number, string> = {
  0: "Lưu tạm",
  1: "Chờ phê duyệt",
  2: "Đã phê duyệt",
  3: "Từ chối phê duyệt",
  4: "Đã gửi ký remote",
  5: "Đã cấp mã hóa đơn",
  6: "Ký remote không thành công",
};

/** Giá trị "Tất cả" của select "Kết quả kiểm tra" (portal không gửi lên). */
export const HDDT_TTXLY_ALL = "-1";

/** Mẫu số hóa đơn (`khmshdon`) — lấy từ `GET /api/category/dmhdons` (24/09/2026). */
export const HDDT_KHMSHDON: Record<number, string> = {
  1: "Hóa đơn giá trị gia tăng",
  2: "Hóa đơn bán hàng",
  3: "Hóa đơn bán tài sản công",
  4: "Hóa đơn bán hàng dự trữ quốc gia",
  5: "Hóa đơn khác",
  6: "Phiếu xuất kho kiêm vận chuyển nội bộ",
  7: "Hóa đơn thương mại điện tử",
  8: "HĐ giá trị gia tăng tích hợp biên lai thuế, phí, lệ phí",
  9: "HĐ bán hàng tích hợp biên lai thuế, phí, lệ phí",
};

/** Tab lớn của trang "Tra cứu hóa đơn": hóa đơn ra (bán ra) / hóa đơn vào (mua vào). */
export type HddtInvoiceDirection = "sold" | "purchase";

/**
 * Tab nhỏ (loại hóa đơn) — quyết định tiền tố endpoint:
 * `query` = "Hóa đơn điện tử", `sco-query` = "Hóa đơn có mã khởi tạo từ máy tính tiền".
 */
export type HddtInvoiceKind = "regular" | "cash-register";

/** Một dòng hóa đơn từ `GET /api/{query|sco-query}/invoices/{sold|purchase}`. */
export interface HddtInvoiceRow {
  id?: string | number;
  /** Mã số thuế người bán hóa đơn. */
  nbmst?: string;
  /** Ký hiệu mẫu số (1..9 — xem {@link HDDT_KHMSHDON}). */
  khmshdon?: number;
  /** Ký hiệu hóa đơn. */
  khhdon?: string;
  /** Số hóa đơn. */
  shdon?: string | number;
  /** Ngày lập hóa đơn (`tdlap`, định dạng ISO từ cổng). */
  tdlap?: string;
  /** MST người mua/nhận hàng. */
  nmmst?: string;
  /** Tên người bán hóa đơn (chỉ có ý nghĩa ở danh sách hóa đơn vào). */
  nbten?: string;
  /** Tên NNT đối tác (người mua với HĐ ra, người bán với HĐ vào). */
  nmten?: string;
  /** Tên người mua (HĐ ra) / người nhận hàng (HĐ vào). */
  nmtnmua?: string;
  /** CCCD người mua. */
  nmcccd?: string;
  /** Tổng tiền chưa thuế. */
  tgtcthue?: number | string;
  /** Tổng tiền thuế. */
  tgtthue?: number | string;
  /** Tổng tiền chiết khấu thương mại. */
  ttcktmai?: number | string;
  /** Tổng tiền phí. */
  tgtphi?: number | string;
  /** Tổng tiền thanh toán. */
  tgtttbso?: number | string;
  /** Đơn vị tiền tệ (VD: VND). */
  dvtte?: string;
  /** Tính chất hóa đơn (3 = điều chỉnh — cổng tô đỏ số tiền). */
  tchat?: number;
  /** Mã loại hóa đơn trong danh mục (`hdon`: "01", "02", …). */
  hdon?: string;
  /** Hồ sơ gốc liên quan (hóa đơn thay thế/điều chỉnh). */
  hsgoc?: string | null;
  /** Trạng thái hóa đơn (0..6 — xem {@link HDDT_TTHAI}). */
  tthai?: number;
  /** Kết quả kiểm tra (0..6 — xem {@link HDDT_TTXLY}). */
  ttxly?: number;
  [k: string]: unknown;
}

/** Kết quả tra cứu hóa đơn — envelope `{datas, state, total, time}`. */
export interface HddtInvoiceList {
  datas: HddtInvoiceRow[];
  /** Cursor phân trang — truyền lại làm `state=` cho trang kế tiếp. */
  state: string | null;
  total: number;
  time: number;
}

/** Bộ lọc tra cứu hóa đơn (khớp form trang `/tra-cuu/tra-cuu-hoa-don` của cổng). */
export interface HddtInvoiceFilter {
  /** Tab lớn: hóa đơn ra (bán ra) hay hóa đơn vào (mua vào). */
  direction?: HddtInvoiceDirection;
  /** Tab nhỏ: hóa đơn điện tử hay máy tính tiền. */
  kind?: HddtInvoiceKind;
  /** Ngày lập từ ngày (dd/MM/yyyy) → `tdlap=ge=`. */
  from?: string;
  /** Ngày lập đến ngày (dd/MM/yyyy) → `tdlap=le=`. */
  to?: string;
  /** Cursor phân trang từ `HddtInvoiceList.state`. */
  state?: string | null;
  /** Mã số thuế (`nmmst`). */
  nmmst?: string;
  /** MST đối tác (`nbmst`) — người mua (HĐ ra) / người bán (HĐ vào). */
  nbmst?: string;
  /** CCCD người mua (`nmcmnd`). */
  nmcmnd?: string;
  /** Trạng thái hóa đơn (`tthai`, 0 = Tất cả → không gửi). */
  tthai?: string;
  /** Kết quả kiểm tra (`ttxly`, "-1" = Tất cả → không gửi). */
  ttxly?: string;
  /** Ký hiệu mẫu số hóa đơn (`khmshdon`, 1..9). */
  khmshdon?: string;
  /** Ký hiệu hóa đơn (`khhdon`). */
  khhdon?: string;
  /** Số hóa đơn (`shdon`). */
  shdon?: string;
  /** Hóa đơn ủy nhiệm (`unhiem==1`). */
  unhiem?: boolean;
  /** Số dòng mỗi trang. */
  size?: number;
}

// ─── Đồng bộ hóa đơn mua (tab trong HĐĐT) ───

/** Kết quả quét cổng: gọi cổng + cache theo ngày. */
export interface HddtSyncScanSummary {
  /** Số ngày thực sự gọi cổng (không tính ngày đã cache). */
  days_scanned: number;
  days_cached: number;
  /** Hóa đơn mới ghi vào cache. */
  invoices_new: number;
  invoices_existing: number;
  /** Hóa đơn lấy được dòng hàng. */
  details_ok: number;
  /** Không lấy được dòng hàng → KHÔNG tạo phiếu, cần thử lại. */
  details_failed: number;
  /** Hóa đơn cần người xử lý (thay thế/điều chỉnh/hủy). */
  need_manual: number;
  /** Hóa đơn lỗi chi tiết lượt trước được thử lại lượt này. */
  details_retried: number;
  /** Đã quét hôm nay (luôn quét lại, không cache). */
  days_today: number;
  /** File XML hóa đơn tự động lưu cùng lượt quét (best effort). */
  xml?: HddtXmlFill;
}

/** 1 dòng hóa đơn mua trong bảng xem trước (đọc từ cache, không gọi cổng). */
export interface HddtSyncRow {
  id: number;
  portal_id: string;
  posting_date: string;
  nbmst: string;
  nbten: string;
  khhdon: string;
  shdon: string;
  /** Mã mẫu số — cần cho nút "Tải XML" (endpoint export-xml). */
  khmshdon: number;
  /** regular = hóa đơn điện tử, cash-register = máy tính tiền. */
  portal_kind: string;
  /** pending = chờ nhập kho · imported = đã có phiếu · manual = cần xử lý. */
  status: "pending" | "imported" | "manual";
  skip_reason: string;
  line_count: number;
  new_product_count: number;
  tgtcthue: number;
  tgtthue: number;
  /** Tổng tiền chiết khấu thương mại trên hóa đơn (cổng trả về `ttcktmai`). */
  ttcktmai: number;
  tgtttbso: number;
  voucher_no: string;
  detail_error: string;
}

export interface HddtSyncPreview {
  rows: HddtSyncRow[];
  summary: {
    total: number;
    pending: number;
    imported: number;
    manual: number;
    new_products: number;
    total_value: number;
    days_cached: number;
  };
}

/** Kết quả 1 hóa đơn sau khi nhập kho. */
export interface HddtSyncImportResult {
  portal_id: string;
  ok: boolean;
  message: string;
  voucher_no: string;
  products_created: number;
}

export interface HddtSyncImport {
  imported: number;
  failed: number;
  results: HddtSyncImportResult[];
  /** File XML hóa đơn tự động lưu trước lúc nhập (best effort). */
  xml?: HddtXmlFill;
}

// ─── Lưu file XML hóa đơn điện tử (quy định lưu trữ HĐĐT) ───

/** Số liệu tự động lưu XML chạy cùng lúc quét/nhập (best effort — lỗi từng
 *  hóa đơn không làm hỏng luồng chính, chỉ báo ra UI). */
export interface HddtXmlFill {
  /** Đã có file từ trước — lượt này không đụng tới. */
  skipped: number;
  /** Tải + lưu mới trong lượt này. */
  saved: number;
  /** Lỗi (cổng chưa có hồ sơ gốc, hết phiên, mạng…). */
  failed: number;
  /** Lỗi đầu tiên — dùng cho tin nhắn cảnh báo. */
  first_error: string;
}

/** Kết quả lệnh "Tải XML" 1 hóa đơn. */
export interface HddtSaveXmlResult {
  /** Tên file trong `…/profiles/<key>/hddt_xml/`. */
  file_name: string;
  byte_size: number;
  /** true = đã lưu từ trước → lượt này không tải lại. */
  already: boolean;
}

/** 1 file XML đã lưu (dòng trả từ `hddt_list_xml`). */
export interface HddtSavedXml {
  /** Id trong `hddt_invoice_xml` — gửi lại cho `hddt_export_xml_zip` để gộp ZIP. */
  id: number;
  portal_id: string;
  direction: "sold" | "purchase";
  kind: "regular" | "cash-register";
  nbmst: string;
  khmshdon: number;
  khhdon: string;
  shdon: string;
  file_name: string;
  byte_size: number;
  /** auto = tải lúc quét/nhập · manual = người dùng bấm nút. */
  source: string;
  saved_at: string;
}

/** Kết quả xoá cache đồng bộ. */
export interface HddtSyncClearCache {
  /** Số hóa đơn chưa nhập kho đã bị xoá. */
  invoicesDeleted: number;
  /** Số dấu ngày đã quét đã bị xoá. */
  daysDeleted: number;
}

// ─── Chi tiết chứng từ (in PNK 01-VT / PXK 02-VT) ───

export interface VoucherRow {
  posting_date: string;
  voucher_no: string;
  doc_date: string;
  entry_type: string;
  description: string;
  product_code: string;
  product_name: string;
  unit: string;
  supplier_code: string;
  supplier_name: string;
  customer_code: string;
  customer_name: string;
  quantity: number;
  unit_price: number;
  /** Thành tiền SAU chiết khấu (đã hạch toán). */
  amount: number;
  /** Tiền chiết khấu của dòng (0 nếu không CK) — cộng lại với `amount` ra thành tiền GỐC. */
  discount: number;
  debit_account: string;
  credit_account: string;
  industry_code: string;
  note: string;
}

// ─── Tiến độ của việc chạy lâu (xuất báo cáo kỳ thuế) ───

/**
 * Ảnh chụp tiến độ backend giữ trong RAM — frontend hỏi lại bằng
 * `getJobProgress` mỗi vài trăm mili giây cho tới khi `finished`.
 *
 * Hỏi (poll) chứ không bắn sự kiện: app chạy cả ở cửa sổ Tauri lẫn web server
 * mở bằng trình duyệt, mà sự kiện Tauri không tới được trình duyệt.
 */
export interface JobProgress {
  /** UUID frontend sinh — lệch là backend trả `null` (không đọc nhầm việc khác). */
  id: string;
  label: string;
  /** Việc đang làm ngay lúc này (1 dòng). */
  step: string;
  /** Đã xong / tổng số việc đã biết. `total = 0` = chưa đếm được → progressbar chạy vòng. */
  done: number;
  total: number;
  finished: boolean;
  error: string | null;
  /** Nhật ký, cũ nhất trước — khung log tự cuộn xuống cuối. */
  log: string[];
}
