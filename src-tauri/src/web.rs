//! Chế độ "dùng qua web": app tự chạy 1 HTTP server trên 127.0.0.1, vừa serve
//! frontend (dist nhúng sẵn trong binary) vừa phơi REST API gọi thẳng các command
//! Tauri có sẵn. Mở Chrome vào http://127.0.0.1:<port> là dùng được app như web —
//! cùng DB, cùng phiên đăng nhập với cửa sổ app desktop.
//!
//! Mặc định: bản dev (debug) LUÔN bật để dùng qua Chrome khi gõ `bun run tauri dev`;
//! bản production (release) mặc định TẮT — bật trong màn Cài đặt → "Bật web server".
//!
//! Vì chỉ bind 127.0.0.1 nên chỉ máy đang chạy app truy cập được; không lộ ra
//! mạng ngoài.

use crate::commands;
use crate::helpers::require_role;
use crate::models::AppState;
use crate::models::{
    AttendanceEntryInput, CashEntryInput, CountItemInput, EmployeeInput, ImportEntryInput,
    InboundItemInput, InvoiceItemInput, OutboundInvoiceInput, OutboundItemInput, PayrollLineInput,
    UserInput,
};
use axum::extract::Path;
use axum::http::{header, HeaderValue, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use serde::de::DeserializeOwned;
use serde_json::json;
use std::collections::HashMap;
use std::io::Write as _;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Manager, State};

/// Cổng mặc định; nếu bận sẽ tự dò cổng trống kế tiếp.
pub const WEB_PORT: u16 = 45731;

static APP: OnceLock<AppHandle> = OnceLock::new();

pub fn init_app(app: &AppHandle) {
    let _ = APP.set(app.clone());
}

fn app_handle() -> AppHandle {
    APP.get().expect("web: app handle chưa được đặt").clone()
}

fn state_arg() -> State<'static, AppState> {
    APP.get()
        .expect("web: app handle chưa được đặt")
        .state::<AppState>()
}

/// snake_case → camelCase (giống Tauri rename_all camelCase cho tham số command).
fn to_camel_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut upper = false;
    for c in s.chars() {
        if c == '_' {
            upper = true;
        } else if upper {
            out.push(c.to_ascii_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// Trích tham số từ body JSON (chấp nhận cả khóa camelCase lẫn snake_case).
#[allow(clippy::result_large_err)] // Response của axum là kiểu opaque lớn — không thể thu nhỏ.
fn extract_arg<T: DeserializeOwned>(body: &serde_json::Value, name: &str) -> Result<T, Response> {
    let key = to_camel_case(name);
    let v = body
        .get(&key)
        .or_else(|| body.get(name))
        .ok_or_else(|| err_json(StatusCode::BAD_REQUEST, format!("Thiếu tham số '{}'", name)))?;
    serde_json::from_value(v.clone()).map_err(|e| {
        err_json(
            StatusCode::BAD_REQUEST,
            format!("Tham số '{}' sai kiểu: {}", name, e),
        )
    })
}

macro_rules! mx {
    // Command nhận State đầu tiên (mặc định).
    ($func:path, $body:expr $(, $arg:ident : $ty:ty)* $(,)?) => {{
        $(
            let $arg: $ty = match crate::web::extract_arg(&$body, stringify!($arg)) {
                Ok(v) => v,
                Err(e) => return e,
            };
        )*
        $func(crate::web::state_arg(), $($arg),*).await
    }};
}

macro_rules! mx_state_app {
    // Command nhận State + AppHandle.
    ($func:path, $body:expr $(, $arg:ident : $ty:ty)* $(,)?) => {{
        $(
            let $arg: $ty = match crate::web::extract_arg(&$body, stringify!($arg)) {
                Ok(v) => v,
                Err(e) => return e,
            };
        )*
        $func(crate::web::state_arg(), crate::web::app_handle(), $($arg),*).await
    }};
}

macro_rules! mx_app {
    // Command chỉ nhận AppHandle.
    ($func:path, $body:expr $(, $arg:ident : $ty:ty)* $(,)?) => {{
        $(
            let $arg: $ty = match crate::web::extract_arg(&$body, stringify!($arg)) {
                Ok(v) => v,
                Err(e) => return e,
            };
        )*
        $func(crate::web::app_handle(), $($arg),*).await
    }};
}

macro_rules! mx_none {
    // Command không nhận State/AppHandle (vd hddt_status).
    ($func:path) => {{
        $func().await
    }};
}

fn ok_payload(s: &str) -> Response {
    // Command trả chuỗi: nếu đúng JSON thì trả JSON, ngược lại trả text thuần
    // (vd "ok") — frontend JSON.parse thử trước, fail thì dùng text.
    match serde_json::from_str::<serde_json::Value>(s) {
        Ok(v) => (
            StatusCode::OK,
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            )],
            v.to_string(),
        )
            .into_response(),
        Err(_) => (
            StatusCode::OK,
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/plain; charset=utf-8"),
            )],
            s.to_string(),
        )
            .into_response(),
    }
}

