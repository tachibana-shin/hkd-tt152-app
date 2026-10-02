// ─── Sinh PDF hóa đơn điện tử ───
//
// Frontend dựng HTML từ `detail_json` (template port từ dự án `tra-cuu-hd`),
// backend in ra file PDF bằng Chrome/Chromium headless rồi trả về base64.
//
// Vì sao lại là Chrome chứ không phải thư viện Rust thuần: template hóa đơn
// dùng CSS A4 (`@page`), font nhúng base64, ảnh nền và một script chạy trong
// trang (sinh QR, tách trang) — chỉ trình duyệt thật mới in ra đúng bản in.
//
// Biến môi trường `HKD_CHROME_PATH` được ưu tiên trước tiên để đóng gói/CI
// chỉ định sẵn đường dẫn Chrome.

use crate::helpers::require_role;
use crate::models::AppState;
use base64::Engine;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tauri::State;
use tokio::process::Command;

/// Thời gian tối đa cho 1 lần in (Chrome khởi động + tải font nhúng ~5MB).
const PRINT_TIMEOUT: Duration = Duration::from_secs(90);

/// Trả về đường dẫn executable Chrome/Chromium/Edge dùng để in PDF.
pub(crate) fn find_chrome() -> Result<PathBuf, String> {
    if let Ok(v) = std::env::var("HKD_CHROME_PATH") {
        let v = v.trim();
        if !v.is_empty() {
            let p = PathBuf::from(v);
            return if p.is_file() {
                Ok(p)
            } else {
                Err(format!(
                    "Biến HKD_CHROME_PATH trỏ tới tệp không tồn tại: {v}"
                ))
            };
        }
    }

    for p in FIXED_CHROME_PATHS {
        let path = PathBuf::from(p);
        if path.is_file() {
            return Ok(path);
        }
    }

    for name in CHROME_COMMANDS {
        if let Some(p) = which(name) {
            return Ok(p);
        }
    }

    Err(
        "Không tìm thấy Chrome/Chromium để in PDF. Hãy cài Google Chrome hoặc \
         đặt biến môi trường HKD_CHROME_PATH = đường dẫn tới file chrome."
            .to_string(),
    )
}

/// Đường dẫn Chrome cố định theo từng hệ điều hành (kiểm tra `is_file` nên
/// dùng chung cho mọi OS — path của OS khác đơn giản là không tồn tại).
const FIXED_CHROME_PATHS: &[&str] = &[
    // macOS
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/Applications/Chromium.app/Contents/MacOS/Chromium",
    "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
    // Windows
    r"C:\Program Files\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files\Google\Chrome Beta\Application\chrome.exe",
    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
    // Linux
    "/opt/google/chrome/chrome",
    "/opt/google/chrome/google-chrome",
    "/usr/bin/google-chrome",
    "/usr/bin/google-chrome-stable",
    "/usr/bin/chromium",
    "/usr/bin/chromium-browser",
    "/usr/bin/microsoft-edge",
];

/// Tên executable tra trên `PATH` (Linux/macOS/WSL).
const CHROME_COMMANDS: &[&str] = &[
    "google-chrome-stable",
    "google-chrome",
    "chromium",
    "chromium-browser",
    "chrome",
    "microsoft-edge",
    "msedge",
];

