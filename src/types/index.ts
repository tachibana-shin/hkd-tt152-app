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
  note: string;
}