fn err_json(status: StatusCode, msg: String) -> Response {
    (
        status,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        )],
        json!({ "error": msg }).to_string(),
    )
        .into_response()
}

/// Ghi nhật ký request web (ra stdout + file web.log trong thư mục dữ liệu app).
/// KHÔNG ghi tham số request (tránh lộ mật khẩu) — chỉ ghi lệnh, trạng thái, lỗi.
fn log_request(cmd: &str, status: u16, detail: &str) {
    let line = format!(
        "[{}] POST /api/{cmd} -> {status}{}",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        if detail.is_empty() {
            String::new()
        } else {
            format!(" — {detail}")
        }
    );
    println!("{line}");
    if let Some(app) = APP.get() {
        if let Ok(dir) = app.path().app_data_dir() {
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(dir.join("web.log"))
            {
                let _ = writeln!(f, "{line}");
            }
        }
    }
}

/// POST /api/<cmd> — gọi command Tauri tương ứng với body JSON {camelCase: value}.
pub async fn api_invoke(
    Path(cmd): Path<String>,
    body: Option<axum::extract::Json<serde_json::Value>>,
) -> Response {
    let body: serde_json::Value = body.map(|j| j.0).unwrap_or_else(|| json!({}));
    let result: Result<String, String> = match cmd.as_str() {
        // ── Auth / người dùng ──
        "login" => mx!(commands::auth::login, body, username: String, password: String),
        "logout" => mx!(commands::auth::logout, body),
        "get_current_user" => mx!(commands::auth::get_current_user, body),
        "list_users" => mx!(commands::auth::list_users, body),
        "save_user" => mx!(commands::auth::save_user, body, input: UserInput),
        "delete_user" => mx!(commands::auth::delete_user, body, id: i64),
        "change_password" => mx!(
            commands::auth::change_password,
            body,
            old_password: String,
            new_password: String
        ),

        // ── Hồ sơ HKD (multi-profile) ──
        "get_profiles" => mx_app!(commands::profile::get_profiles, body),
        "create_profile" => mx_state_app!(commands::profile::create_profile, body, name: String),
        "rename_profile" => mx_state_app!(
            commands::profile::rename_profile,
            body,
            key: String,
            name: String
        ),
        "delete_profile" => mx_state_app!(commands::profile::delete_profile, body, key: String),
        "switch_profile" => mx_state_app!(commands::profile::switch_profile, body, key: String),
        "select_profile" => mx_state_app!(commands::profile::select_profile, body, key: String),
        "get_profile_prefs" => mx_app!(commands::profile::get_profile_prefs, body),
        "save_profile_pref" => mx_app!(
            commands::profile::save_profile_pref,
            body,
            key: String,
            prefs: crate::commands::profile::ProfilePrefs
        ),

        // ── Thông tin / cấu hình hộ kinh doanh ──
        "get_business_info" => mx!(commands::business::get_business_info, body),
        "get_business_config" => mx!(commands::business::get_business_config, body),
        "save_business_config" => mx!(
            commands::business::save_business_config,
            body,
            name: String,
            tax_code: String,
            address: String,
            short_name: String,
            ownership: String,
            province: String,
            tax_code_issued_on: String,
            phone: String,
            email: String,
            hddt_start_date: String
        ),
        "get_app_settings" => mx!(commands::settings::get_app_settings, body),
        "save_app_settings" => mx!(
            commands::settings::save_app_settings,
            body,
            settings: HashMap<String, String>
        ),

        // ── Web server (browser mode) ──
        // Chỉ web_url (đọc). set_web_server KHÔNG phơi qua /api: bật lại qua HTTP
        // bất khả thi khi server đã tắt (request không tới nơi) — bật/tắt chỉ từ
        // cửa sổ app desktop (IPC).
        "web_url" => mx_none!(crate::web::web_url),

        // ── Danh mục: tài khoản kế toán ──
        "get_accounts" => mx!(commands::catalog::get_accounts, body),
        "save_account" => mx!(
            commands::catalog::save_account,
            body,
            code: String,
            name: String,
            opening_debit: f64,
            opening_credit: f64
        ),
        "delete_account" => mx!(commands::catalog::delete_account, body, id: i64),

        // ── Danh mục: sản phẩm ──
        "get_products" => mx!(commands::catalog::get_products, body),
        "get_products_page" => mx!(commands::catalog::get_products_page, body, lazy_event: String),
        "save_product" => mx!(
            commands::catalog::save_product,
            body,
            code: String,
            name: String,
            unit: String,
            sale_price: f64,
            cost_price: f64,
            min_stock: f64,
            vat_rate: f64,
            import_tax_rate: f64,
            is_service: bool,
            industry_code: String
        ),
        "delete_product" => mx!(commands::catalog::delete_product, body, id: i64),
        "delete_products" => mx!(commands::catalog::delete_products, body, ids: Vec<i64>),
        "assign_goods_industry" => mx!(commands::catalog::assign_goods_industry, body),
        "next_product_code" => mx!(commands::catalog::next_product_code, body),
        "next_warehouse_code" => mx!(commands::catalog::next_warehouse_code, body),
        "next_customer_code" => mx!(commands::catalog::next_customer_code, body),
        "next_supplier_code" => mx!(commands::catalog::next_supplier_code, body),
        "next_employee_code" => mx!(commands::catalog::next_employee_code, body),

        // ── Danh mục: nhóm ngành / kho / đối tác ──
        "get_industry_groups" => mx!(commands::catalog::get_industry_groups, body),
        "save_industry_group" => mx!(
            commands::catalog::save_industry_group,
            body,
            code: String,
            name: String,
            vat_rate: f64,
            pit_rate: f64
        ),
        "delete_industry_group" => {
            mx!(commands::catalog::delete_industry_group, body, code: String)
        }
        "get_warehouses" => mx!(commands::catalog::get_warehouses, body),
        "save_warehouse" => {
            mx!(commands::catalog::save_warehouse, body, code: String, name: String)
        }
        "get_suppliers" => mx!(commands::catalog::get_suppliers, body),
        "save_supplier" => mx!(
            commands::catalog::save_supplier,
            body,
            code: String,
            name: String,
            address: String,
            tax_code: String,
            phone: String
        ),
        "get_customers" => mx!(commands::catalog::get_customers, body),
        "save_customer" => mx!(
            commands::catalog::save_customer,
            body,
            code: String,
            name: String,
            address: String,
            tax_code: String,
            phone: String
        ),

        // ── Nhân sự & bảng lương ──
        "get_employees" => mx!(commands::payroll::get_employees, body),
        "save_employee" => mx!(commands::payroll::save_employee, body, input: EmployeeInput),
        "get_payroll_periods" => mx!(commands::payroll::get_payroll_periods, body),
        "get_payroll" => mx!(commands::payroll::get_payroll, body, period: String),
        "save_payroll" => mx!(
            commands::payroll::save_payroll,
            body,
            period: String,
            items: Vec<PayrollLineInput>
        ),

        // ── Chấm công ──
        "get_attendance" => mx!(commands::attendance::get_attendance, body, period: String),
        "save_attendance" => mx!(
            commands::attendance::save_attendance,
            body,
            period: String,
            entries: Vec<AttendanceEntryInput>
        ),
        "get_attendance_work_days" => {
            mx!(commands::attendance::get_attendance_work_days, body, period: String)
        }

        // ── Nhập / xuất kho ──
        "save_inbound" => mx!(
            commands::stock::save_inbound,
            body,
            posting_date: String,
            voucher_no: String,
            description: String,
            supplier_code: String,
            warehouse_code: String,
            unit_code: String,
            items: Vec<InboundItemInput>,
            note: String,
            inbound_type: String,
            reference_no: String,
            vat_rate: f64,
            debit_account: String,
            credit_account: String,
            pay_now: bool,
            adjust_dir: String
        ),
        "save_outbound" => mx!(
            commands::stock::save_outbound,
            body,
            posting_date: String,
            voucher_no: String,
            description: String,
            customer_code: String,
            unit_code: String,
            items: Vec<OutboundItemInput>,
            note: String,
            receive_now: bool,
            outbound_type: String,
            adjust_dir: String,
            create_invoice: bool,
            invoice: OutboundInvoiceInput
        ),
        // ── Xuất hóa đơn sang dịch vụ HĐĐT khác (chép tay) ──
        "invoice_export_pack" => mx!(
            commands::invoice_export::invoice_export_pack,
            body,
            invoice_id: i64
        ),
        "invoice_mark_exported" => mx!(
            commands::invoice_export::invoice_mark_exported,
            body,
            invoice_id: i64
        ),
        "invoice_events" => mx!(commands::invoice_export::invoice_events, body, invoice_id: i64),
        "invoice_set_status" => mx!(
            commands::invoice_export::invoice_set_status,
            body,
            invoice_id: i64,
            to_status: String,
            reason: String,
            ref_invoice: String,
            adjust_voucher_no: String
        ),
        "get_journal_entries" => mx!(
            commands::stock::get_journal_entries,
            body,
            entry_type: String,
            from_date: String,
            to_date: String
        ),
        "get_voucher" => mx!(commands::stock::get_voucher, body, voucher_no: String),
        "get_stock_lots" => mx!(commands::stock::get_stock_lots, body, product_code: String),

        // ── Thu / chi ──
        "save_cash_entry" => mx!(commands::cash::save_cash_entry, body, input: CashEntryInput),

        // ── Tồn kho / kiểm kê ──
        "get_inventory_summary" => mx!(commands::accounting::get_inventory_summary, body),
        "get_inventory_counts" => mx!(commands::inventory::get_inventory_counts, body),
        "get_inventory_count_detail" => {
            mx!(commands::inventory::get_inventory_count_detail, body, id: i64)
        }
        "save_inventory_count" => mx!(
            commands::inventory::save_inventory_count,
            body,
            date: String,
            note: String,
            items: Vec<CountItemInput>
        ),

        // ── Hóa đơn ──
        "get_invoices" => mx!(commands::invoice::get_invoices, body),
        "get_invoice_detail" => mx!(commands::invoice::get_invoice_detail, body, id: i64),
        "save_invoice" => mx!(
            commands::invoice::save_invoice,
            body,
            number: String,
            date: String,
            customer: String,
            customer_tax_code: String,
            items: Vec<InvoiceItemInput>
        ),
        "link_hddt" => mx!(
            commands::invoice::link_hddt,
            body,
            invoice_id: i64,
            hddt_no: String,
            hddt_symbol: String,
            hddt_date: String
        ),

        // ── HĐĐT ──
        "hddt_get_config" => mx!(commands::hddt::hddt_get_config, body),
        "hddt_get_password" => mx!(commands::hddt::hddt_get_password, body),
        "hddt_save_config" => mx!(
            commands::hddt::hddt_save_config,
            body,
            username: String,
            password: String,
            base_url: String
        ),
        "hddt_status" => mx!(commands::hddt::hddt_status, body),
        "hddt_login" => mx!(commands::hddt::hddt_login, body),
        "hddt_captcha" => mx!(commands::hddt::hddt_captcha, body),
        "hddt_login_manual" => mx!(
            commands::hddt::hddt_login_manual,
            body,
            captcha_key: String,
            captcha_value: String
        ),
        "hddt_logout" => mx!(commands::hddt::hddt_logout, body),
        "hddt_set_auto_change_password" => mx!(
            commands::hddt::hddt_set_auto_change_password,
            body,
            enabled: bool
        ),
        "hddt_change_password" => mx!(commands::hddt::hddt_change_password, body),
        "hddt_list_invoices" => mx!(
            commands::hddt::hddt_list_invoices,
            body,
            direction: Option<String>,
            kind: Option<String>,
            from: Option<String>,
            to: Option<String>,
            state_filter: Option<String>,
            nmmst: Option<String>,
            nbmst: Option<String>,
            nmcmnd: Option<String>,
            tthai: Option<String>,
            ttxly: Option<String>,
            khmshdon: Option<String>,
            khhdon: Option<String>,
            shdon: Option<String>,
            unhiem: Option<bool>,
            size: Option<u32>
        ),
        // CHỈ DÙNG CHO E2E (không đăng ký trong invoke_handler của Tauri).
        "hddt_sync_test_seed" => mx!(
            commands::hddt::hddt_sync_test_seed,
            body,
            portal_id: String,
            detail: serde_json::Value
        ),
        "hddt_sync_scan" => mx!(
            commands::hddt::hddt_sync_scan,
            body,
            from: Option<String>,
            to: Option<String>,
            kinds: Option<String>
        ),
        "hddt_sync_preview" => mx!(
            commands::hddt::hddt_sync_preview,
            body,
            from: Option<String>,
            to: Option<String>,
            retry_failed: Option<bool>
        ),
        "hddt_sync_clear_cache" => mx!(commands::hddt::hddt_sync_clear_cache, body),
        "hddt_sync_import" => mx!(
            commands::hddt::hddt_sync_import,
            body,
            ids: Option<Vec<i64>>,
            warehouse_code: Option<String>,
            unit_code: Option<String>,
            debit_account: Option<String>,
            credit_account: Option<String>
        ),
        "hddt_send_simulated" => mx!(
            commands::hddt::hddt_send_simulated,
            body,
            invoice_no: String,
            symbol: String,
            total: f64
        ),

        // ── Nhập liệu / tra cứu MST ──
        "import_nhap_lieu" => mx!(
            commands::import::import_nhap_lieu,
            body,
            items: Vec<ImportEntryInput>
        ),
        "lookup_tax_code" => mx!(commands::tax_lookup::lookup_tax_code, body, mst: String),
        "lookup_tax_detail" => mx!(commands::tax_lookup::lookup_tax_detail, body, url: String),

        // ── Báo cáo / sổ sách ──
        "get_ledger" => mx!(
            commands::accounting::get_ledger,
            body,
            from_date: String,
            to_date: String
        ),
        "get_trial_balance" => mx!(
            commands::accounting::get_trial_balance,
            body,
            from_date: String,
            to_date: String
        ),
        "get_tax_summary" => mx!(
            commands::accounting::get_tax_summary,
            body,
            from_date: String,
            to_date: String,
            unit_code: String
        ),
        "get_tax_declaration" => mx!(
            commands::accounting::get_tax_declaration,
            body,
            year: i64,
            period: String,
            period_no: i64
        ),
        "get_tax_overview" => mx!(
            commands::accounting::get_tax_overview,
            body,
            year: i64,
            period: String,
            period_no: i64
        ),
        "get_revenue_expense" => mx!(
            commands::accounting::get_revenue_expense,
            body,
            from_date: String,
            to_date: String
        ),

        // ── Nhật ký hệ thống / sao lưu ──
        "log_audit" => mx!(
            commands::audit::log_audit,
            body,
            action: String,
            entity: String,
            detail: String
        ),
        "get_audit_log" => mx!(commands::audit::get_audit_log, body, limit: i64),
        "create_backup" => mx_state_app!(commands::admin::create_backup, body),
        "list_backups" => mx_state_app!(commands::admin::list_backups, body),
        "restore_backup" => {
            mx_state_app!(commands::admin::restore_backup, body, filename: String)
        }

        _ => {
            let msg = format!("Lệnh '{}' không hỗ trợ trên web", cmd);
            log_request(&cmd, 404, &msg);
            return err_json(StatusCode::NOT_FOUND, msg);
        }
    };

    // Lỗi nghiệp vụ (command Err) KHÔNG phải lỗi server:
    //   - login sai tài khoản/mật khẩu → 401 Unauthorized
    //   - các lỗi nghiệp vụ khác           → 400 Bad Request
    // (HTTP 500 chỉ còn dành cho lỗi thật sự của server.)
    let resp: Response = match result {
        Ok(s) => {
            log_request(&cmd, 200, "");
            ok_payload(&s)
        }
        Err(e) => {
            let status = if cmd == "login" {
                StatusCode::UNAUTHORIZED
            } else {
                StatusCode::BAD_REQUEST
            };
            log_request(&cmd, status.as_u16(), &e);
            err_json(status, e)
        }
    };
    resp
}

