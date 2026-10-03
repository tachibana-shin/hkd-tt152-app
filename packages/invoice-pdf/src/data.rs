//! Khung dữ liệu `detail_json` của hóa đơn điện tử.
//!
//! Mọi trường đều là [`serde_json::Value`] để "làm mềm": thiếu trường, sai kiểu
//! hay `null` vẫn render được — bản TypeScript gốc cũng vậy (`data.x ?? ""`).
//! Tên trường giữ nguyên theo cổng HĐĐT để template khớp 100% với dữ liệu trong DB.

use serde::Deserialize;
use serde_json::Value;

/// Danh sách dòng; `null`/thiếu → rỗng (JS: `data.hdhhdvu ?? []`).
fn vec_or_empty<'de, D, T>(d: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(d)?.unwrap_or_default())
}

/// Một dòng hàng hóa trong bảng chi tiết.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct GoodsRow {
    /// Loại tính chất (1 = hàng hóa/dịch vụ, 5 = hàng hóa đặc trưng).
    pub tchat: Value,
    pub stt: Value,
    pub ten: Value,
    pub dvtinh: Value,
    pub sluong: Value,
    pub dgia: Value,
    /// Tỷ lệ thuế hiển thị (chuỗi `"8%"`).
    pub ltsuat: Value,
    pub thtien: Value,
    /// Tiền chiết khấu của dòng (`null` khi không có).
    pub stckhau: Value,
    /// Tỷ lệ thuế dạng số (`0.08`) — dự phòng khi thiếu `ltsuat`.
    pub tsuat: Value,
}

/// Một dòng trong bảng tổng hợp theo thuế suất.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct TaxRow {
    pub tsuat: Value,
    pub thtien: Value,
    pub tthue: Value,
}

/// Chi tiết hóa đơn đọc từ `detail_json`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct InvoiceData {
    // ── Đầu hóa đơn ──
    pub khmshdon: Value,
    pub khhdon: Value,
    pub shdon: Value,
    pub tlhdon: Value,
    pub nky: Value,
    pub mhdon: Value,
    pub tchat: Value,

    // ── Bên bán ──
    pub nbten: Value,
    pub nbmst: Value,
    pub chma: Value,
    pub chten: Value,
    pub nbdchi: Value,
    pub nbsdthoai: Value,
    pub nbstkhoan: Value,
    pub nbtnhang: Value,
    pub nbmdvqhnsach: Value,

    // ── Bên mua ──
    pub nmten: Value,
    pub nmmst: Value,
    pub nmcmnd: Value,
    pub nmdchi: Value,
    pub nmstkhoan: Value,
    pub nmtnhang: Value,
    pub nmtnmua: Value,
    pub nmshchieu: Value,
    pub dksbke: Value,
    pub dknlbke: Value,

    // ── Thanh toán / tổng tiền ──
    pub thtttoan: Value,
    pub tgtcthue: Value,
    pub tgtthue: Value,
    pub tgtphi: Value,
    pub ttcktmai: Value,
    pub tgtttbso: Value,
    pub tgtttbchu: Value,

    // ── Chữ ký số + mã QR ──
    /// Chuỗi JSON (không phải object) chứa `Subject` / `SigningTime`.
    pub nbcks: Value,
    /// Chuỗi mã QR theo chuẩn của cổng HĐĐT; rỗng thì không vẽ QR.
    pub qrcode: Value,

    // ── Bảng ──
    #[serde(deserialize_with = "vec_or_empty")]
    pub hdhhdvu: Vec<GoodsRow>,
    #[serde(deserialize_with = "vec_or_empty")]
    pub thttltsuat: Vec<TaxRow>,
}
