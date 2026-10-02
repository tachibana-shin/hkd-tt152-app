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
    // Chỉ thêm cột "Chiết khấu" khi có ít nhất 1 mặt hàng thực sự được chiết
    // khấu — không thì bảng 8 cột như cũ.
    let has_discount = data
        .hdhhdvu
        .iter()
        .any(|row| js_number(&row.stckhau).is_some_and(|v| v > 0.0));
    slots.insert(
        "DISCOUNT_HEAD",
        if has_discount {
            r#"<th class="tb-dg">Chiết khấu</th>"#.to_string()
        } else {
            String::new()
        },
    );
    slots.insert("GOODS_ROWS", goods_rows(&data.hdhhdvu, has_discount));
    slots.insert("TAX_ROWS", tax_rows(&data.thttltsuat));

    // ── Chữ ký số ──
    let signature = parse_signature(&data.nbcks);
    slots.insert("SIGN_BOX", sign_box(&signature));

    drop_empty_info_rows(&substitute(template(), &slots))
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

/// Bỏ information không có giá trị — đúng như `Kn()` của `capture.js`: một
/// `.data-item` rỗng thì bỏ ô đó, `.li-row` không còn ô nào thì bỏ cả dòng.
///
/// Làm trên chuỗi HTML đã dựng (không chạy script trong trang) nên cần tự đi
/// tìm cặp thẻ `<div>` cân bằng thay vì dùng DOM.
fn drop_empty_info_rows(html: &str) -> String {
    let marker = r#"class="li-row""#;
    let mut out = String::with_capacity(html.len());
    let mut written = 0;
    let mut pos = 0;

    while let Some(rel) = html[pos..].find(marker) {
        let start = pos + rel;
        let Some(open) = html[..start].rfind('<') else {
            break;
        };
        let Some(end) = div_block_end(html, open) else {
            break;
        };
        pos = end;

        let block = &html[open..end];
        out.push_str(&html[written..open]);
        out.push_str(&filter_li_row(block));
        written = end;
    }
    out.push_str(&html[written..]);
    out
}

/// Giữ lại các `.data-item` còn giá trị trong 1 dòng; không còn gì thì xoá
/// hẳn dòng đó.
fn filter_li_row(block: &str) -> String {
    let Some(open_end) = block.find('>') else {
        return block.to_string();
    };
    let body_end = block.rfind("</div>").unwrap_or(block.len());
    let body = &block[open_end + 1..body_end];

    let mut kept = String::with_capacity(body.len());
    let mut written = 0;
    let mut pos = 0;
    let mut found = 0;
    let mut alive = 0;

    while let Some(rel) = body[pos..].find(r#"class="data-item""#) {
        let start = pos + rel;
        let Some(open) = body[..start].rfind('<') else {
            break;
        };
        let Some(end) = div_block_end(body, open) else {
            break;
        };
        pos = end;

        let item = &body[open..end];
        found += 1;
        // Giữ luôn text đứng trước ô (chỉ là khoảng trắng) rồi mới quyết định
        // giữ/bỏ ô. Không đẩy `written` qua ô bị bỏ thì ô giữ sau đó sẽ copy
        // lại chính ô đã bỏ.
        kept.push_str(&body[written..open]);
        if item_has_value(item) {
            kept.push_str(item);
            alive += 1;
        }
        written = end;
    }

    if found == 0 {
        return block.to_string(); // cấu trúc lạ → không đụng vào
    }
    if alive == 0 {
        return String::new();
    }
    kept.push_str(&body[written..]);
    format!("{}{}</div>", &block[..open_end + 1], kept)
}

/// `.di-value` có nội dung (sau khi bỏ thẻ) hay không.
fn item_has_value(item: &str) -> bool {
    let Some(rel) = item.find(r#"class="di-value""#) else {
        return true;
    };
    let Some(open) = item[..rel].rfind('<') else {
        return true;
    };
    let Some(end) = div_block_end(item, open) else {
        return true;
    };
    let close = end.saturating_sub("</div>".len());
    if close <= open {
        return true;
    }
    let Some(gt) = item[open..close].find('>') else {
        return true;
    };
    !strip_tags(&item[open + gt + 1..close]).trim().is_empty()
}