/// Serve assets nhúng (dist) — fallback index.html cho route client-side (SPA).
pub async fn serve_assets(uri: Uri) -> Response {
    let resolver = APP
        .get()
        .expect("web: app handle chưa được đặt")
        .asset_resolver();
    let path = uri.path().trim_start_matches('/').to_string();
    let asset = if path.is_empty() {
        resolver.get("index.html".into())
    } else {
        resolver
            .get(path.clone())
            .or_else(|| resolver.get("index.html".into()))
    };
    match asset {
        Some(a) => (
            StatusCode::OK,
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_str(&a.mime_type)
                    .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
            )],
            a.bytes,
        )
            .into_response(),
        None => err_json(StatusCode::NOT_FOUND, "Không tìm thấy tài nguyên".into()),
    }
}

pub fn router() -> Router {
    Router::new()
        .route("/api/health", get(|| async { "ok" }))
        .route("/api/{cmd}", post(api_invoke))
        .fallback(get(serve_assets))
}

/// Handle của task web server đang chạy (None nếu đang tắt) — để dừng khi tắt trong Cài đặt.
static SERVER_HANDLE: Mutex<Option<tauri::async_runtime::JoinHandle<Option<u16>>>> =
    Mutex::new(None);
/// Cổng web server đang bind (None nếu đang tắt) — để web_url / giao diện Cài đặt đọc.
static CURRENT_PORT: Mutex<Option<u16>> = Mutex::new(None);

