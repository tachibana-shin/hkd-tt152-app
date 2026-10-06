// ─── Ghi file do frontend chọn ───
//
// Bản desktop (Tauri/WebKitGTK) KHÔNG xử lý tải xuống kiểu web — `<a download>`
// trên `blob:` lặng thinh, không hộp thoại, không lỗi. Mọi nút xuất file vì thế
// phải đi: hộp thoại "Lưu file" gốc của hệ điều hành (tauri-plugin-dialog, do
// frontend mở) → trả về đường dẫn người dùng đã chọn → lệnh này ghi bytes ra.
//
// Đường dẫn đến từ chính lựa chọn của người dùng trong hộp thoại nên không giới
// hạn scope ghi (frontend là code bản địa, không phải trang web xa).

use base64::Engine;

/// Nhận nội dung file dạng base64 — IPC JSON gọn hơn nhiều so với mảng byte
/// (file ZIP báo cáo nhiều MB, mảng số sẽ phình to gấp nhiều lần).
#[tauri::command]
pub(crate) fn save_file(path: String, contents: String) -> Result<(), String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(contents)
        .map_err(|e| format!("Sai dữ liệu base64: {e}"))?;
    std::fs::write(&path, bytes).map_err(|e| format!("Không ghi được file {path}: {e}"))
}
