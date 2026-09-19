#![allow(unused_imports)]

use crate::helpers::*;
use crate::models::*;
use serde_json::json;
use sqlx::sqlite::{SqlitePoolOptions, SqliteTransaction};
use sqlx::SqlitePool;
use tauri::{AppHandle, Manager, State};

/// ─── Cổng HĐĐT (hóa đơn điện tử) — scaffold kết nối ───
///
/// Trạng thái hiện tại: **MÔ PHỎNG cục bộ** — `link_hddt` chỉ ghi nhận
/// số/ký hiệu/ngày HĐĐT vào DB (theo thông tư 78/2021/TT-BTC, mẫu TT152).
///
/// Khi có chứng thư số + endpoint cổng Tổng cục Thuế, thay thế
/// `send_invoice_simulated` bằng luồng:
///   1. Chuẩn bị XML hóa đơn (chuẩn hóa theo nghiệp vụ HĐĐT)
///   2. Ký số bằng USB Token / HSM
///   3. POST lên cổng (vd `InvoiceAPIv2/InvoiceWS/uploadInvoice` cũ của TCT khu vực)
///   4. Nhận Mã số của CQT (trường `e_invoice_no` chính là mã CT CQT cấp)
///
/// TODO(hddt): bổ sung cấu hình `hddt_endpoint`, `hddt_username` vào bảng
/// `business` + màn hình cài đặt; giữ chế độ mô phỏng làm mặc định.
/// Gửi hóa đơn (mô phỏng) — luôn trả OK để app dùng được ngay.
pub(crate) fn send_invoice_simulated(
    invoice_no: &str,
    symbol: &str,
    total: f64,
) -> Result<String, String> {
    Ok(format!(
        "SIMULATED|{}|{}|{}",
        symbol, invoice_no, total as i64
    ))
}

/// Kiểm tra trạng thái kết nối cổng HĐĐT (mô phỏng).
#[tauri::command]
pub(crate) async fn hddt_status() -> Result<String, String> {
    Ok(json!({
        "mode": "simulated",
        "connected": false,
        "note": "Chưa cấu hình cổng HĐĐT — chế độ mô phỏng cục bộ (ghi nhận số/ký hiệu/ngày cục bộ)."
    })
    .to_string())
}

/// Gửi thử hóa đơn lên cổng (mô phỏng) — để kiểm thử luồng HĐĐT.
#[tauri::command]
pub(crate) async fn hddt_send_simulated(
    state: State<'_, AppState>,
    invoice_no: String,
    symbol: String,
    total: f64,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    send_invoice_simulated(&invoice_no, &symbol, total)
}
