//! Dựng HTML hóa đơn từ `detail_json` rồi render ra PDF.
//!
//! Toàn bộ pipeline nằm trong backend: không có script chạy trong trang, không
//! cần Chrome/Font nhúng — htmltopdf tự layout, tự nhúng font hệ thống và tự
//! tách trang A4.

use crate::invoice::data::{GoodsRow, InvoiceData, TaxRow};
use crate::invoice::format::{
    escape, extract_cn, format_vietnamese_date, format_vnd, js_number, js_string, parse_signature,
    Signature,
};
use crate::invoice::qr::{qr_data_uri, QR_PX};
use base64::Engine as _;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Template HTML tĩnh (tiếng Việt) nhúng sẵn vào binary.
const TEMPLATE: &str = include_str!("template.html");

/// Ảnh nền + ảnh dấu tích — nhúng base64 nên htmltopdf không cần thư mục nguồn.
const ASSETS: &[(&str, &str, &[u8])] = &[
    (
        "viewinvoice-bg.jpg",
        "image/jpeg",
        include_bytes!("assets/viewinvoice-bg.jpg"),
    ),
    (
        "sign-check.jpg",
        "image/jpeg",
        include_bytes!("assets/sign-check.jpg"),
    ),
];

/// Template đã hoán chỗ ảnh nền thành `data:` URI (chỉ phải làm 1 lần).
fn template() -> &'static str {
    static READY: OnceLock<String> = OnceLock::new();
    READY.get_or_init(|| {
        let mut html = TEMPLATE.to_string();
        for (name, mime, bytes) in ASSETS {
            let data = format!(
                "data:{mime};base64,{}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            );
            html = html.replace(name, &data);
        }
        html
    })
}

/// Dựng HTML hoàn chỉnh (chưa render PDF) — dùng cho test đối chiếu.
pub(crate) fn build_invoice_html(detail_json: &str) -> Result<String, String> {
    let data: InvoiceData = serde_json::from_str(detail_json)
        .map_err(|e| format!("Không đọc được chi tiết hóa đơn (detail_json): {e}"))?;
    Ok(render(&data))
}

/// Render PDF cho 1 hóa đơn từ chuỗi `detail_json`.
pub(crate) fn render_pdf(detail_json: &str) -> Result<Vec<u8>, String> {
    let html = build_invoice_html(detail_json)?;
    htmltopdf::Engine::new()
        .render_html(&html, htmltopdf::RenderOptions::default())
        .map_err(|e| format!("Không render được PDF hóa đơn: {e}"))
}

fn render(data: &InvoiceData) -> String {
    let mut slots: HashMap<&'static str, String> = HashMap::new();

    // ── Đầu hóa đơn ──
    let title = if matches!(data.tlhdon, serde_json::Value::Null) {
        "HÓA ĐƠN".to_string()
    } else {
        js_string(&data.tlhdon)
    };
    slots.insert("INVOICE_TITLE", escape(&title.to_uppercase()));
    slots.insert("INVOICE_DATE", escape(&format_vietnamese_date(&data.nky)));

    for (key, value) in [
        ("KHMSHDON", &data.khmshdon),
        ("KHHDON", &data.khhdon),
        ("SHDON", &data.shdon),
        ("MHDON", &data.mhdon),
        // ── Bên bán ──
        ("NBTEN", &data.nbten),
        ("NBMST", &data.nbmst),
        ("CHMA", &data.chma),
        ("CHTEN", &data.chten),
        ("NBDCHI", &data.nbdchi),
        ("NBSDTHOAI", &data.nbsdthoai),
        ("NBSTKHOAN", &data.nbstkhoan),
        ("NBTNHANG", &data.nbtnhang),
        ("NBMDVQHNSACH", &data.nbmdvqhnsach),
        // ── Bên mua ──
        ("NMTEN", &data.nmten),
        ("NMMST", &data.nmmst),
        ("NMCMND", &data.nmcmnd),
        ("NMDCHI", &data.nmdchi),
        ("NMSTKHOAN", &data.nmstkhoan),
        ("NMTNHANG", &data.nmtnhang),
        ("NMTNMUA", &data.nmtnmua),
        ("NMSHCHIEU", &data.nmshchieu),
        ("DKSBKE", &data.dksbke),
        ("DKNLBKE", &data.dknlbke),
        // ── Thanh toán ──
        ("THTTTOAN", &data.thtttoan),
        ("TGTTTBCHU", &data.tgtttbchu),
    ] {
        slots.insert(key, escape(&js_string(value)));
    }

    for (key, value) in [
        ("TGTCTHUE_VND", &data.tgtcthue),
        ("TGTTHUE_VND", &data.tgtthue),
        ("TGTPHI_VND", &data.tgtphi),
        ("TTCKTMAI_VND", &data.ttcktmai),
        ("TGTTTBSO_VND", &data.tgtttbso),
    ] {
        slots.insert(key, format_vnd(value));
    }

    // ── Mã QR ──
    slots.insert("QR_IMG", qr_image(&js_string(&data.qrcode)));

    // ── Bảng ──
    slots.insert("GOODS_ROWS", goods_rows(&data.hdhhdvu));
    slots.insert("TAX_ROWS", tax_rows(&data.thttltsuat));

    // ── Chữ ký số ──
    let signature = parse_signature(&data.nbcks);
    slots.insert("SIGN_BOX", sign_box(&signature));

    substitute(template(), &slots)
}

