//! Sinh mã QR cho hóa đơn điện tử.

use base64::Engine as _;
use image::{DynamicImage, Luma};
use qrcode::{EcLevel, QrCode};

/// Số pixel cạnh của khung mã QR (tính cả vùng tĩnh lặng 4 module) — khớp
/// `capture.js` gọi `toDataURL(n, { errorCorrectionLevel: "low", width: 80 })`.
pub(crate) const QR_PX: u32 = 80;

/// Số pixel mỗi module khi vẽ PNG: render lớn rồi thu về [`QR_PX`] để nét hơn
/// (HTML gán `width`/`height` = 80 nên tỉ lệ giữ nguyên như bản frontend).
const QR_MODULE_PX: u32 = 8;

/// Mã QR PNG (EC-low) dạng `data:` URI; chuỗi rỗng → không vẽ QR.
pub(crate) fn qr_data_uri(payload: &str) -> Option<String> {
    let payload = payload.trim();
    if payload.is_empty() {
        return None;
    }
    let code = QrCode::with_error_correction_level(payload, EcLevel::L).ok()?;
    let img = code
        .render::<Luma<u8>>()
        .module_dimensions(QR_MODULE_PX, QR_MODULE_PX)
        .build();

    let mut png: Vec<u8> = Vec::new();
    DynamicImage::ImageLuma8(img)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .ok()?;
    if png.is_empty() {
        return None;
    }
    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    ))
}
