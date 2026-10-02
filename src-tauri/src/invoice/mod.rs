//! Sinh HTML + PDF hóa đơn điện tử hoàn toàn trong backend.
//!
//! Nguồn dữ liệu là `detail_json` đã lưu trong DB; template HTML tiếng Việt
//! nhúng sẵn vào binary và được render bằng `htmltopdf` — frontend không phải
//! dựng HTML/CSS, máy user không cần cài Chrome.

mod data;
mod format;
mod qr;
mod render;

pub(crate) use render::{build_invoice_html, render_pdf};