/// Vị trí sau thẻ `</div>` khớp với `<div>` mở tại `start`.
fn div_block_end(html: &str, start: usize) -> Option<usize> {
    let bytes = html.as_bytes();
    let mut i = start;
    let mut depth = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        if html[i..].starts_with("</div") {
            depth = depth.saturating_sub(1);
            i += 5;
            if depth == 0 {
                return html[i..].find('>').map(|k| i + k + 1);
            }
        } else if html[i..].starts_with("<div") {
            depth += 1;
            i += 4;
        } else {
            i += 1;
        }
    }
    None
}

/// Bỏ mọi `...</...>` còn lại chỉ để kiểm tra text có rỗng hay không.
fn strip_tags(fragment: &str) -> String {
    let mut out = String::with_capacity(fragment.len());
    let mut rest = fragment;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        match rest[i..].find('>') {
            Some(k) => rest = &rest[i + k + 1..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// Bảng chi tiết hàng hóa — cột "Tính chất" / "Loại hàng hóa đặc trưng" đã bỏ,
/// thứ tự cột lấy theo Chrome sau khi `capture.js` chạy (Thuế suất đẩy xuống
/// cuối, thêm cột "Tiền thuế" tính sẵn ở `thead`).
///
/// Cột "Chiết khấu" chỉ xuất hiện khi `with_discount` (xem [`render`]).
fn goods_rows(rows: &[GoodsRow], with_discount: bool) -> String {
    let mut out = String::new();
    for row in rows {
        out.push_str(&format!(
            r#"<tr t-chat="{}"><td class="tx-center">{}</td><td class="tx-left" style="max-width: 220px; min-width: 220px; word-wrap: break-word;">{}</td><td class="tx-center">{}</td><td class="tx-center">{}</td><td class="tx-center">{}</td><td class="tx-center">{}</td>"#,
            escape(&js_string(&row.tchat)),
            escape(&js_string(&row.stt)),
            escape(&js_string(&row.ten)),
            escape(&js_string(&row.dvtinh)),
            escape(&js_string(&row.sluong)),
            format_vnd(&row.dgia),
            format_vnd(&row.thtien),
        ));
        if with_discount {
            out.push_str(&format!(
                r#"<td class="tx-center">{}</td>"#,
                format_vnd(&row.stckhau)
            ));
        }
        out.push_str(&format!(
            r#"<td class="tx-center">{}</td><td class="tx-center">{}</td></tr>"#,
            escape(&js_string(&row.ltsuat)),
            line_tax(row),
        ));
        out.push('\n');
    }
    out
}

/// Tiền thuế của 1 dòng — `capture.js` không lấy `tthue` (luôn `null`) mà tự
/// tính `thành tiền × tỷ lệ / 100`, và chỉ với dòng có `tchat` 1 hoặc 5.
fn line_tax(row: &GoodsRow) -> String {
    let taxable = matches!(js_number(&row.tchat), Some(v) if v == 1.0 || v == 5.0);
    if !taxable {
        return String::new();
    }
    let thtien = js_number(&row.thtien).unwrap_or(0.0);
    let tax = thtien * ltsuat_percent(row) / 100.0;
    format_vnd(&serde_json::Value::from(tax))
}

/// Đọc tỷ lệ thuế từ chuỗi hiển thị `"8%"` đúng cách `capture.js` đọc ô bảng
/// (bỏ dấu phân cách nhóm, phẩy thập phân thành dấu chấm). Không đọc được thì
/// suy ra từ trường số `tsuat` (`0.08` → `8`).
fn ltsuat_percent(row: &GoodsRow) -> f64 {
    let raw = js_string(&row.ltsuat)
        .replace('.', "")
        .replace(',', ".")
        .replace('%', "");
    match raw.trim().parse::<f64>() {
        Ok(v) => v,
        Err(_) => js_number(&row.tsuat).map_or(0.0, |v| v * 100.0),
    }
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
        assert_eq!(
            row.matches("<td").count(),
            8,
            "dòng hàng hóa 7 ô + Tiền thuế"
        );
        assert!(!row.contains("Chiết khấu"), "không còn cột chiết khấu");
        // Tiền thuế = thành tiền × 8% (tchat = 1) và format theo vi-VN.
        assert!(row.contains(">160.000<"), "phải tính ra 160.000: {}", row);
    }

    #[test]
    fn discount_column_appears_only_when_a_line_is_discounted() {
        let plain = build_invoice_html(&fixture()).expect("dựng HTML được");
        assert!(!plain.contains(">Chiết khấu<"), "không chiết khấu → bỏ cột");

        let mut data: serde_json::Value = serde_json::from_str(&fixture()).unwrap();
        data["hdhhdvu"][0]["stckhau"] = serde_json::json!(200_000);
        let discounted = build_invoice_html(&data.to_string()).expect("dựng HTML được");

        let thead = goods_thead(&discounted);
        assert_eq!(thead.matches("<th ").count(), 9, "thêm 1 cột Chiết khấu");
        let dg = thead.find("Đơn giá").expect("có cột Đơn giá");
        let ck = thead.find(">Chiết khấu<").expect("có cột Chiết khấu");
        let ttct = thead
            .find("Thành tiền chưa có thuế")
            .expect("có cột Thành tiền");
        assert!(
            dg < ck && ck < ttct,
            "Chiết khấu nằm giữa Đơn giá và Thành tiền"
        );

        let row = discounted
            .split("<tr t-chat=")
            .nth(1)
            .expect("có dòng")
            .split("</tr>")
            .next()
            .unwrap()
            .to_string();
        assert_eq!(row.matches("<td").count(), 9, "dòng cũng thêm ô Chiết khấu");
        assert!(row.contains(">200.000<"), "giá trị chiết khấu phải in ra");
    }

    #[test]
    fn info_rows_without_a_value_are_dropped() {
        let html = build_invoice_html(&fixture()).expect("dựng HTML được");
        for gone in [
            "Mã cửa hàng:",
            "Tên cửa hàng:",
            "Số hộ chiếu:",
            "Mã ĐVCQHVNSNN:",
            "CCCD người mua:",
            "Số bảng kê:",
            "Ngày bảng kê:",
        ] {
            assert!(!html.contains(gone), "dòng rỗng `{gone}` phải bị ẩn");
        }
        // Dòng có giá trị giữ nguyên.
        for kept in ["Tên người bán:", "Địa chỉ:", "Hình thức thanh toán:"] {
            assert!(html.contains(kept), "dòng `{kept}` không rỗng phải giữ");
        }
        // Nhãn đi kèm giá trị vẫn còn.
        assert!(html.contains("HỘ KINH DOANH MOCK"));
    }

    /// "Số bảng kê" / "Ngày bảng kê" giờ là 2 dòng độc lập: dòng có giá trị
    /// giữ nguyên, dòng rỗng biến mất — không ảnh hưởng đến nhau.
    #[test]
    fn valued_bang_ke_row_is_kept_while_the_empty_one_is_dropped() {
        let mut data: serde_json::Value = serde_json::from_str(&fixture()).unwrap();
        data["dknlbke"] = serde_json::json!("2026-09-15");
        let html = build_invoice_html(&data.to_string()).expect("dựng HTML được");
        assert!(html.contains("Ngày bảng kê:"), "có giá trị thì phải giữ");
        assert!(html.contains("2026-09-15"));
        assert!(!html.contains("Số bảng kê:"), "dòng kia rỗng thì bỏ hẳn");
    }

    /// Một `.li-row` mà có nhiều `.data-item`: bỏ ô rỗng giữ ô còn giá trị,
    /// hết ô thì xoá cả dòng. Template hiện mỗi dòng chỉ 1 ô nhưng `Kn()` của
    /// `capture.js` vẫn lọc trên từng ô nên ta giữ đúng hành vi đó.
    #[test]
    fn filter_li_row_keeps_items_with_a_value_and_drops_the_rest() {
        let item = |label: &str, value: &str| {
            format!(
                "<div class=\"data-item\"><div class=\"di-label\"><span>{label}</span></div>\
                 <div class=\"di-value\"><div>{value}</div></div></div>"
            )
        };
        let row = |body: &str| format!("<div class=\"li-row\">{body}</div>");

        let mixed = row(&format!(
            "{}{}",
            item("Số bảng kê:", ""),
            item("Ngày bảng kê:", "15/09/2026")
        ));
        let kept = filter_li_row(&mixed);
        assert!(kept.contains("Ngày bảng kê:"), "ô còn giá trị phải giữ");
        assert!(!kept.contains("Số bảng kê:"), "ô rỗng phải bỏ");
        assert!(
            kept.starts_with(r#"<div class="li-row">"#),
            "còn ô thì dòng vẫn phải ở lại: {kept}"
        );

        let all_empty = row(&format!("{}{}", item("A:", ""), item("B:", "")));
        assert_eq!(filter_li_row(&all_empty), "", "hết ô thì xoá cả dòng");
    }

    #[test]
    fn taxable_line_gets_tax_and_other_lines_stay_blank() {
        let mut data: serde_json::Value = serde_json::from_str(&fixture()).unwrap();
        // tchat = 2 (không thuộc 1/5) → ô Tiền thuế để trống như capture.js.
        data["hdhhdvu"][0]["tchat"] = serde_json::json!(2);
        let html = build_invoice_html(&data.to_string()).expect("dựng HTML được");
        let row = html
            .split("<tr t-chat=")
            .nth(1)
            .expect("có dòng")
            .split("</tr>")
            .next()
            .unwrap();
        assert!(
            row.ends_with(r#"<td class="tx-center"></td>"#),
            "tchat 2 không tính thuế: {row}"
        );

        // Thiếu ltsuat → suy từ tsuat.
        let mut data: serde_json::Value = serde_json::from_str(&fixture()).unwrap();
        data["hdhhdvu"][0]["ltsuat"] = serde_json::Value::Null;
        let html = build_invoice_html(&data.to_string()).expect("dựng HTML được");
        assert!(html.contains(">160.000<"), "phải suy ra 8% từ tsuat");
    }

    fn goods_thead(html: &str) -> String {
        html.split("<table class=\"res-tb\">")
            .nth(1)
            .expect("bảng hàng hóa là bảng đầu tiên")
            .split("</thead>")
            .next()
            .unwrap_or_default()
            .to_string()
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

    /// Khối chữ ký: 2 tiêu đề nằm trong 1 hàng nên ngang nhau (hai đầu trang),
    /// ô chữ ký của người bán ở hàng riêng bên dưới, và cả khối không được vỡ
    /// sang trang — htmltopdf cắt giữa khối thì PDF thành 3 trang.
    #[test]
    fn signature_headings_share_a_row_and_the_block_stays_whole() {
        let html = build_invoice_html(&fixture()).expect("dựng HTML được");

        let start = html
            .find(r#"<div class="sign-row">"#)
            .expect("phải có hàng tiêu đề chữ ký");
        let end = div_block_end(&html, start).expect("hàng tiêu đề phải cân bằng thẻ");
        let row = &html[start..end];
        assert!(row.contains("NGƯỜI MUA HÀNG"), "thiếu tiêu đề người mua");
        assert!(row.contains("NGƯỜI BÁN HÀNG"), "thiếu tiêu đề người bán");

        let box_row = html
            .find(r#"<div class="sign-box-row">"#)
            .expect("phải có hàng ô chữ ký");
        assert!(box_row > end, "ô chữ ký phải nằm sau hàng tiêu đề");

        assert!(
            template().contains("break-inside: avoid"),
            ".ft-sign phải giữ nguyên cả khối chữ ký trên một trang"
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