/// Bind listener rồi serve đến khi bị hủy (tắt qua Cài đặt) hoặc lỗi.
/// Ghi CURRENT_PORT ngay sau khi bind thành công.
async fn bind_and_serve() -> Option<u16> {
    // Test/E2E: khóa cổng cố định (chỉ 1 cổng, không tự dò) — tránh test nhầm instance khác.
    if let Ok(p) = std::env::var("HKD_WEB_PORT") {
        if let Ok(port) = p.trim().parse::<u16>() {
            let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
            match tokio::net::TcpListener::bind(addr).await {
                Ok(listener) => {
                    *CURRENT_PORT.lock().unwrap() = Some(port);
                    println!(
                        "[web] Dùng được trên trình duyệt: http://127.0.0.1:{}  (mở bằng Chrome)",
                        port
                    );
                    if let Err(e) = axum::serve(listener, router()).await {
                        eprintln!("[web] Lỗi server: {}", e);
                    }
                    return Some(port);
                }
                Err(e) => {
                    eprintln!("[web] Không bind được cổng {port} ({e}) — thoát chế độ web");
                    return None;
                }
            }
        }
    }
    for port in WEB_PORT..=WEB_PORT + 20 {
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => {
                *CURRENT_PORT.lock().unwrap() = Some(port);
                println!(
                    "[web] Dùng được trên trình duyệt: http://127.0.0.1:{}  (mở bằng Chrome)",
                    port
                );
                if let Err(e) = axum::serve(listener, router()).await {
                    eprintln!("[web] Lỗi server: {}", e);
                }
                return Some(port);
            }
            Err(e) => {
                eprintln!("[web] Cổng {} bận ({}), thử cổng khác…", port, e);
            }
        }
    }
    eprintln!(
        "[web] Không bind được cổng {}–{} — bỏ qua chế độ web",
        WEB_PORT,
        WEB_PORT + 20
    );
    None
}

