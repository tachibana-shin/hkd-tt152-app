#![allow(unused_imports)]

use sqlx::SqlitePool;
use tauri::Manager;
pub(crate) struct AppState {
    /// Pool SQLite của HKD đang mở — dùng `RwLock` để có thể ĐỔI HỒ SƠ (đổi file DB)
    /// khi đang chạy: `switch_profile` ghi bản `write()` mới, mọi lệnh đọc qua `read()`.
    pub(crate) pool: tokio::sync::RwLock<SqlitePool>,
    /// Người dùng đang đăng nhập (None nếu chưa đăng nhập — buộc vào màn hình login)
    pub(crate) current_user: tokio::sync::Mutex<Option<CurrentUser>>,
}

// ─── NGƯỜI DÙNG / PHÂN QUYỀN ───
// Vai trò: admin (toàn quyền) | ketoan (kế toán/thuế/lương) | kho (kho/hàng hóa) | xem (chỉ đọc)

#[derive(sqlx::FromRow, serde::Serialize, Clone)]
pub(crate) struct CurrentUser {
    pub(crate) id: i64,
    pub(crate) username: String,
    pub(crate) display_name: String,
    pub(crate) role: String,
}

#[derive(sqlx::FromRow, serde::Serialize)]
pub(crate) struct UserRow {
    pub(crate) id: Option<i64>,
    pub(crate) username: String,
    pub(crate) display_name: String,
    pub(crate) role: String,
    pub(crate) active: bool,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UserInput {
    pub(crate) username: String,
    pub(crate) display_name: String,
    pub(crate) password: String, // rỗng = giữ nguyên mật khẩu cũ (khi sửa)
    pub(crate) role: String,
    pub(crate) active: bool,
}

#[derive(sqlx::FromRow, serde::Serialize)]
pub(crate) struct BusinessInfo {
    pub(crate) name: String,
}

#[derive(sqlx::FromRow, serde::Serialize)]
pub(crate) struct BusinessConfigRow {
    pub(crate) name: String,
    pub(crate) tax_code: String,
    pub(crate) address: String,
    pub(crate) short_name: String,
    pub(crate) ownership: String,
    pub(crate) province: String,
    pub(crate) tax_code_issued_on: String,
    pub(crate) phone: String,
    pub(crate) email: String,
    pub(crate) fiscal_year: i64,
    pub(crate) report_from: String,
    pub(crate) report_to: String,
}

#[derive(sqlx::FromRow, serde::Serialize)]
pub(crate) struct AccountRow {
    pub(crate) id: Option<i64>,
    pub(crate) code: String,
    pub(crate) name: String,
    pub(crate) opening_debit: f64,
    pub(crate) opening_credit: f64,
}

#[derive(sqlx::FromRow, serde::Serialize)]
pub(crate) struct ProductRow {
    pub(crate) id: Option<i64>,
    pub(crate) code: String,
    pub(crate) name: String,
    pub(crate) unit: String,
    pub(crate) sale_price: f64,
    pub(crate) cost_price: f64,
    pub(crate) min_stock: f64,
    pub(crate) vat_rate: f64,
    pub(crate) vat_reduced: bool,
}

#[derive(sqlx::FromRow, serde::Serialize)]
pub(crate) struct IndustryGroupRow {
    pub(crate) code: String,
    pub(crate) name: String,
    pub(crate) vat_rate: f64,
    pub(crate) pit_rate: f64,
}

#[derive(sqlx::FromRow, serde::Serialize)]
pub(crate) struct WarehouseRow {
    pub(crate) id: Option<i64>,
    pub(crate) code: String,
    pub(crate) name: String,
}

#[derive(sqlx::FromRow, serde::Serialize)]
pub(crate) struct SupplierRow {
    pub(crate) id: Option<i64>,
    pub(crate) code: String,
    pub(crate) name: String,
    pub(crate) address: String,
    pub(crate) tax_code: String,
    pub(crate) phone: String,
}

#[derive(sqlx::FromRow, serde::Serialize)]
pub(crate) struct CustomerRow {
    pub(crate) id: Option<i64>,
    pub(crate) code: String,
    pub(crate) name: String,
    pub(crate) address: String,
    pub(crate) tax_code: String,
    pub(crate) phone: String,
}

#[derive(sqlx::FromRow, serde::Serialize)]
pub(crate) struct TaxSummaryRow {
    pub(crate) industry_code: String,
    pub(crate) industry_name: String,
    pub(crate) vat_rate: f64,
    pub(crate) pit_rate: f64,
    pub(crate) revenue_up: f64,
    pub(crate) revenue_down: f64,
    pub(crate) vat_tax: f64,
    pub(crate) pit_tax: f64,
}

#[derive(sqlx::FromRow, serde::Serialize)]
pub(crate) struct RevenueExpenseRow {
    pub(crate) revenue_up: f64,
    pub(crate) revenue_down: f64,
    pub(crate) expense_up: f64,
    pub(crate) expense_down: f64,
}

#[derive(sqlx::FromRow, serde::Serialize)]
pub(crate) struct InventoryRow {
    pub(crate) product_code: String,
    pub(crate) product_name: String,
    pub(crate) unit: String,
    pub(crate) inbound: f64,
    pub(crate) outbound: f64,
    pub(crate) balance: f64,
}

#[derive(sqlx::FromRow, serde::Serialize)]
pub(crate) struct LedgerRow {
    pub(crate) posting_date: String,
    pub(crate) voucher_no: String,
    pub(crate) entry_type: String,
    pub(crate) description: String,
    pub(crate) product_code: String,
    pub(crate) debit_account: String,
    pub(crate) credit_account: String,
    pub(crate) quantity: f64,
    pub(crate) unit_price: f64,
    pub(crate) amount: f64,
}

#[derive(sqlx::FromRow, serde::Serialize)]
pub(crate) struct StockLotRow {
    pub(crate) id: Option<i64>,
    pub(crate) product_code: String,
    pub(crate) product_name: String,
    pub(crate) warehouse_code: String,
    pub(crate) quantity: f64,
    pub(crate) unit_cost: f64,
    pub(crate) received_at: String,
    pub(crate) depleted: bool,
}

#[derive(sqlx::FromRow, serde::Serialize)]
pub(crate) struct JournalEntryRow {
    pub(crate) id: Option<i64>,
    pub(crate) posting_date: String,
    pub(crate) voucher_no: String,
    pub(crate) entry_type: String,
    pub(crate) description: String,
    pub(crate) product_code: String,
    pub(crate) quantity: f64,
    pub(crate) unit_price: f64,
    pub(crate) amount: f64,
    pub(crate) supplier_name: Option<String>,
    pub(crate) customer_name: Option<String>,
    pub(crate) industry_code: String,
    pub(crate) adjust_code: String,
    pub(crate) note: String,
}

#[derive(sqlx::FromRow, serde::Serialize)]
pub(crate) struct InvoiceRow {
    pub(crate) id: Option<i64>,
    pub(crate) number: String,
    pub(crate) date: String,
    pub(crate) customer: String,
    pub(crate) customer_tax_code: String,
    pub(crate) total: f64,
    pub(crate) vat_amount: f64,
    pub(crate) status: String,
    pub(crate) e_invoice_no: String,
    pub(crate) e_invoice_symbol: String,
    pub(crate) e_invoice_date: String,
}

#[derive(sqlx::FromRow, serde::Serialize)]
pub(crate) struct InvoiceItemRow {
    pub(crate) id: Option<i64>,
    pub(crate) invoice_id: i64,
    pub(crate) product_code: String,
    pub(crate) product_name: String,
    pub(crate) unit: String,
    pub(crate) quantity: f64,
    pub(crate) unit_price: f64,
    pub(crate) subtotal: f64,
}

#[derive(sqlx::FromRow, serde::Serialize)]
pub(crate) struct InventoryCountRow {
    pub(crate) id: Option<i64>,
    pub(crate) date: String,
    pub(crate) note: String,
}

#[derive(sqlx::FromRow, serde::Serialize)]
pub(crate) struct CountItemRow {
    pub(crate) id: Option<i64>,
    pub(crate) count_id: i64,
    pub(crate) product_code: String,
    pub(crate) product_name: String,
    pub(crate) warehouse_code: String,
    pub(crate) book_qty: f64,
    pub(crate) counted_qty: f64,
    pub(crate) variance: f64,
}

#[derive(serde::Deserialize)]
pub(crate) struct InboundItemInput {
    pub(crate) product_code: String,
    pub(crate) quantity: f64,
    pub(crate) unit_price: f64,
}

#[derive(serde::Deserialize)]
pub(crate) struct OutboundItemInput {
    pub(crate) product_code: String,
    pub(crate) quantity: f64,
    pub(crate) unit_price: f64,
    pub(crate) industry_code: String,
}

#[derive(serde::Deserialize)]
pub(crate) struct InvoiceItemInput {
    pub(crate) product_code: String,
    pub(crate) quantity: f64,
    pub(crate) unit_price: f64,
}

#[derive(serde::Deserialize)]
pub(crate) struct CountItemInput {
    pub(crate) product_code: String,
    pub(crate) warehouse_code: String,
    pub(crate) counted_qty: f64,
}

/// Phiếu thu (PT) / phiếu chi (PC) — 1 bút toán tiền (ít nhất 1 dòng).
/// Không tạo stock_lot như PNK/PXK; chỉ ghi vào journal_entry.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CashEntryInput {
    pub(crate) entry_type: String, // "PT" | "PC"
    pub(crate) posting_date: String,
    pub(crate) voucher_no: String,
    pub(crate) description: String,
    pub(crate) customer_code: String,
    pub(crate) supplier_code: String,
    pub(crate) amount: f64,
    pub(crate) debit_account: String,
    pub(crate) credit_account: String,
    pub(crate) industry_code: String,
    pub(crate) vat_rate: f64,
    pub(crate) pit_rate: f64,
    pub(crate) unit_code: String,
    pub(crate) note: String,
}