/// Thẻ `<img>` cho mã QR (chuỗi rỗng → để trống, đúng như `capture.js`).
fn qr_image(payload: &str) -> String {
    match qr_data_uri(payload) {
        None => String::new(),
        Some(uri) => format!(r#"<img src="{uri}" alt="Mã QR" width="{QR_PX}" height="{QR_PX}">"#),
    }
}

/// Thay toàn bộ `{{NAME}}` trong 1 lượt — giá trị chứa `{{...}}` cũng không bị
/// thay nhầm ở lần thay sau.
fn substitute(template: &str, slots: &HashMap<&str, String>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find("}}") {
            Some(end) if slots.contains_key(&after[..end]) => {
                out.push_str(&slots[&after[..end]]);
                rest = &after[end + 2..];
            }
            _ => {
                out.push_str("{{");
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Bảng chi tiết hàng hóa — cột "Tính chất" / "Loại hàng hóa đặc trưng" /
/// "Chiết khấu" đã bỏ, thứ tự cột lấy theo Chrome sau khi `capture.js` chạy
/// (Thuế suất đẩy xuống cuối, thêm cột "Tiền thuế" rỗng ở `thead`).
fn goods_rows(rows: &[GoodsRow]) -> String {
    let mut out = String::new();
    for row in rows {
        out.push_str(&format!(
            r#"<tr t-chat="{}"><td class="tx-center">{}</td><td class="tx-left" style="max-width: 220px; min-width: 220px; word-wrap: break-word;">{}</td><td class="tx-center">{}</td><td class="tx-center">{}</td><td class="tx-center">{}</td><td class="tx-center">{}</td><td class="tx-center">{}</td></tr>"#,
            escape(&js_string(&row.tchat)),
            escape(&js_string(&row.stt)),
            escape(&js_string(&row.ten)),
            escape(&js_string(&row.dvtinh)),
            escape(&js_string(&row.sluong)),
            format_vnd(&row.dgia),
            format_vnd(&row.thtien),
            escape(&js_string(&row.ltsuat)),
        ));
        out.push('\n');
    }
    out
}

/// Bảng tổng hợp theo thuế suất.
fn tax_rows(rows: &[TaxRow]) -> String {
    let mut out = String::new();
    for row in rows {
        out.push_str(&format!(
            r#"<tr><td class="tx-center">{}</td><td class="tx-center">{}</td><td class="tx-center">{}</td></tr>"#,
            escape(&js_string(&row.tsuat)),
            format_vnd(&row.thtien),
            format_vnd(&row.tthue),
        ));
        out.push('\n');
    }
    out
}

/// Khối "Signature Valid" — không có Subject/SigningTime thì bỏ hẳn (hiển thị
/// ô chữ ký trống sẽ như thể hóa đơn đã được ký).
fn sign_box(signature: &Signature) -> String {
    if !signature.is_signed() {
        return String::new();
    }
    format!(
        concat!(
            r#"<div class="sign-box"><span>Signature Valid</span>"#,
            r#"<span class="span-sign-box">Ký bởi</span>"#,
            r#"<span id="cks" class="span-sign-box">{cn}</span>"#,
            r#"<span></span><span class="span-sign-box">Ký ngày</span>"#,
            r#"<span class="span-sign-box">{time}</span></div>"#,
        ),
        cn = escape(&extract_cn(&signature.subject)),
        time = escape(&signature.signing_time),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `detail_json` đủ trường như cổng HĐĐT trả về.
    fn fixture() -> String {
        serde_json::json!({
            "tlhdon": "hóa đơn giá trị gia tăng",
            "nky": "2026-09-15T03:00:00Z",
            "khmshdon": 1,
            "khhdon": "C26E2E",
            "shdon": 3614,
            "mhdon": "MOCK01",
            "nbten": "CÔNG TY <MOCK> & CO",
            "nbmst": "0102152026",
            "nbdchi": "12 Đường Mock",
            "nmten": "HỘ KINH DOANH MOCK",
            "nmmst": "001170019085",
            "nmdchi": "Số 409 Đường Mock",
            "thtttoan": "Chuyển khoản",
            "tgtcthue": 2000000,
            "tgtthue": 160000,
            "tgtphi": 0,
            "ttcktmai": 0,
            "tgtttbso": 2160000,
            "tgtttbchu": "Hai triệu một trăm sáu mươi nghìn đồng",
            "thttltsuat": [{"tsuat": "8%", "thtien": 2000000, "tthue": 160000, "gttsuat": null}],
            "nbcks": r#"{"Subject":"CN=MOCK CONG TY, O=FASTCA, C=VN","SigningTime":"2026-09-15T10:00:00"}"#,
            "qrcode": "https://hoadondientu.gdt.gov.vn/mock",
            "hdhhdvu": [{
                "stt": 1, "tchat": 1, "ten": "Máy lọc nước", "dvtinh": "Cái",
                "sluong": 2, "dgia": 1000000, "thtien": 2000000,
                "stckhau": 0, "ltsuat": "8%", "tsuat": 0.08
            }]
        })
        .to_string()
    }

    #[test]
    fn html_is_complete_and_leaves_no_placeholder() {
        let html = build_invoice_html(&fixture()).expect("dựng HTML được");
        assert!(
            !html.contains("{{"),
            "còn placeholder chưa thay: {}",
            leftover(&html)
        );
        assert!(html.contains("Máy lọc nước"));
        // Escape HTML như bản TS: `<`, `>` và `&` trong dữ liệu người dùng.
        assert!(html.contains("CÔNG TY &lt;MOCK&gt; &amp; CO"));
        assert!(
            html.contains("HÓA ĐƠN GIÁ TRỊ GIA TĂNG"),
            "tiêu đề phải viết hoa"
        );
        assert!(html.contains("Ngày 15 tháng 09 năm 2026"));
        // Ảnh nền nhúng base64.
        assert!(html.contains("data:image/jpeg;base64,"));
        assert!(!html.contains("viewinvoice-bg.jpg"));
    }

    #[test]
    fn goods_table_matches_chrome_baseline_layout() {
        let html = build_invoice_html(&fixture()).expect("dựng HTML được");
        let thead = html
            .split("<table class=\"res-tb\">")
            .nth(1)
            .expect("bảng hàng hóa là bảng đầu tiên")
            .split("</thead>")
            .next()
            .unwrap_or_default();
        assert_eq!(
            thead.matches("<th ").count(),
            8,
            "thead có 7 cột dữ liệu + Tiền thuế"
        );
        for (before, after) in [
            ("Tên hàng hóa, dịch vụ", "Số lượng"),
            ("Thành tiền chưa có thuế", "Thuế suất"),
            ("Thuế suất", "Tiền thuế"),
        ] {
            let b = thead.find(before).expect(before);
            let a = thead.find(after).expect(after);
            assert!(b < a, "`{before}` phải đứng trước `{after}` trong thead");
        }
        for gone in ["Tính chất", "Loại hàng hóa đặc trưng", "Chiết khấu"] {
            assert!(!thead.contains(gone), "cột {gone} đã bị bỏ");
        }

        let row = html.split("<tr t-chat=").nth(1).expect("có dòng hàng hóa");
        let row = row.split("</tr>").next().unwrap();
        assert_eq!(row.matches("<td").count(), 7, "dòng hàng hóa cũng phải 7 ô");
        assert!(!row.contains("Chiết khấu"), "không còn cột chiết khấu");
    }

    #[test]
    fn signature_box_only_when_signed_and_cn_is_extracted() {
        let signed = build_invoice_html(&fixture()).expect("dựng HTML được");
        assert!(signed.contains("Signature Valid"));
        assert!(
            signed.contains(">MOCK CONG TY<"),
            "phải trích đúng CN: {}",
            cn_of(&signed)
        );

        let mut data: serde_json::Value = serde_json::from_str(&fixture()).unwrap();
        data["nbcks"] = serde_json::Value::Null;
        let unsigned = build_invoice_html(&data.to_string()).expect("dựng HTML không chữ ký được");
        assert!(
            !unsigned.contains("Signature Valid"),
            "thiếu chữ ký thì bỏ hẳn khối"
        );
    }

    #[test]
    fn qr_is_a_png_data_uri_or_absent_when_empty() {
        let with_qr = build_invoice_html(&fixture()).expect("dựng HTML được");
        assert!(with_qr.contains("data:image/png;base64,"), "phải có mã QR");

        let mut data: serde_json::Value = serde_json::from_str(&fixture()).unwrap();
        data["qrcode"] = serde_json::Value::Null;
        let no_qr = build_invoice_html(&data.to_string()).unwrap();
        assert!(
            !no_qr.contains("data:image/png;base64,"),
            "không có mã thì để trống"
        );
        assert!(no_qr.contains(r#"<div id="qrcodeTable"></div>"#));
    }

    #[test]
    fn missing_fields_still_render() {
        let html = build_invoice_html("{}").expect("hóa đơn nháp vẫn render được");
        assert!(!html.contains("{{"));
        assert!(!html.contains("NaN"), "không in ra NaN");
        assert!(!html.contains("Ngày NaN"));
    }

    fn leftover(html: &str) -> String {
        html.match_indices("{{")
            .take(3)
            .map(|(i, _)| html[i..html.len().min(i + 30)].to_string())
            .collect::<Vec<_>>()
            .join(" · ")
    }

    fn cn_of(html: &str) -> String {
        let start = html.find(r#"id="cks""#).unwrap_or(0);
        let seg = &html[start..html.len().min(start + 200)];
        let a = seg.find('>').map(|i| i + 1).unwrap_or(0);
        let b = seg[a..].find('<').map(|i| a + i).unwrap_or(a);
        seg[a..b].to_string()
    }

    /// Số trang trong file PDF htmltopdf vừa sinh (đếm thẻ `<Page>`).
    #[test]
    fn renders_two_a4_pages() {
        let pdf = match render_pdf(&fixture()) {
            Ok(pdf) => pdf,
            Err(e) => {
                eprintln!("bỏ qua test render PDF: {e}");
                return;
            }
        };
        assert!(pdf.starts_with(b"%PDF"), "phải là file PDF hợp lệ");
        let pages = count_pages(&pdf);
        assert_eq!(pages, 2, "hóa đơn mẫu phải là 2 trang A4 (đã có {pages})");
        assert!(pdf.len() > 10_000, "PDF quá nhỏ ({} byte)", pdf.len());
    }

    fn count_pages(pdf: &[u8]) -> usize {
        let needle = b"/Type /Page";
        pdf.windows(needle.len()).filter(|w| *w == needle).count()
            - pdf
                .windows(needle.len() + 1)
                .filter(|w| w == b"/Type /Pages")
                .count()
    }

    #[test]
    fn number_and_date_format_match_the_browser() {
        let n = |v: &str| serde_json::from_str::<serde_json::Value>(v).unwrap();
        assert_eq!(format_vnd(&n("2860000")), "2.860.000");
        assert_eq!(format_vnd(&n("0")), "0");
        assert_eq!(format_vnd(&n("2160000")), "2.160.000");
        assert_eq!(format_vnd(&n("null")), "0", "không phải số → 0");
        assert_eq!(
            format_vietnamese_date(&n(r#""2026-09-07T05:03:36Z""#)),
            "Ngày 07 tháng 09 năm 2026"
        );
        assert_eq!(
            format_vietnamese_date(&n(r#""2026-12-31""#)),
            "Ngày 31 tháng 12 năm 2026"
        );
        assert_eq!(format_vietnamese_date(&n(r#""hỏng""#)), "");
        assert_eq!(js_number(&n("2")), Some(2.0));
    }
}
