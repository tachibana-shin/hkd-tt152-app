//! Nhận diện **kiểu hóa đơn** theo ký hiệu mẫu và ký hiệu hóa đơn.
//!
//! Căn cứ Phụ lục I Thông tư 91/2026/TT-BTC:
//!
//! - Ký hiệu mẫu là **một chữ số 1..9** phản ánh loại hóa đơn điện tử;
//! - Ký hiệu hóa đơn gồm 6 ký tự `C|K` + 2 chữ số năm + **ký tự thứ 4** +
//!   2 ký tự người bán tự quy định; chữ thứ 4 phân biệt các mẫu cùng nhóm
//!   (`T` đăng ký, `D` đặc thù/bán tài sản công, `M` máy tính tiền, `N` phiếu
//!   xuất kho kiêm vận chuyển nội bộ, `B` phiếu xuất kho gửi bán đại lý,
//!   `F` kiêm tờ khai hoàn thuế, `X` thương mại điện tử…).
//!
//! Danh sách 14 mẫu hóa đơn/biên lai và 7 lĩnh vực đặc thù lấy theo bài tham
//! khảo <https://www.meinvoice.vn/tin-tuc/4500/mau-hoa-don-dien-tu-linh-vuc/>.
//!
//! Mục đích: khi `tlhdon`/`thdon` trong `detail_json` rỗng thì hóa đơn vẫn in
//! ra **đúng tên loại** thay vì một dòng chung chung. Tên lấy từ dữ liệu khi
//! có — bảng này chỉ là đường dự phòng.

use serde_json::Value;

/// Loại hóa đơn suy ra từ ký hiệu mẫu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// 1 — Hóa đơn giá trị gia tăng.
    ValueAdded,
    /// 2 — Hóa đơn bán hàng.
    SaleOfGoods,
    /// 3 — Hóa đơn bán tài sản công.
    PublicAsset,
    /// 4 — Hóa đơn bán hàng dự trữ quốc gia.
    StrategicReserve,
    /// 5 — Hóa đơn điện tử khác (tem, vé, thẻ, phiếu thu điện tử…).
    Other,
    /// 6 — Chứng từ điện tử dùng và quản lý như hóa đơn (phiếu xuất kho kiêm
    /// vận chuyển nội bộ, phiếu xuất kho hàng gửi bán đại lý).
    ManagedDocument,
    /// 7 — Hóa đơn thương mại điện tử.
    ECommerce,
    /// 8 — Hóa đơn GTGT tích hợp biên lai thu thuế, phí, lệ phí.
    ValueAddedWithReceipt,
    /// 9 — Hóa đơn bán hàng tích hợp biên lai thu thuế, phí, lệ phí.
    SaleOfGoodsWithReceipt,
    /// Thiếu hoặc không đọc được ký hiệu mẫu.
    Unknown,
}

impl Kind {
    /// Nhận diện theo `khmshdon`.
    ///
    /// Cổng HĐĐT gửi số `1..9`; hóa đơn đặt in còn để dạng mẫu số 11 ký tự
    /// (`01GTKT3/001`) thì lấy 2 chữ số đầu.
    pub fn detect(khmshdon: &Value) -> Kind {
        let code: String = match khmshdon {
            Value::Number(n) => n
                .as_f64()
                .filter(|f| f.is_finite())
                .map(|f| format!("{}", f as i64))
                .unwrap_or_default(),
            Value::String(s) => s
                .trim()
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect(),
            _ => String::new(),
        };
        match code.as_str() {
            "1" => Kind::ValueAdded,
            "2" => Kind::SaleOfGoods,
            "3" => Kind::PublicAsset,
            "4" => Kind::StrategicReserve,
            "5" => Kind::Other,
            "6" => Kind::ManagedDocument,
            "7" => Kind::ECommerce,
            "8" => Kind::ValueAddedWithReceipt,
            "9" => Kind::SaleOfGoodsWithReceipt,
            // Mẫu số 11 ký tự: `01` GTGT, `02` bán hàng, `03`/`04` phiếu xuất
            // kho (cùng nhóm 6). Không đoán tiếp — sai tên còn tệ hơn tên chung.
            _ => match code.get(..2) {
                Some("01") => Kind::ValueAdded,
                Some("02") => Kind::SaleOfGoods,
                Some("03") | Some("04") => Kind::ManagedDocument,
                _ => Kind::Unknown,
            },
        }
    }

    /// Tên loại hóa đơn — ghép ký hiệu mẫu với ký tự thứ 4 của ký hiệu hóa đơn
    /// để phân biệt các mẫu cùng nhóm (Phụ lục V).
    ///
    /// Trả `""` cho [`Kind::Unknown`] để người gọi tự chọn tên chung.
    pub fn name(&self, khhdon: &Value) -> &'static str {
        let letter = subtype_letter(khhdon);
        match (self, letter) {
            (Kind::ValueAdded, Some('F')) => "Hóa đơn giá trị gia tăng kiêm tờ khai hoàn thuế",
            (Kind::ValueAdded, Some('D')) => "Hóa đơn giá trị gia tăng đặc thù",
            (Kind::SaleOfGoods, Some('M')) => "Hóa đơn bán hàng khởi tạo từ máy tính tiền",
            (Kind::ManagedDocument, Some('N')) => "Phiếu xuất kho kiêm vận chuyển nội bộ",
            (Kind::ManagedDocument, Some('B')) => "Phiếu xuất kho hàng gửi bán đại lý",
            (Kind::ValueAdded, _) => "Hóa đơn giá trị gia tăng",
            (Kind::SaleOfGoods, _) => "Hóa đơn bán hàng",
            (Kind::PublicAsset, _) => "Hóa đơn bán tài sản công",
            (Kind::StrategicReserve, _) => "Hóa đơn bán hàng dự trữ quốc gia",
            (Kind::Other, _) => "Hóa đơn điện tử khác",
            (Kind::ManagedDocument, _) => "Chứng từ điện tử sử dụng, quản lý như hóa đơn",
            (Kind::ECommerce, _) => "Hóa đơn thương mại điện tử",
            (Kind::ValueAddedWithReceipt, _) => {
                "Hóa đơn giá trị gia tăng tích hợp biên lai thu thuế, phí, lệ phí"
            }
            (Kind::SaleOfGoodsWithReceipt, _) => {
                "Hóa đơn bán hàng tích hợp biên lai thu thuế, phí, lệ phí"
            }
            (Kind::Unknown, _) => "",
        }
    }
}