/// Đang chạy → giữ nguyên, trả cổng hiện tại. Chưa chạy → khởi động và đợi bind xong.
async fn ensure_server_running() -> Result<u16, String> {
    if let Some(p) = *CURRENT_PORT.lock().unwrap() {
        return Ok(p);
    }
    let handle = tauri::async_runtime::spawn(bind_and_serve());
    *SERVER_HANDLE.lock().unwrap() = Some(handle);
    // Bind local là tức thì — chỉ đợi vài giây cho chắc.
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let Some(p) = *CURRENT_PORT.lock().unwrap() {
            return Ok(p);
        }
    }
    *SERVER_HANDLE.lock().unwrap() = None;
    Err(format!(
        "Không mở được web server — các cổng {}–{} đều bận?",
        WEB_PORT,
        WEB_PORT + 20
    ))
}

fn disable_server() {
    if let Some(h) = SERVER_HANDLE.lock().unwrap().take() {
        h.abort();
    }
    *CURRENT_PORT.lock().unwrap() = None;
}

/// Đọc cài đặt web_server_enabled từ DB (production): web server mặc định TẮT,
/// chỉ bật khi người dùng bật trong Cài đặt.
fn web_server_setting(app: &AppHandle) -> bool {
    tauri::async_runtime::block_on(async {
        let st = app.state::<AppState>();
        let pool = st.pool.read().await;
        match sqlx::query_scalar::<_, String>(
            "SELECT value FROM app_setting WHERE key = 'web_server_enabled'",
        )
        .fetch_optional(&*pool)
        .await
        {
            Ok(Some(v)) => v == "true",
            _ => false,
        }
    })
}

