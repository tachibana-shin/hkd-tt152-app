#![allow(unused_imports)]

use sqlx::SqlitePool;
use tauri::Manager;
pub(crate) struct AppState {
    /// Pool SQLite của HKD đang mở — dùng `RwLock` để có thể ĐỔI HỒ SƠ (đổi file DB)
    /// khi đang chạy: `switch_profile` ghi bản `write()` mới, mọi lệnh đọc qua `read()`.
    pub(crate) pool: tokio::sync::RwLock<SqlitePool>,
    /// Người dùng đang đăng nhập (None nếu chưa đăng nhập — buộc vào màn hình login)
    pub(crate) current_user: tokio::sync::Mutex<Option<CurrentUser>>,
    /// HDDT portal session (JWT + expiry), refreshed automatically before it
    /// expires. `None` = not logged in to the portal (or a different HKD profile).
    pub(crate) portal: tokio::sync::Mutex<Option<crate::hddt::PortalSession>>,
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
    /// Ngày bắt đầu sử dụng HĐĐT — mốc lấp hóa đơn mua vào.
    pub(crate) hddt_start_date: String,
    /// Ký hiệu (mẫu số) HĐĐT của hộ, tuỳ chọn — mặc định cho hộp thoại liên
    /// kết HĐĐT và được cập nhật theo lần dùng gần nhất.
    pub(crate) hddt_symbol: String,
    /// Nhóm hộ kinh doanh (1–4) do người dùng xác nhận trong cấu hình HKD.
    /// `None` = chưa chốt → app tự xếp theo doanh thu cả năm.
    pub(crate) tax_group: Option<i64>,
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
    pub(crate) import_tax_rate: f64,
    /// Sản phẩm dịch vụ (nhân công...) — không theo dõi tồn kho.
    pub(crate) is_service: bool,
    /// Nhóm ngành mặc định của sản phẩm (cơ sở tính thuế bán ra).
    pub(crate) industry_code: String,
}

#[derive(sqlx::FromRow, serde::Serialize)]
pub(crate) struct IndustryGroupRow {
    pub(crate) code: String,
    pub(crate) name: String,
    pub(crate) vat_rate: f64,
    pub(crate) pit_rate: f64,
}

