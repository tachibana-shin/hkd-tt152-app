//! Dựng HTML + render PDF hóa đơn điện tử hoàn toàn trong backend.
//!
//! Nguồn dữ liệu là `detail_json` đã lưu trong DB; template HTML tiếng Việt
//! nhúng sẵn vào binary và được render bằng `htmltopdf` — frontend không phải
//! dựng HTML/CSS, máy user không cần cài Chrome.
//!
//! Gói độc lập với app Tauri: chỉ nhận chuỗi `detail_json` và trả HTML/PDF,
//! không đụng tới DB, IPC hay `tauri`.

mod data;
mod format;
mod qr;
mod render;

pub use render::{build_invoice_html, render_pdf};