/// Khởi động web server local (127.0.0.1) trong nền:
/// - bản dev (debug): LUÔN bật (dùng app qua Chrome khi gõ `bun run tauri dev`);
/// - bản production (release): mặc định TẮT — bật trong Cài đặt → "Bật web server".
pub fn start_server(app: AppHandle) {
    init_app(&app);
    let enabled = cfg!(debug_assertions) || web_server_setting(&app);
    if enabled {
        let handle = tauri::async_runtime::spawn(bind_and_serve());
        *SERVER_HANDLE.lock().unwrap() = Some(handle);
    }
}

/// URL web server local (browser mode) nếu đang chạy, ngược lại trả "".
#[tauri::command]
pub(crate) async fn web_url() -> Result<String, String> {
    Ok(match *CURRENT_PORT.lock().unwrap() {
        Some(p) => format!("http://127.0.0.1:{}", p),
        None => String::new(),
    })
}

/// Bật/tắt web server (production) từ màn Cài đặt: lưu cài đặt rồi khởi động /
/// dừng server ngay. Trả về `{enabled, url}`.
#[tauri::command]
pub(crate) async fn set_web_server(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    {
        let pool = state.pool.read().await;
        let value = if enabled { "true" } else { "false" };
        sqlx::query!(
            "INSERT INTO app_setting (key, value) VALUES ('web_server_enabled', ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            value
        )
        .execute(&*pool)
        .await
        .map_err(|e| e.to_string())?;
    }
    if enabled {
        let port = ensure_server_running().await?;
        Ok(json!({ "enabled": true, "url": format!("http://127.0.0.1:{}", port) }).to_string())
    } else {
        disable_server();
        Ok(json!({ "enabled": false, "url": "" }).to_string())
    }
}