// ─── SẢN PHẨM — LAZY LOAD (server-side page / sort / filter) ───
// Event PrimeVue DataTable: { first, rows, sortField, sortOrder, multiSortMeta, filters }

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProductPageEvent {
    #[serde(default)]
    pub(crate) first: Option<i64>,
    #[serde(default)]
    pub(crate) rows: Option<i64>,
    #[serde(default)]
    pub(crate) sort_field: Option<String>,
    #[serde(default)]
    pub(crate) sort_order: Option<i32>,
    #[serde(default)]
    pub(crate) multi_sort_meta: Option<Vec<ProductSortMeta>>,
    #[serde(default)]
    pub(crate) filters: std::collections::HashMap<String, ProductPageFilter>,
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProductSortMeta {
    pub(crate) field: Option<String>,
    pub(crate) order: Option<i32>,
}

/// Filter của 1 cột — PrimeVue gửi dạng `{ value, matchMode }` cho filterDisplay="row"
/// hoặc `{ operator, constraints: [{ value, matchMode }] }`.
#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProductPageFilter {
    pub(crate) value: Option<serde_json::Value>,
    #[serde(default)]
    pub(crate) match_mode: Option<String>,
    #[serde(default)]
    pub(crate) constraints: Option<Vec<ProductPageConstraint>>,
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProductPageConstraint {
    pub(crate) value: Option<serde_json::Value>,
    #[serde(default)]
    pub(crate) match_mode: Option<String>,
}

#[derive(serde::Serialize)]
pub(crate) struct ProductPageResult {
    pub(crate) rows: Vec<ProductRow>,
    pub(crate) total: i64,
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

/// Bảng cân đối số phát sinh — Số dư đầu kỳ (DMTK) + Phát sinh Nợ/Có trong kỳ
/// + Số dư cuối kỳ, theo từng tài khoản kế toán của hộ.
#[derive(sqlx::FromRow, serde::Serialize)]
pub(crate) struct TrialBalanceRow {
    pub(crate) code: String,
    pub(crate) name: String,
    pub(crate) opening_debit: f64,
    pub(crate) opening_credit: f64,
    pub(crate) debit_mvmt: f64,
    pub(crate) credit_mvmt: f64,
    pub(crate) closing_debit: f64,
    pub(crate) closing_credit: f64,
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

/// 1 dòng = 1 phiếu nhập/phiếu xuất (gom từ `journal_entry` cho màn kho).
#[derive(sqlx::FromRow, serde::Serialize)]
pub(crate) struct StockVoucherRow {
    pub(crate) voucher_no: String,
    pub(crate) posting_date: String,
    pub(crate) entry_type: String,
    pub(crate) description: String,
    pub(crate) supplier_name: Option<String>,
    pub(crate) customer_name: Option<String>,
    /// Số DÒNG hàng của phiếu (kể cả dòng điều chỉnh).
    pub(crate) line_count: i64,
    /// SỐ LOẠI mặt hàng = COUNT(DISTINCT product_code).
    pub(crate) item_count: i64,
    /// Tổng số lượng (các loại cộng lại) — tiện khi nhập ghi chung đơn vị.
    pub(crate) total_qty: f64,
    /// Tổng tiền chiết khấu thương mại của phiếu (đ).
    pub(crate) discount: f64,
    /// Tổng thành tiền = giá trị SAU chiết khấu.
    pub(crate) amount: f64,
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
    /// Số phiếu xuất nguồn (PX) khi hóa đơn được lập kèm phiếu xuất (rỗng nếu
    /// lập tay từ tab Hóa đơn) — liên kết 1-1, truy vết khi cần.
    pub(crate) voucher_no: String,
    /// Thời điểm bấm "Chép để xuất" (đã chép dữ liệu sang dịch vụ HĐĐT khác).
    pub(crate) exported_at: String,
    /// Lý do bị thay thế / bị sửa bên kia (xem `invoice_event` cho lịch sử đầy đủ).
    pub(crate) replace_reason: String,
    pub(crate) adjust_reason: String,
    /// Số HĐĐT liên quan khi bị sửa, hoặc số hóa đơn nội bộ của HĐ thay thế.
    pub(crate) ref_invoice: String,
    /// Hóa đơn thay thế cho hóa đơn này (ngược lại với `replaces_invoice_id`).
    pub(crate) replaces_invoice_id: Option<i64>,
    /// Phiếu xuất điều chỉnh dùng để hủy HĐ đã phát hành.
    pub(crate) adjust_voucher_no: String,
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
    /// Số tiền chiết khấu thương mại (đ) — giá trị dòng = Thành tiền − Tiền CK.
    pub(crate) discount: f64,
    /// Kho xuất trên dòng (rỗng → kho mặc định của sản phẩm / kho đầu tiên).
    pub(crate) warehouse_code: String,
    /// Nhóm ngành của dòng (cơ sở tỷ lệ thuế bán ra).
    pub(crate) industry_code: String,
    pub(crate) vat_rate: f64,
    pub(crate) pit_rate: f64,
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
    /// Số tiền chiết khấu thương mại (đ) — cột "Tiền CK" của Chứng từ mua hàng
    /// (TT88). Giá trị nhập kho = Thành tiền − Tiền CK.
    #[serde(default)]
    pub(crate) discount: f64,
}

#[derive(serde::Deserialize)]
pub(crate) struct OutboundItemInput {
    pub(crate) product_code: String,
    pub(crate) quantity: f64,
    pub(crate) unit_price: f64,
    /// Tiền chiết khấu thương mại trên dòng (đ) — doanh thu ghi vào sổ =
    /// SL × Đơn giá − CK. Mặc định 0; dùng khi lập phiếu xuất từ hóa đơn để
    /// doanh thu khớp đúng tổng hóa đơn.
    #[serde(default)]
    pub(crate) discount: f64,
    pub(crate) industry_code: String,
    /// Kho xuất trên dòng (rỗng → kho mặc định của sản phẩm / kho đầu tiên).
    #[serde(default)]
    pub(crate) warehouse_code: String,
}

/// Thông tin hóa đơn bán hàng lập KÈM theo phiếu xuất (tab "Hóa đơn").
/// Phiếu xuất thực tăng doanh thu (bán thường / điều chỉnh tăng) sẽ tự sinh 1
/// hóa đơn, số hóa đơn mặc định = số phiếu xuất (liên kết 1-1, không lệch sổ).
#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OutboundInvoiceInput {
    /// Số hóa đơn — rỗng = KHÔNG lập hóa đơn kèm theo phiếu.
    pub(crate) number: String,
    /// Số HĐĐT: khai → hóa đơn chuyển ngay thành "Đã liên kết HĐĐT" (official).
    pub(crate) e_invoice_no: String,
    pub(crate) e_invoice_symbol: String,
    /// Ngày HĐĐT (ISO yyyy-MM-dd hoặc rỗng).
    pub(crate) e_invoice_date: String,
}

#[derive(serde::Deserialize)]
pub(crate) struct InvoiceItemInput {
    pub(crate) product_code: String,
    pub(crate) quantity: f64,
    pub(crate) unit_price: f64,
    /// Nhóm ngành dòng (rỗng → lấy theo sản phẩm).
    #[serde(default)]
    pub(crate) industry_code: String,
    /// Số tiền chiết khấu thương mại (đ) — cột "Tiền CK" (mặc định 0).
    #[serde(default)]
    pub(crate) discount: f64,
    /// Kho xuất trên dòng (rỗng → kho mặc định của sản phẩm / kho đầu tiên).
    #[serde(default)]
    pub(crate) warehouse_code: String,
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
    /// Chứng từ thanh toán không dùng tiền mặt (tài khoản / mã giao dịch
    /// chuyển khoản). NĐ 68/2026 Điều 6 khoản 1: khoản chi từ 05 triệu đồng
    /// trở lên thanh toán bằng tiền mặt mà không có chứng từ này thì KHÔNG được
    /// trừ khi tính thu nhập tính thuế.
    #[serde(default)]
    pub(crate) bank_code: String,
    /// Người dùng đánh dấu khoản chi không được trừ (lương chủ hộ, chi cá nhân
    /// - gia đình, chi không có chứng từ... — Điều 6 khoản 2).
    #[serde(default = "default_true")]
    pub(crate) deductible: bool,
    #[serde(default)]
    pub(crate) non_deductible_reason: String,
}

/// Mặc định `deductible = true` để client cũ (chưa gửi trường này) không làm
/// mọi phiếu chi thành "không được trừ".
fn default_true() -> bool {
    true
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
}

/// Một khoản chi bị loại khỏi chi phí khi tính thu nhập tính thuế — hiện ra
/// cho người dùng biết cần bổ sung chứng từ (NĐ 68/2026 Điều 6).
#[derive(serde::Serialize)]
pub(crate) struct TaxCostWarning {
    pub(crate) posting_date: String,
    pub(crate) voucher_no: String,
    pub(crate) description: String,
    pub(crate) amount: f64,
    /// Lý do không được trừ (rỗng = được trừ).
    pub(crate) reason: String,
}

#[derive(serde::Serialize)]
pub(crate) struct TaxDeclarationRow {
    pub(crate) industry_code: String,
    pub(crate) industry_name: String,
    pub(crate) vat_rate: f64,
    pub(crate) pit_rate: f64,
    pub(crate) revenue_up: f64,
    pub(crate) revenue_down: f64,
    /// Doanh thu tính thuế TNCN của kỳ = (UP−DOWN) − phần trừ ngưỡng 01 tỷ
    /// (Luật TNCN Điều 7 khoản 3 điểm a)
    pub(crate) revenue_taxable: f64,
    /// Mức trừ ngưỡng TNCN đã khấu trong kỳ này (lũy kế các kỳ trước + kỳ này),
    /// để người dùng thấy rõ cơ sở tính thuế TNCN đã trừ được bao nhiêu.
    pub(crate) pit_deduction: f64,
    pub(crate) vat_tax: f64,     // Số thuế GTGT = (UP-DOWN) x tỷ lệ ngành
    pub(crate) vat_payable: f64, // Thuế GTGT phải nộp = vat_tax
    pub(crate) pit_tax: f64, // Số thuế TNCN theo tỷ lệ doanh thu (chỉ nhóm 2, phương pháp doanh thu)
}

/// Tổng hợp thuế phải nộp theo NĐ 68/2026/NĐ-CP + NĐ 141/2026/NĐ-CP:
/// nhóm hộ xác định theo tổng doanh thu CẢ NĂM; thuế GTGT/TNCN tính theo kỳ khai.
#[derive(serde::Serialize)]
pub(crate) struct TaxOverview {
    pub(crate) year: i64,
    /// Tổng doanh thu cả năm — cơ sở xếp nhóm hộ (1..4)
    pub(crate) year_revenue: f64,
    /// Nhóm hộ kinh doanh: 1 = ≤ 1 tỷ (miễn thuế), 2 = > 1–3 tỷ, 3 = > 3–50 tỷ, 4 = > 50 tỷ
    pub(crate) group: i64,
    /// Nhóm app tự xếp từ doanh thu cả năm
    pub(crate) auto_group: i64,
    /// true = nhóm đang dùng lấy từ hồ sơ HKD (đã chốt, không tự xếp lại)
    pub(crate) group_from_profile: bool,
    /// Nhóm app đã ghim cho năm này khi hộ vượt ngưỡng mà hồ sơ chưa chốt nhóm —
    /// nhóm của năm giữ nguyên, vượt mốc giữa năm không chuyển sang nhóm kế tiếp.
    pub(crate) frozen_group: Option<i64>,
    /// true = hồ sơ HKD chốt Nhóm 2/3/4: hộ thuộc diện nộp thuế suốt năm, mọi kỳ
    /// có doanh thu đều phải nộp (không chờ vượt ngưỡng, không trừ mức ngưỡng).
    pub(crate) taxed_from_start: bool,
    /// Phương pháp tính TNCN đang áp dụng: "revenue" | "profit"
    pub(crate) method: String,
    /// Doanh thu trong kỳ khai
    pub(crate) period_revenue: f64,
    /// Doanh thu tính thuế TNCN của kỳ = doanh thu kỳ − phần trừ ngưỡng 01 tỷ
    /// (Luật TNCN Điều 7 khoản 3 điểm a); bằng 0 trước kỳ vượt ngưỡng.
    pub(crate) taxable_revenue: f64,
    /// Mức trừ ngưỡng áp dụng cho năm (mặc định 01 tỷ đồng — NĐ 141/2026/NĐ-CP,
    /// sửa được ở màn Cài đặt)
    pub(crate) exempt_threshold: f64,
    /// Tổng mức trừ ngưỡng TNCN đã dùng trong năm = min(ngưỡng, doanh thu cả năm).
    pub(crate) exempt_used: f64,
    /// Hạn ngạch mức trừ còn lại = ngưỡng − đã dùng (0 khi đã dùng hết).
    pub(crate) exempt_remaining: f64,
    /// Mức trừ đã khấu cho riêng kỳ đang xem.
    pub(crate) exempt_period: f64,
    /// Mốc Nhóm 3 (thuế suất TNCN theo thu nhập 17%) đang áp dụng.
    pub(crate) group3_threshold: f64,
    /// Mốc Nhóm 4 (thuế suất TNCN theo thu nhập 20%) đang áp dụng.
    pub(crate) group4_threshold: f64,
    /// Giá vốn FIFO tính từ phiếu nhập (Điều 6 khoản 1 điểm a NĐ 68/2026)
    pub(crate) cogs: f64,
    /// Khoản chi khác được trừ (phiếu chi đủ điều kiện)
    pub(crate) other_expense: f64,
    /// Tổng chi phí được trừ = giá vốn + khoản chi được trừ
    pub(crate) expense: f64,
    /// Tổng khoản chi KHÔNG được trừ (Điều 6 khoản 2, hoặc ≥ 5 triệu trả tiền
    /// mặt không có chứng từ thanh toán không dùng tiền mặt — khoản 1)
    pub(crate) non_deductible: f64,
    /// Danh sách khoản chi bị loại khỏi chi phí, để người dùng xử lý chứng từ
    pub(crate) cost_warnings: Vec<TaxCostWarning>,
    /// Thu nhập tính thuế kỳ = doanh thu kỳ − chi phí được trừ
    pub(crate) profit: f64,
    /// Thuế suất TNCN theo lợi nhuận (15%/17%/20%) hoặc 0 nếu không áp dụng
    pub(crate) profit_rate: f64,
    /// false = doanh thu lũy kế tới cuối kỳ chưa vượt mức ngưỡng 01 tỷ → kỳ này
    /// chưa phát sinh thuế (Điều 8 khoản 1a NĐ 68/2026).
    pub(crate) taxable_period: bool,
    pub(crate) vat_payable: f64,
    pub(crate) pit_tax: f64,
    /// Số thuế TNCN tạm nộp theo tỷ lệ % × doanh thu kỳ (Điều 10 khoản 2 điểm b
    /// NĐ 68/2026) — với hộ nộp theo thu nhập tính thuế thì quyết toán cả năm
    /// mới là số chốt.
    pub(crate) pit_provisional: f64,
    /// Hạn nộp hồ sơ khai thuế của kỳ (dd/mm/yyyy), rỗng khi không xác định
    pub(crate) deadline: Option<String>,
    pub(crate) total_tax: f64,
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
    pub(crate) industry_code: String,
    pub(crate) note: String,
}

pub(crate) struct ProductInfo {
    pub(crate) id: i64,
    /// Sản phẩm dịch vụ — không theo dõi tồn kho (không kiểm tra FIFO khi xuất).
    pub(crate) is_service: bool,
    /// Nhóm ngành mặc định — dùng khi dòng xuất/hóa đơn không chỉ định nhóm.
    pub(crate) industry_code: String,
}