/// Ký tự thứ 4 của ký hiệu hóa đơn: `C26MOC` → `M`.
///
/// Không có dạng chuẩn thì trả `None` (loại hóa đơn dùng tên mặc định).
fn subtype_letter(khhdon: &Value) -> Option<char> {
    let mut chars = khhdon.as_str()?.trim().chars();
    let head = chars.next()?;
    if head != 'C' && head != 'K' {
        return None;
    }
    let year: Vec<char> = chars.by_ref().take(2).collect();
    if year.len() != 2 || !year.iter().all(|c| c.is_ascii_digit()) {
        return None;
    }
    chars.next().filter(|c| c.is_ascii_alphabetic())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn group_number_reads_the_official_codes() {
        for (code, expected) in [
            (json!(1), Kind::ValueAdded),
            (json!(2), Kind::SaleOfGoods),
            (json!(3), Kind::PublicAsset),
            (json!(4), Kind::StrategicReserve),
            (json!(5), Kind::Other),
            (json!(6), Kind::ManagedDocument),
            (json!(7), Kind::ECommerce),
            (json!(8), Kind::ValueAddedWithReceipt),
            (json!(9), Kind::SaleOfGoodsWithReceipt),
            // Bản ghi cũ có thể là chuỗi, và JSON number là số thực.
            (json!("2"), Kind::SaleOfGoods),
            (json!(1.0), Kind::ValueAdded),
        ] {
            assert_eq!(Kind::detect(&code), expected, "ký hiệu mẫu {code}");
        }
        for missing in [
            json!(null),
            json!(0),
            json!(12),
            json!(""),
            json!("abc"),
            json!("07KPTQ/001"),
        ] {
            assert_eq!(
                Kind::detect(&missing),
                Kind::Unknown,
                "không nhận diện được {missing}"
            );
        }
    }

    #[test]
    fn eleven_char_sample_numbers_map_to_the_right_group() {
        assert_eq!(Kind::detect(&json!("01GTKT3/001")), Kind::ValueAdded);
        assert_eq!(Kind::detect(&json!("02GTTT/001")), Kind::SaleOfGoods);
        assert_eq!(Kind::detect(&json!("03XKNB/001")), Kind::ManagedDocument);
        assert_eq!(Kind::detect(&json!("04HGDL/001")), Kind::ManagedDocument);
    }

    #[test]
    fn the_fourth_letter_of_the_invoice_code_names_the_variant() {
        for (mau, kyhieu, ten) in [
            (1, "C26TYY", "Hóa đơn giá trị gia tăng"),
            (1, "C26DYY", "Hóa đơn giá trị gia tăng đặc thù"),
            (
                1,
                "C26FYY",
                "Hóa đơn giá trị gia tăng kiêm tờ khai hoàn thuế",
            ),
            (2, "C26TYY", "Hóa đơn bán hàng"),
            (2, "C26MYY", "Hóa đơn bán hàng khởi tạo từ máy tính tiền"),
            (3, "C26DYY", "Hóa đơn bán tài sản công"),
            (4, "C26DYY", "Hóa đơn bán hàng dự trữ quốc gia"),
            (5, "C26TYY", "Hóa đơn điện tử khác"),
            (6, "C26TYY", "Chứng từ điện tử sử dụng, quản lý như hóa đơn"),
            (6, "C26NYY", "Phiếu xuất kho kiêm vận chuyển nội bộ"),
            (6, "C26BYY", "Phiếu xuất kho hàng gửi bán đại lý"),
            (7, "C26XYY", "Hóa đơn thương mại điện tử"),
            (
                8,
                "C26TYY",
                "Hóa đơn giá trị gia tăng tích hợp biên lai thu thuế, phí, lệ phí",
            ),
            (
                9,
                "C26TYY",
                "Hóa đơn bán hàng tích hợp biên lai thu thuế, phí, lệ phí",
            ),
        ] {
            let kind = Kind::detect(&json!(mau));
            assert_eq!(kind.name(&json!(kyhieu)), ten, "mẫu {mau} / {kyhieu}");
        }
    }

    #[test]
    fn a_code_without_the_expected_shape_has_no_variant_letter() {
        assert!(
            subtype_letter(&json!("FASTCA01")).is_none(),
            "không bắt đầu C/K"
        );
        assert!(subtype_letter(&json!("C2")).is_none(), "thiếu 2 chữ số năm");
        assert!(
            subtype_letter(&json!("CXYTYY")).is_none(),
            "năm không phải số"
        );
        assert!(subtype_letter(&json!(null)).is_none(), "thiếu ký hiệu");
        assert_eq!(
            subtype_letter(&json!("K26MYY")),
            Some('M'),
            "hóa đơn không mã"
        );
    }

    #[test]
    fn unknown_type_has_no_name_so_the_renderer_can_fall_back() {
        assert_eq!(Kind::Unknown.name(&json!("C26TYY")), "");
        assert_eq!(Kind::detect(&json!(null)).name(&json!(null)), "");
    }
}
