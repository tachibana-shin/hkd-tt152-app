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

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;

    /// Xuất lại cùng tên file phải ghi đè nội dung cũ, không báo lỗi.
    #[test]
    fn save_file_ghi_de_file_da_ton_tai() {
        let dir = std::env::temp_dir().join(format!("hkd-save-file-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bao-gia.xlsx");
        std::fs::write(&path, b"noi dung cu").unwrap();

        save_file(
            path.to_string_lossy().into_owned(),
            base64::engine::general_purpose::STANDARD.encode(b"noi dung moi"),
        )
        .unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), b"noi dung moi");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_file_tu_choi_base64_hong() {
        let path = std::env::temp_dir().join("hkd-save-file-bad.xlsx");
        assert!(save_file(path.to_string_lossy().into_owned(), "%%%".into()).is_err());
    }
}
