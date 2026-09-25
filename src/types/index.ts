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
  status: "draft" | "pasted" | "official";
  e_invoice_no?: string;
  e_invoice_symbol?: string;
  e_invoice_date?: string;
  /** Số phiếu xuất nguồn khi hóa đơn được lập kèm phiếu xuất (rỗng nếu lập tay). */
  voucher_no?: string;
  items?: InvoiceItem[];
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

export interface TaxSummaryRow {
  industry_code: string;
  industry_name: string;
  vat_rate: number;
  pit_rate: number;
  revenue_up: number;
  revenue_down: number;
  vat_tax: number;
  pit_tax: number;
}

export interface RevenueExpenseRow {
  revenue_up: number;
  revenue_down: number;
  expense_up: number;
  expense_down: number;
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

export interface TaxDeclarationRow {
  industry_code: string;
  industry_name: string;
  vat_rate: number;
  pit_rate: number;
  revenue_up: number; // DT tính thuế GTGT — tăng trong kỳ
  revenue_down: number; // DT tính thuế GTGT — giảm trong kỳ
  vat_tax: number; // Số thuế GTGT = doanh thu × tỷ lệ ngành
  vat_payable: number; // Thuế GTGT phải nộp
  pit_tax: number; // Số thuế TNCN phải nộp (theo tỷ lệ doanh thu — nhóm 2 p.pháp doanh thu)
}

/** Tổng hợp thuế phải nộp theo NĐ 68/2026 + NĐ 141/2026 (Kế toán hộ KD). */
export interface TaxOverview {
  year: number;
  /** Tổng doanh thu cả năm — cơ sở xếp nhóm hộ */
  year_revenue: number;
  /** Nhóm hộ: 1 = ≤ 1 tỷ (miễn thuế), 2 = > 1–3 tỷ, 3 = > 3–50 tỷ, 4 = > 50 tỷ */
  group: number;
  /** Phương pháp TNCN đang áp dụng: "revenue" | "profit" */
  method: string;
  period_revenue: number; // doanh thu kỳ khai
  expense: number; // chi phí kỳ khai
  profit: number; // lợi nhuận kỳ = DT − CP
  profit_rate: number; // thuế suất TNCN theo lợi nhuận (15/17/20%) hoặc 0
  vat_payable: number;
  pit_tax: number;
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
  /** regular = hóa đơn điện tử, cash-register = máy tính tiền. */
  portal_kind: string;
  /** pending = chờ nhập kho · imported = đã có phiếu · manual = cần xử lý. */
  status: "pending" | "imported" | "manual";
  skip_reason: string;
  line_count: number;
  new_product_count: number;
  tgtcthue: number;
  tgtthue: number;
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
  amount: number;
  debit_account: string;
  credit_account: string;
  industry_code: string;
  note: string;
}