/// Tìm `cmd` trong `PATH` thủ công (tránh phải thêm thư viện `which`).
fn which(cmd: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let mut names = vec![cmd.to_string()];
    if cfg!(windows) {
        names.push(format!("{cmd}.exe"));
        names.push(format!("{cmd}.cmd"));
        names.push(format!("{cmd}.bat"));
    }
    for dir in std::env::split_paths(&path) {
        for name in &names {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Cờ Chrome dùng cho mọi lần in. `no_sandbox` chỉ thêm khi lần thử có sandbox
/// thất bại (máy không bật được user-namespace / chạy trong container).
fn print_flags(no_sandbox: bool) -> Vec<String> {
    let mut flags = vec![
        "--headless=new".to_string(),
        "--disable-gpu".to_string(),
        "--disable-background-networking".to_string(),
        "--disable-extensions".to_string(),
        "--disable-sync".to_string(),
        "--no-first-run".to_string(),
        "--no-default-browser-check".to_string(),
        "--hide-scrollbars".to_string(),
        "--no-pdf-header-footer".to_string(),
        // Đảo thời gian ảo để script trong trang (sinh QR, tách trang, nạp font
        // nhúng) chạy xong trước khi Chrome chụp PDF.
        "--virtual-time-budget=20000".to_string(),
    ];
    if no_sandbox {
        flags.push("--no-sandbox".to_string());
        flags.push("--disable-dev-shm-usage".to_string());
    }
    flags
}

/// URL `file://` đúng chuẩn theo nền tảng (Windows dùng `file:///C:/...`).
fn file_url(path: &Path) -> String {
    let p = path.display().to_string();
    if cfg!(windows) {
        format!("file:///{}", p.replace('\\', "/"))
    } else {
        format!("file://{p}")
    }
}

/// In 1 chuỗi HTML thành PDF bằng Chrome headless, trả về bytes của file PDF.
///
/// Thử chạy có sandbox trước; nếu Chrome thoát lỗi thì thử lại với
/// `--no-sandbox` (cần trên một số máy ảo / container không bật user namespace).
pub(crate) async fn html_to_pdf(html: &str) -> Result<Vec<u8>, String> {
    let chrome = find_chrome()?;
    let dir = std::env::temp_dir().join(format!("hkd-invoice-pdf-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).map_err(|e| format!("Không tạo được thư mục tạm in PDF: {e}"))?;

    let result = match write_html_and_print(&chrome, &dir, html, false).await {
        Ok(pdf) => Ok(pdf),
        Err(first_err) => match write_html_and_print(&chrome, &dir, html, true).await {
            Ok(pdf) => Ok(pdf),
            Err(_) => Err(first_err),
        },
    };

    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// Ghi `html` vào thư mục tạm rồi chạy Chrome một lần.
async fn write_html_and_print(
    chrome: &Path,
    dir: &Path,
    html: &str,
    no_sandbox: bool,
) -> Result<Vec<u8>, String> {
    let html_path = dir.join("invoice.html");
    let pdf_path = dir.join("invoice.pdf");
    let profile = dir.join("profile");
    std::fs::write(&html_path, html)
        .map_err(|e| format!("Không ghi được file HTML in PDF: {e}"))?;
    // Lần thử sau phải tạo lại PDF từ đầu — file cũ (nếu có) sẽ làm ta lầm
    // tưởng lần thử này thành công.
    let _ = std::fs::remove_file(&pdf_path);

    let mut cmd = Command::new(chrome);
    for f in print_flags(no_sandbox) {
        cmd.arg(f);
    }
    cmd.arg(format!("--user-data-dir={}", profile.display()))
        .arg(format!("--print-to-pdf={}", pdf_path.display()))
        .arg(file_url(&html_path))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let out = tokio::time::timeout(PRINT_TIMEOUT, cmd.output())
        .await
        .map_err(|_| format!("Chrome in PDF quá {} giây", PRINT_TIMEOUT.as_secs()))?
        .map_err(|e| format!("Không khởi động được Chrome ({chrome:?}): {e}"))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let tail: String = stderr
            .lines()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join(" · ");
        return Err(format!(
            "Chrome thoát mã {}{}",
            out.status.code().unwrap_or(-1),
            if tail.is_empty() {
                String::new()
            } else {
                format!(" — {tail}")
            }
        ));
    }

    let pdf = std::fs::read(&pdf_path).map_err(|e| format!("Không đọc được file PDF: {e}"))?;
    if pdf.is_empty() {
        return Err("Chrome không tạo ra file PDF (nội dung trang rỗng?)".to_string());
    }
    Ok(pdf)
}

/// Lệnh frontend: nhận HTML đã dựng sẵn, trả về PDF dạng base64.
#[tauri::command]
pub(crate) async fn render_invoice_pdf(
    state: State<'_, AppState>,
    html: String,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let pdf = html_to_pdf(&html).await?;
    Ok(base64::engine::general_purpose::STANDARD.encode(pdf))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chrome_env_override_wins_and_reports_missing_file() {
        // `std::env::set_var` không an toàn với thread khác trong Rust 2024, nên
        // test này chỉ kiểm tra nhánh báo lỗi bằng path không tồn tại.
        let missing = "/definitely/not/a/chrome/binary";
        let saved = std::env::var("HKD_CHROME_PATH").ok();
        unsafe {
            std::env::set_var("HKD_CHROME_PATH", missing);
        }
        let err = find_chrome().expect_err("path không tồn tại phải báo lỗi");
        assert!(err.contains(missing), "lỗi phải nêu rõ đường dẫn: {err}");
        unsafe {
            match saved {
                Some(v) => std::env::set_var("HKD_CHROME_PATH", v),
                None => std::env::remove_var("HKD_CHROME_PATH"),
            }
        }
    }

    #[test]
    fn print_flags_enable_headless_pdf_and_page_budget() {
        let flags = print_flags(false);
        let joined = flags.join(" ");
        assert!(joined.contains("--headless=new"));
        assert!(joined.contains("--no-pdf-header-footer"));
        assert!(joined.contains("--virtual-time-budget=20000"));
        assert!(
            !joined.contains("--no-sandbox"),
            "không mặc định tắt sandbox"
        );

        let fallback = print_flags(true).join(" ");
        assert!(fallback.contains("--no-sandbox"));
        assert!(fallback.contains("--disable-dev-shm-usage"));
    }

    #[test]
    fn file_url_is_absolute_and_platform_correct() {
        let url = file_url(Path::new("/tmp/x/invoice.html"));
        assert!(url.starts_with("file://"), "phải là URL file: {url}");
        assert!(url.ends_with("/tmp/x/invoice.html"));
        if cfg!(windows) {
            assert!(url.starts_with("file:///"));
        }
    }

    /// In thật bằng Chrome có trên máy — bỏ qua sạch nếu máy không cài Chrome
    /// (CI đóng gói không nhất định phải có).
    #[tokio::test]
    async fn html_to_pdf_returns_a_real_pdf_document() {
        if let Err(e) = find_chrome() {
            eprintln!("bỏ qua test in PDF: {e}");
            return;
        }
        let html = r#"<!doctype html><html><head><meta charset="utf-8">
<style>@page{size:A4;margin:0}body{font-family:sans-serif;padding:20px}</style>
</head><body><h1>Hóa đơn test</h1><p>Số tiền: 1.234.567 đ</p></body></html>"#;
        let pdf = html_to_pdf(html).await.expect("in PDF thành công");
        assert!(pdf.starts_with(b"%PDF"), "phải là file PDF hợp lệ");
        assert!(pdf.len() > 1000, "PDF quá nhỏ ({} byte)", pdf.len());
    }
}
