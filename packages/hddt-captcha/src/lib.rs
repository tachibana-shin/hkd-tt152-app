//! Giải captcha cổng HĐĐT (`hoadondientu.gdt.gov.vn`) — **offline 100%**.
//!
//! Nghiên cứu 09/2026 (decompile bundle Next.js của cổng + kiểm chứng bằng
//! Chrome thật):
//!   - `GET {base}/api/captcha` → `{"key":"<id hex>","content":"<svg 200x40>"}`
//!   - SVG gồm 6 glyph (đường Bézier bậc hai kiểu TrueType, ~32 cao trong
//!     viewBox 40) + 2 nét sóng nhiễu (`stroke` vô `fill`).
//!   - Bộ ký tự thực tế 30 chữ `23456789ABCDEFGHJKMNPQRSTVWXYZ` (loại các ký tự
//!     dễ nhầm `0 1 I L O U`).
//!
//! Solver khớp mẫu glyph: mỗi ký tự có bitmap chuẩn trong `templates.bin`, glyph
//! truy vấn được raster hoá đúng thuật toán đó rồi so IoU — không spawn tiến
//! trình, không ghi file tạm, không gọi API bên thứ ba. Số đo trên cổng thật:
//! 8/8 lần đăng nhập thành công, leave-one-out 756/756.
//!
//! [`CaptchaSolver`] là điểm cắm cho solver khác (OCR/AI/trả phí) nếu muốn.

pub mod captcha;
pub mod glyph;
pub mod solver;

pub use captcha::{Captcha, CaptchaSolver};
pub use solver::{
    classify_glyphs_detailed, classify_svg, glyph_paths, GlyphTemplateSolver, CHARSET,
};
