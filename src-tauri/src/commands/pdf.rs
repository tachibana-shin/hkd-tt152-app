// ─── Sinh PDF hóa đơn điện tử ───
//
// Frontend gửi thẳng `detail_json` (chuỗi đã lưu trong DB); backend tự dựng HTML
// từ template tiếng Việt nhúng sẵn rồi render bằng `htmltopdf` — không cần
// Chrome trên máy user, không có script chạy trong trang.
//
// Xem `crate::invoice` cho phần dựng HTML, mã QR và format số/ngày.

use crate::helpers::require_role;
use crate::invoice;
use crate::models::AppState;
use base64::Engine;
use tauri::State;

/// Lệnh frontend: nhận `detail_json`, trả về PDF hóa đơn dạng base64.
#[tauri::command]
pub(crate) async fn render_invoice_pdf(
    state: State<'_, AppState>,
    detail_json: String,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    // Render là CPU-bound (~200ms) — tách sang worker để không chặn runtime async.
    let pdf = tauri::async_runtime::spawn_blocking(move || invoice::render_pdf(&detail_json))
        .await
        .map_err(|e| format!("Lỗi render PDF hóa đơn: {e}"))??;
    Ok(base64::engine::general_purpose::STANDARD.encode(pdf))
}