// ─── LƯƠNG / NHÂN SỰ (sheet Nhan Vien / Bang Luong) ───

#[derive(sqlx::FromRow, serde::Serialize)]
pub(crate) struct EmployeeRow {
    pub(crate) id: Option<i64>,
    pub(crate) code: String,
    pub(crate) name: String,
    pub(crate) department: String,
    pub(crate) position: String,
    pub(crate) job: String,
    pub(crate) basic_salary: f64,
    pub(crate) allowance_cv: f64,
    pub(crate) allowance_xx: f64,
    pub(crate) allowance_phone: f64,
    pub(crate) bh_salary: f64,
    pub(crate) hired_on: String,
    pub(crate) left_on: String,
    pub(crate) dependents: i64,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EmployeeInput {
    pub(crate) code: String,
    pub(crate) name: String,
    pub(crate) department: String,
    pub(crate) position: String,
    pub(crate) job: String,
    pub(crate) basic_salary: f64,
    pub(crate) allowance_cv: f64,
    pub(crate) allowance_xx: f64,
    pub(crate) allowance_phone: f64,
    pub(crate) bh_salary: f64,
    pub(crate) hired_on: String,
    pub(crate) left_on: String,
    pub(crate) dependents: i64,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PayrollLineInput {
    pub(crate) employee_code: String,
    pub(crate) work_days: f64,
    pub(crate) bonus: f64,
    pub(crate) advance: f64,
    pub(crate) pit_amount: f64,
    pub(crate) note: String,
}

#[derive(sqlx::FromRow, serde::Serialize)]
pub(crate) struct PayrollRow {
    pub(crate) period: String,
    pub(crate) employee_code: String,
    pub(crate) employee_name: String,
    pub(crate) department: String,
    pub(crate) work_days: f64,
    pub(crate) bonus: f64,
    pub(crate) advance: f64,
    pub(crate) pit_amount: f64,
    pub(crate) gross_salary: f64,
    pub(crate) bh_employer: f64,
    pub(crate) bh_employee: f64,
    pub(crate) net_pay: f64,
    pub(crate) note: String,
}

// ─── NHẬP KHỐI NHAP LIEU (sheet NHAP LIEU của file mẫu) ───

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImportEntryInput {
    pub(crate) posting_date: String,
    pub(crate) voucher_no: String,
    pub(crate) doc_date: String,
    pub(crate) entry_type: String,
    pub(crate) description: String,
    pub(crate) product_code: String,
    pub(crate) supplier_code: String,
    pub(crate) customer_code: String,
    pub(crate) quantity: f64,
    pub(crate) unit_price: f64,
    pub(crate) amount: f64,
    pub(crate) debit_account: String,
    pub(crate) credit_account: String,
    pub(crate) industry_code: String,
    pub(crate) vat_rate: f64,
    pub(crate) pit_rate: f64,
    pub(crate) tax_period: i64,
    pub(crate) unit_code: String,
    pub(crate) adjust_code: String,
    pub(crate) note: String,
}

// ─── BẢNG KÊ GIẢM THUẾ GTGT (sheet Giam Thue GTGT) ───

#[derive(sqlx::FromRow, serde::Serialize)]
pub(crate) struct VatReductionRow {
    pub(crate) posting_date: String,
    pub(crate) voucher_no: String,
    pub(crate) product_code: String,
    pub(crate) product_name: String,
    pub(crate) quantity: f64,
    pub(crate) unit_price: f64,
    pub(crate) amount: f64,
    pub(crate) vat_rate: f64,
    pub(crate) reduced_rate: f64,  // tỷ lệ sau giảm = vat_rate x 80%
    pub(crate) vat_reduced: f64,   // thuế GTGT được giảm = amount x (rate - reduced_rate)
}

// ─── AUDIT LOG ───

#[derive(sqlx::FromRow, serde::Serialize)]
pub(crate) struct AuditRow {
    pub(crate) id: Option<i64>,
    pub(crate) ts: String,
    pub(crate) action: String,
    pub(crate) entity: String,
    pub(crate) detail: String,
    pub(crate) username: String,
}

// ─── CHẤM CÔNG THEO NGÀY (sheet Cham Cong) ───

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AttendanceEntryInput {
    pub(crate) employee_code: String,
    pub(crate) work_date: String, // 'YYYY-MM-DD'
    pub(crate) status: String,    // X | NC | P | H | CT | TS | O | CO | KL | ''
}

#[derive(sqlx::FromRow, serde::Serialize)]
pub(crate) struct AttendanceRow {
    pub(crate) work_date: String,
    pub(crate) employee_code: String,
    pub(crate) employee_name: String,
    pub(crate) status: String,
}

#[derive(sqlx::FromRow, serde::Serialize)]
pub(crate) struct AttendanceWorkDayRow {
    pub(crate) employee_code: String,
    pub(crate) employee_name: String,
    pub(crate) work_days: f64, // X/P/H/CT/TS/O/CO = 1; NC = 0,5; KL/trống = 0
}

// ─── TỜ KHAI THUẾ THEO KỲ (sheet To Khai Thue) ───

/// Dữ liệu gộp thô được query từ DB (trước khi tính số thuế phải nộp)
#[derive(sqlx::FromRow)]
pub(crate) struct TaxAgg {
    pub(crate) industry_code: String,
    pub(crate) industry_name: String,
    pub(crate) vat_rate: f64,
    pub(crate) pit_rate: f64,
    pub(crate) revenue_up: f64,   // DT tính thuế GTGT — Tăng trong kỳ
    pub(crate) revenue_down: f64, // DT tính thuế GTGT — Giảm trong kỳ (điều chỉnh giảm)
    pub(crate) vat_reduced: f64,  // Thuế GTGT được giảm (sản phẩm giảm thuế, 20% thuế suất)
}

#[derive(serde::Serialize)]
pub(crate) struct TaxDeclarationRow {
    pub(crate) industry_code: String,
    pub(crate) industry_name: String,
    pub(crate) vat_rate: f64,
    pub(crate) pit_rate: f64,
    pub(crate) revenue_up: f64,
    pub(crate) revenue_down: f64,
    pub(crate) vat_reduced: f64,
    pub(crate) vat_tax: f64,    // Số thuế GTGT trước giảm = (UP-DOWN) x tỷ lệ
    pub(crate) vat_payable: f64, // Thuế GTGT phải nộp = vat_tax - vat_reduced
    pub(crate) pit_tax: f64,    // Số thuế TNCN = (UP-DOWN) x tỷ lệ
}

// ─── CHI TIẾT CHỨNG TỪ (in PNK 01-VT / PXK 02-VT) ───

#[derive(sqlx::FromRow, serde::Serialize)]
pub(crate) struct VoucherRow {
    pub(crate) posting_date: String,
    pub(crate) voucher_no: String,
    pub(crate) doc_date: String,
    pub(crate) entry_type: String,
    pub(crate) description: String,
    pub(crate) product_code: String,
    pub(crate) product_name: String,
    pub(crate) unit: String,
    pub(crate) supplier_code: String,
    pub(crate) supplier_name: String,
    pub(crate) customer_code: String,
    pub(crate) customer_name: String,
    pub(crate) quantity: f64,
    pub(crate) unit_price: f64,
    pub(crate) amount: f64,
    pub(crate) debit_account: String,
    pub(crate) credit_account: String,
    pub(crate) note: String,
}

pub(crate) struct ProductInfo {
    pub(crate) id: i64,
    pub(crate) vat_rate: f64,
    pub(crate) vat_reduced: bool,
}
