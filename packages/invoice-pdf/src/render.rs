//! Dựng HTML hóa đơn từ `detail_json` rồi render ra PDF.
//!
//! Toàn bộ pipeline nằm trong backend: không có script chạy trong trang, không
//! cần Chrome/Font nhúng — htmltopdf tự layout, tự nhúng font hệ thống và tự
//! tách trang A4.

use crate::data::{GoodsRow, InvoiceData, TtKhac};
use crate::format::{
    escape, extract_cn, format_vietnamese_date, format_vnd, js_number, js_string, parse_signature,
    Signature,
};
use crate::kind::Kind;
use crate::qr::{qr_data_uri, QR_PX};
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
pub fn build_invoice_html(detail_json: &str) -> Result<String, String> {
    let data: InvoiceData = serde_json::from_str(detail_json)
        .map_err(|e| format!("Không đọc được chi tiết hóa đơn (detail_json): {e}"))?;
    Ok(render(&data))
}

/// Render PDF cho 1 hóa đơn từ chuỗi `detail_json`.
pub fn render_pdf(detail_json: &str) -> Result<Vec<u8>, String> {
    let html = build_invoice_html(detail_json)?;
    htmltopdf::Engine::new()
        .render_html(&html, htmltopdf::RenderOptions::default())
        .map_err(|e| format!("Không render được PDF hóa đơn: {e}"))
}

fn render(data: &InvoiceData) -> String {
    let mut slots: HashMap<&'static str, String> = HashMap::new();

    // ── Đầu hóa đơn ──
    // Tiêu đề lấy theo dữ liệu; thiếu thì tra tên loại theo ký hiệu mẫu
    // (Phụ lục I Thông tư 91/2026/TT-BTC) — mọi kiểu hóa đơn đều có tên.
    slots.insert("INVOICE_TITLE", escape(&invoice_title(data).to_uppercase()));
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
    ] {
        slots.insert(key, escape(&js_string(value)));
    }

    // ── Thông tin đặc thù theo kiểu hóa đơn ──
    slots.insert("EXTRA_INFO_ROWS", extra_info_rows(data));

    // ── Mã QR ──
    slots.insert("QR_IMG", qr_image(&js_string(&data.qrcode)));

    // ── Bảng ──
    // Chỉ thêm cột "Chiết khấu" khi có ít nhất 1 mặt hàng thực sự được chiết
    // khấu — không thì bảng gọn lại như cũ.
    let has_discount = data
        .hdhhdvu
        .iter()
        .any(|row| js_number(&row.stckhau).is_some_and(|v| v > 0.0));
    // Cột "Thuế suất" chỉ cần khi hàng hóa xài nhiều tỉ lệ khác nhau; còn 1
    // tỉ lệ duy nhất thì con số đó nói được ngay ở dòng tổng cộng.
    let has_rate = has_multiple_rates(&data.hdhhdvu);
    // Cột "Chiết khấu" chỉ thêm khi có ít nhất 1 mặt hàng thực sự được chiết
    // khấu — không thì bảng gọn lại như cũ. Tiêu đề đứng sau "Thành tiền" cho
    // đúng thứ tự xuất dữ liệu của `goods_rows` (thành tiền trước chiết khấu).
    slots.insert(
        "DISCOUNT_HEAD",
        if has_discount {
            r#"<th class="tb-ck">Chiết khấu</th>"#.to_string()
        } else {
            String::new()
        },
    );
    slots.insert(
        "RATE_HEAD",
        if has_rate {
            r#"<th class="tb-ts">Thuế suất</th>"#.to_string()
        } else {
            String::new()
        },
    );
    // Bề rộng cột: htmltopdf bỏ qua `width` trên `<th>`, chỉ bộ quy tắc
    // `col.colN { width }` đọc từ CSS mới được nó dùng.
    slots.insert("COL_WIDTHS", col_width_css(data, has_discount, has_rate));
    slots.insert(
        "GOODS_ROWS",
        goods_rows(&data.hdhhdvu, has_discount, has_rate),
    );
    // Tổng cộng nằm ngay trong bảng hàng hóa — 2 bảng con (thuế suất + thành
    // tiền) đã gộp vào đây.
    slots.insert("SUMMARY_ROWS", summary_rows(data, has_discount, has_rate));

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

/// Tiêu đề hóa đơn: `tlhdon` → `thdon` → tên loại theo ký hiệu mẫu → `"HÓA ĐƠN"`.
///
/// Cổng HĐĐT luôn gửi `tlhdon`, nhưng dữ liệu thiếu/sai kiểu thì vẫn còn 2 lớp
/// dự phòng để không in ra một dòng trống.
fn invoice_title(data: &InvoiceData) -> String {
    let kind = Kind::detect(&data.khmshdon);
    [
        js_string(&data.tlhdon),
        js_string(&data.thdon),
        kind.name(&data.khhdon).to_string(),
    ]
    .into_iter()
    .find(|title| !title.trim().is_empty())
    .unwrap_or_else(|| "HÓA ĐƠN".to_string())
}

/// Danh sách thông tin được phép in ra: (nhãn hiển thị, khóa thay thế).
///
/// Cổng HĐĐT không định nghĩa sẵn nội dung của `ttkhac` — đó là túi mở rộng do
/// phần mềm người bán điền, và thực tế chứa cả khóa **nội bộ**
/// (`StockTotalAmount`, `RowType`, `SellerAddress`…). Chỉ những `ttruong` khớp
/// đúng nhãn dưới đây mới được in; mọi khóa khác bị bỏ qua tuyệt đối.
///
/// Nhãn lấy theo bài tham khảo mục 3 (vận tải, khách sạn, tiền điện/nước, vé
/// máy bay); thêm một dòng ở đây là thêm một trường hiển thị.
const EXTRA_FIELDS: &[(&str, &[&str])] = &[
    // Vận tải (mục 3.3): tên tàu, vận đơn, quốc tịch, ngày đến, ngày đi.
    ("Tên tàu", &["tentau", "shipname", "vesselname"]),
    ("Vận đơn", &["vandon", "waybill"]),
    ("Quốc tịch", &["quoctich", "nationality"]),
    ("Ngày đến", &["ngayden", "checkindate", "arrivaldate"]),
    ("Ngày đi", &["ngaydi", "checkoutdate", "departuredate"]),
    // Khách sạn (mục 3.4): số phòng, ngày đến, ngày đi.
    ("Số phòng", &["sophong", "roomnumber", "roomno"]),
    // Tiền điện, tiền nước (mục 3.1, 3.2): chỉ số và sản lượng.
    ("Chỉ số điện", &["chisodien", "meterelectric"]),
    ("Sản lượng điện", &["sanluongdien"]),
    ("Chỉ số nước", &["chisonuoc", "meternuoc"]),
    ("Sản lượng nước", &["sanluongnuoc"]),
    // Đại lý vé máy bay (mục 3.5): thông tin đặc thù của vé máy bay.
    ("Số hiệu chuyến bay", &["sohieuchuyenbay", "flightno"]),
    ("Điểm đi", &["diemdi", "departure"]),
    ("Điểm đến", &["diemden", "arrival"]),
];

/// Khối `.li-row` cho một dòng thông tin — cùng khuôn với khối trong template
/// để `drop_empty_info_rows` xử lý như nhau.
fn info_row(label: &str, value: &str) -> String {
    format!(
        r#"<div class="li-row"><div class="data-item"><div class="di-label"><span>{label}:</span></div><div class="di-value"><div>{value}</div></div></div></div>"#,
        label = escape(label),
        value = escape(value),
    )
}

/// Tìm giá trị của một nhãn trong `ttkhac` — khớp không phân biệt hoa thường,
/// kể cả chữ Việt hoa/thường.
fn find_extra(items: &[TtKhac], label: &str, aliases: &[&str]) -> Option<String> {
    let wanted: Vec<String> = std::iter::once(label)
        .chain(aliases.iter().copied())
        .map(|key| key.trim().to_lowercase())
        .collect();
    items.iter().find_map(|item| {
        let key = js_string(&item.ttruong).trim().to_lowercase();
        if !wanted.contains(&key) {
            return None;
        }
        // Bỏ qua mục rỗng để còn lấy mục sau trùng tên — dữ liệu thật có cả
        // "Ghi chú" rỗng lẫn "Ghi chú hóa đơn" có nội dung trên cùng hóa đơn.
        let value = js_string(&item.dlieu).trim().to_string();
        (!value.is_empty()).then_some(value)
    })
}

/// Dòng thông tin thêm vào cuối khối thông tin (ngay trước bảng hàng hóa):
///
/// - thông tin đặc thù theo lĩnh vực lấy từ `ttkhac` (xem [`EXTRA_FIELDS`]);
/// - đồng tiền + tỷ giá (Điều 10.7 NĐ 254/2026) khi hóa đơn không bằng VND;
/// - phí, lệ phí (Điều 10.6 NĐ 254/2026) khi có và khác 0;
/// - ghi chú của người bán (`Ghi chú` / `Ghi chú hóa đơn`) khi có nội dung.
///
/// Chỉ sinh dòng khi có giá trị thật — dòng rỗng không bao giờ được tạo ra.
fn extra_info_rows(data: &InvoiceData) -> String {
    let mut out = String::new();

    for (label, aliases) in EXTRA_FIELDS {
        if let Some(value) = find_extra(&data.ttkhac, label, aliases) {
            out.push_str(&info_row(label, &value));
        }
    }

    // Điều 10.7 — đồng tiền thể hiện trên hóa đơn.
    let dvtte = js_string(&data.dvtte);
    if !dvtte.trim().is_empty() && !dvtte.eq_ignore_ascii_case("VND") {
        out.push_str(&info_row("Đơn vị tiền tệ", &dvtte));
        let tgia = js_number(&data.tgia).unwrap_or(1.0);
        if (tgia - 1.0).abs() > f64::EPSILON {
            out.push_str(&info_row(
                "Tỷ giá",
                &format_vnd(&serde_json::Value::from(tgia)),
            ));
        }
    }

    // Điều 10.6 — phí, lệ phí thuộc ngân sách nhà nước.
    if let Some(phi) = js_number(&data.tgtphi).filter(|phi| *phi > 0.0) {
        out.push_str(&info_row(
            "Phí, lệ phí",
            &format_vnd(&serde_json::Value::from(phi)),
        ));
    }

    // "Nội dung khác liên quan (nếu có)" (Điều 10.7) — ghi chú của người bán,
    // đặt cuối cùng vì nó là câu chữ chứ không phải ô thông tin.
    if let Some(note) = find_extra(
        &data.ttkhac,
        "Ghi chú",
        &["ghichu", "ghi chu", "ghi chú hóa đơn", "ghichuhoadon"],
    ) {
        out.push_str(&info_row("Ghi chú", &note));
    }

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
/// "Đơn vị tính" viết tắt thành "ĐVT", "Thành tiền" lược bỏ phần "chưa có thuế
/// GTGT". Không còn cột "Tiền thuế": tổng tiền thuế nằm ở dòng tổng cộng dưới
/// cùng của bảng.
///
/// Cột "Chiết khấu" / "Thuế suất" lần lượt hiện khi `with_discount` /
/// `with_rate` (xem [`render`]). Tên hàng không ép bề rộng — cột này tự nở theo
/// nội dung nên luôn là cột rộng nhất.
fn goods_rows(rows: &[GoodsRow], with_discount: bool, with_rate: bool) -> String {
    let mut out = String::new();
    for row in rows {
        out.push_str(&format!(
            r#"<tr t-chat="{}"><td class="tx-center">{}</td><td class="tx-left">{}</td><td class="tx-center">{}</td><td class="tx-center">{}</td><td class="tx-center">{}</td><td class="tx-center">{}</td>"#,
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
        if with_rate {
            out.push_str(&format!(
                r#"<td class="tx-center">{}</td>"#,
                escape(&rate_label(row))
            ));
        }
        out.push_str("</tr>\n");
    }
    out
}

/// Các dòng tổng cộng — nối ngay sau hàng hóa để gộp 2 bảng con cũ (thuế suất
/// và thành tiền) thành 1 bảng.
///
/// Cột chia đôi bảng: nhãn chiếm nửa trái (căn trái, tất cả các nhãn nằm trên
/// cùng 1 đường), giá trị chiếm nửa phải (căn phải, các giá trị kết thúc trên
/// cùng 1 đường). Hàng thuế tách thêm 1 nhãn riêng cho từng bên nên thành 4 ô —
/// "Thuế suất GTGT" và "Tiền thuế GTGT" mỗi nhãn chỉ viết 1 lần; nhiều tỉ lệ
/// thuế thì mỗi tỉ lệ ra 1 dòng riêng (nhãn các dòng sau để trống cho thẳng
/// hàng) — không xếp chồng trong 1 ô được vì htmltopdf bỏ `<br>` khi dựng ô
/// bảng (`collect_cell_runs_into` trong `html.rs`) làm các giá trị dính nhau.
fn summary_rows(data: &InvoiceData, with_discount: bool, with_rate: bool) -> String {
    let cols = 6 + usize::from(with_discount) + usize::from(with_rate);
    let half = cols / 2;

    let mut out = String::new();
    let line = |label: &str, value: String| {
        format!(
            r#"<tr><td colspan="{half}" class="tx-left">{label}</td><td colspan="{}" class="tx-right">{value}</td></tr>"#,
            cols - half
        )
    };
    out.push_str(&line("Cộng tiền hàng", format_vnd(&data.tgtcthue)));
    out.push('\n');

    for (i, rate) in data.thttltsuat.iter().enumerate() {
        let first = i == 0;
        out.push_str(&format!(
            r#"<tr><td colspan="{}" class="tx-left">{}</td><td class="tx-center">{}</td><td colspan="{}" class="tx-left">{}</td><td class="tx-right">{}</td></tr>"#,
            half - 1,
            if first { "Thuế suất GTGT" } else { "" },
            escape(&js_string(&rate.tsuat)),
            cols - half - 1,
            if first { "Tiền thuế GTGT" } else { "" },
            format_vnd(&rate.tthue),
        ));
        out.push('\n');
    }

    out.push_str(&line(
        "Tổng tiền thanh toán bằng số",
        format_vnd(&data.tgtttbso),
    ));
    out.push('\n');
    out.push_str(&line(
        "Số tiền viết bằng chữ",
        escape(&js_string(&data.tgtttbchu)),
    ));
    out
}

/// Bề rộng khung nội dung của trang A4 với lề ngang 21pt: 595 − 2 × 21 = 553.
/// htmltopdf chỉ dùng bộ `col.colN { width }` khi tổng của chúng **không lớn
/// hơn** bề rộng này nên nó cũng là ngân sách cho cả bảng.
const FRAME_WIDTH_PT: u32 = 553;

/// Cỡ chữ mà htmltopdf dùng để đo ô bảng — bằng cỡ chữ thân trang (13pt).
const CELL_FONT_PT: f32 = 13.0;

/// Padding ngang của 1 ô: `padding: 3px 4px` = 4px mỗi bên = 6pt. Engine trừ
/// phần này khỏi bề rộng cột trước khi ngắt dòng nên phải cộng lại.
const CELL_PADDING_X_PT: f32 = 6.0;

/// Sàn cho cột Tên hàng hóa (pt). Bề rộng các cột còn lại vượt ngân sách thì
/// cắt về sàn của từng cột để giữ được mức này — dưới đó tên hàng vỡ nát thì
/// thà nhường chỗ cho engine tự bố trí còn hơn.
const MIN_NAME_WIDTH_PT: u32 = 100;

/// Bề rộng từng cột bảng hàng hóa (pt), đúng thứ tự `<th>` trong template:
/// STT, Tên hàng hóa, ĐVT, Số lượng, Đơn giá, Thành tiền, [Chiết khấu],
/// [Thuế suất].
///
/// Cột số lấy bề rộng theo **số liệu thật của hóa đơn**: htmltopdf chỉ chịu
/// xếp bộ `col.colN` khi mỗi cột rộng ít nhất bằng bề rộng nó tự đo (xem
/// [`estimate_text_width_pt`]), hẹp hơn là nó bỏ hết và tự bố trí lại — khi
/// đấy các dòng tổng cộng có chữ dài sẽ kéo cột số nở to ra. Sàn mỗi cột thì đủ
/// để tiêu đề không vỡ dòng (đã đo trực tiếp trên engine); cột Tên hàng hóa
/// nhận phần dư nên luôn rộng nhất.
fn column_widths(data: &InvoiceData, has_discount: bool, has_rate: bool) -> Vec<u32> {
    let cols = 6 + usize::from(has_discount) + usize::from(has_rate);
    // Sàn theo thứ tự cột, đo trực tiếp trên htmltopdf: số lượng 56pt là vỡ
    // tiêu đề "Số lượng", chiết khấu 60pt vỡ "Chiết khấu"…
    let mut floors = vec![32, 0, 36, 60, 64, 64]; // STT, Tên, ĐVT, SL, ĐG, TT
    if has_discount {
        floors.push(65); // Chiết khấu
    }
    if has_rate {
        floors.push(64); // Thuế suất
    }
    debug_assert_eq!(floors.len(), cols, "sàn cột khớp số cột");

    let mut needs = vec![0f32; cols];
    for row in &data.hdhhdvu {
        widen(&mut needs[0], &js_string(&row.stt));
        widen(&mut needs[2], &js_string(&row.dvtinh));
        widen(&mut needs[3], &js_string(&row.sluong));
        widen(&mut needs[4], &format_vnd(&row.dgia));
        widen(&mut needs[5], &format_vnd(&row.thtien));
        if has_discount {
            widen(&mut needs[6], &format_vnd(&row.stckhau));
        }
        if has_rate {
            widen(&mut needs[cols - 1], &rate_label(row));
        }
    }
    // Dòng tổng hợp: ô tỉ lệ thuế nằm ở cột cuối bên trái, ô tiền thuế nằm ở
    // cột cuối cùng (cùng vị trí với `summary_rows`).
    let half = cols / 2;
    for rate in &data.thttltsuat {
        widen(&mut needs[half - 1], &js_string(&rate.tsuat));
        widen(&mut needs[cols - 1], &format_vnd(&rate.tthue));
    }

    let mut widths: Vec<u32> = needs
        .iter()
        .zip(&floors)
        .map(|(need, floor)| (*need as u32).max(*floor))
        .collect();
    // Bắt buộc chừa đủ chỗ cho cột Tên: cắt dần phần dư của cột rộng nhất
    // (trừ Tên) về tới sàn của nó.
    let mut others: u32 = widths
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != 1)
        .map(|(_, w)| *w)
        .sum();
    let mut over = (others + MIN_NAME_WIDTH_PT).saturating_sub(FRAME_WIDTH_PT);
    while over > 0 {
        let widest = (0..cols)
            .filter(|i| *i != 1)
            .max_by_key(|i| widths[*i] - floors[*i])
            .expect("bảng có cột ngoài Tên hàng hóa");
        let surplus = widths[widest] - floors[widest];
        if surplus == 0 {
            break; // mọi cột đã về sàn — số liệu quá khổ, đành chịu vỡ chữ
        }
        let cut = surplus.min(over);
        widths[widest] -= cut;
        others -= cut;
        over -= cut;
    }
    widths[1] = FRAME_WIDTH_PT - others;
    widths
}

/// Ghi nhớ bề rộng lớn nhất mà 1 chuỗi đòi hỏi cho ô của nó.
fn widen(slot: &mut f32, text: &str) {
    *slot = slot.max(cell_width_pt(text));
}

/// Bề rộng khai báo cho 1 ô chứa `text` (pt) — làm tròn lên và cộng 1pt dư để
/// việc làm tròn không làm htmltopdf tưởng chữ chưa vừa rồi ngắt giữa chừng.
fn cell_width_pt(text: &str) -> f32 {
    (estimate_text_width_pt(text) + CELL_PADDING_X_PT + 1.0).ceil()
}

/// Ước bề rộng của `text` khi htmltopdf đo (pt).
///
/// Engine không lộ API đo chữ nên mình tái hiện cách nó đo: bảng ký tự kiểu
/// Helvetica ở cỡ 13pt. Hiệu chuẩn bằng cách bisect bề rộng khai báo cho tới
/// khi engine chịu xếp (xem test `col_width_rules_cover_every_column_…`):
/// `"2.860.000"` ≈ 57.8pt và `"1.234.567.890"` ≈ 83.1pt — khớp đúng
/// 0.556em/chữ số và 0.278em/dấu chấm. Ký tự tiếng Việt không có trong bảng
/// lấy 0.65em (hơi rộng hơn thật → dư an toàn).
fn estimate_text_width_pt(text: &str) -> f32 {
    text.chars().map(|c| char_em(c) * CELL_FONT_PT).sum()
}

/// Bề rộng 1 ký tự tính bằng em (theo bảng Helvetica).
fn char_em(c: char) -> f32 {
    const UPPER: [f32; 26] = [
        0.667, 0.667, 0.722, 0.722, 0.667, 0.611, 0.778, 0.722, 0.278, 0.5, 0.667, 0.556, 0.833,
        0.722, 0.778, 0.667, 0.778, 0.722, 0.667, 0.611, 0.722, 0.667, 0.944, 0.667, 0.667, 0.611,
    ];
    const LOWER: [f32; 26] = [
        0.556, 0.556, 0.5, 0.556, 0.556, 0.278, 0.556, 0.556, 0.222, 0.222, 0.5, 0.222, 0.833,
        0.556, 0.556, 0.556, 0.556, 0.333, 0.5, 0.278, 0.556, 0.5, 0.722, 0.5, 0.5, 0.5,
    ];
    match c {
        '0'..='9' => 0.556,
        '.' | ',' => 0.278,
        '%' => 0.889,
        ' ' => 0.278,
        'A'..='Z' => UPPER[(c as u8 - b'A') as usize],
        'a'..='z' => LOWER[(c as u8 - b'a') as usize],
        c if c.is_ascii() => 0.278, // gạch, hai chấm, ngoặc… đều hẹp
        _ => 0.65,                  // tiếng Việt có dấu: dư một chút
    }
}

/// Bộ quy tắc `col.colN { width }` mà htmltopdf đọc từ CSS text, hoặc chuỗi
/// rỗng khi cột Tên hàng hóa không còn chỗ (lúc đó để engine tự bố trí).
fn col_width_css(data: &InvoiceData, has_discount: bool, has_rate: bool) -> String {
    let widths = column_widths(data, has_discount, has_rate);
    if widths[1] < MIN_NAME_WIDTH_PT {
        return String::new();
    }
    widths
        .iter()
        .enumerate()
        .map(|(i, w)| format!("        col.col{i} {{ width: {w}pt }}\n"))
        .collect()
}

/// Hàng hóa có xài từ 2 tỉ lệ thuế GTGT khác nhau trở lên hay không — chỉ khi
/// đó cột "Thuế suất" mới đáng để xuất hiện.
fn has_multiple_rates(rows: &[GoodsRow]) -> bool {
    let mut seen: Vec<f64> = Vec::new();
    for row in rows {
        let percent = ltsuat_percent(row);
        if seen
            .iter()
            .any(|other| (*other - percent).abs() < f64::EPSILON)
        {
            continue;
        }
        seen.push(percent);
        if seen.len() >= 2 {
            return true;
        }
    }
    false
}

/// Nhãn hiển thị của tỉ lệ thuế: dùng chuỗi `ltsuat` có sẵn (`"8%"`), thiếu thì
/// suy ra từ `tsuat` (`0.08` → `"8%"`).
fn rate_label(row: &GoodsRow) -> String {
    let raw = js_string(&row.ltsuat);
    if !raw.trim().is_empty() {
        return raw;
    }
    format!("{}%", ltsuat_percent(row))
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

/// Khối "Signature Valid" — không có Subject/SigningTime thì bỏ hẳn (hiển thị
/// ô chữ ký trống sẽ như thể hóa đơn đã được ký). Mỗi mục 1 dòng có nhãn, đúng
/// kiểu `Signature Valid` / `Ký bởi: …` / `Ký ngày: …`.
fn sign_box(signature: &Signature) -> String {
    if !signature.is_signed() {
        return String::new();
    }
    format!(
        concat!(
            r#"<div class="sign-box"><div>Signature Valid</div>"#,
            r#"<div>Ký bởi: {cn}</div>"#,
            r#"<div>Ký ngày: {date}</div></div>"#,
        ),
        cn = escape(&extract_cn(&signature.subject)),
        date = escape(&format_sign_day(&signature.signing_time)),
    )
}

/// Ngày ký `"2026-09-15T10:00:00"` → `"15/09/2026"`; không phải ngày ISO thì
/// giữ nguyên chuỗi gốc.
fn format_sign_day(iso: &str) -> String {
    let b = iso.as_bytes();
    if b.len() >= 10 && b[4] == b'-' && b[7] == b'-' {
        format!("{}/{}/{}", &iso[8..10], &iso[5..7], &iso[0..4])
    } else {
        iso.to_string()
    }
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
        let thead = goods_thead(&html);
        assert_eq!(
            thead.matches("<th ").count(),
            6,
            "cột cơ bản: STT, Tên, ĐVT, Số lượng, Đơn giá, Thành tiền"
        );
        for (before, after) in [
            ("Tên hàng hóa, dịch vụ", "Số lượng"),
            ("Số lượng", "Đơn giá"),
            ("Đơn giá", "Thành tiền"),
        ] {
            let b = thead.find(before).expect(before);
            let a = thead.find(after).expect(after);
            assert!(b < a, "`{before}` phải đứng trước `{after}` trong thead");
        }
        for gone in [
            "Tính chất",
            "Loại hàng hóa đặc trưng",
            "Chiết khấu",
            "Tiền thuế",
            "Đơn vị tính", // viết tắt thành ĐVT
            "Thành tiền chưa có thuế",
        ] {
            assert!(!thead.contains(gone), "cột {gone} đã bị bỏ/đổi");
        }
        assert!(thead.contains(">ĐVT<"), "Đơn vị tính viết tắt là ĐVT");

        let row = html.split("<tr t-chat=").nth(1).expect("có dòng hàng hóa");
        let row = row.split("</tr>").next().unwrap();
        assert_eq!(row.matches("<td").count(), 6, "dòng hàng hóa đúng 6 ô");
        assert!(!row.contains("Chiết khấu"), "không có chiết khấu → bỏ ô");
        // Tên hàng không bị ép bề rộng: cột này tự nở và là cột rộng nhất.
        assert!(
            !row.contains("max-width"),
            "không ép width lên ô Tên hàng hóa: {row}"
        );
    }

    #[test]
    fn discount_column_appears_only_when_a_line_is_discounted() {
        let plain = build_invoice_html(&fixture()).expect("dựng HTML được");
        assert!(!plain.contains(">Chiết khấu<"), "không chiết khấu → bỏ cột");

        let mut data: serde_json::Value = serde_json::from_str(&fixture()).unwrap();
        data["hdhhdvu"][0]["stckhau"] = serde_json::json!(200_000);
        let discounted = build_invoice_html(&data.to_string()).expect("dựng HTML được");

        let thead = goods_thead(&discounted);
        assert_eq!(thead.matches("<th ").count(), 7, "thêm 1 cột Chiết khấu");
        let dg = thead.find("Đơn giá").expect("có cột Đơn giá");
        let ck = thead.find(">Chiết khấu<").expect("có cột Chiết khấu");
        let ttct = thead.find(">Thành tiền<").expect("có cột Thành tiền");
        assert!(
            dg < ttct && ttct < ck,
            "Thành tiền nằm giữa Đơn giá và Chiết khấu"
        );

        let row = discounted
            .split("<tr t-chat=")
            .nth(1)
            .expect("có dòng")
            .split("</tr>")
            .next()
            .unwrap()
            .to_string();
        assert_eq!(row.matches("<td").count(), 7, "dòng cũng thêm ô Chiết khấu");
        assert!(row.contains(">200.000<"), "giá trị chiết khấu phải in ra");
        // Số liệu phải rơi đúng cột: thành tiền (2.000.000) đứng trước ô
        // chiết khấu (200.000) — cùng thứ tự với hàng tiêu đề phía trên.
        let tt = row.find(">2.000.000<").expect("có số thành tiền");
        let ck_at = row.find(">200.000<").expect("có số chiết khấu");
        assert!(tt < ck_at, "ô thành tiền trước ô chiết khấu trong dòng");
    }

    /// Cột "Thuế suất" chỉ xuất hiện khi hàng hóa xài từ 2 tỉ lệ thuế khác
    /// nhau; 1 tỉ lệ duy nhất thì con số đó nói được ở dòng tổng cộng.
    #[test]
    fn rate_column_shows_up_only_for_two_different_rates() {
        let plain = build_invoice_html(&fixture()).expect("dựng HTML được");
        assert!(
            !goods_thead(&plain).contains(">Thuế suất<"),
            "một tỉ lệ duy nhất → không cần cột Thuế suất"
        );
        assert!(
            plain.split("<tr t-chat=").count() - 1 == 1,
            "dòng hàng hóa không kèm ô tỉ lệ"
        );

        let mut data: serde_json::Value = serde_json::from_str(&fixture()).unwrap();
        data["hdhhdvu"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "stt": 2, "tchat": 1, "ten": "Khác tỉ lệ", "dvtinh": "Cái",
                "sluong": 1, "dgia": 500000, "thtien": 500000,
                "stckhau": 0, "ltsuat": "5%", "tsuat": 0.05
            }));
        let two_rates = build_invoice_html(&data.to_string()).expect("dựng HTML được");
        assert!(
            goods_thead(&two_rates).contains(">Thuế suất<"),
            "2 tỉ lệ khác nhau → hiện cột Thuế suất"
        );
        let row = two_rates
            .split("<tr t-chat=")
            .nth(2)
            .expect("có dòng thứ 2");
        assert!(
            row.split("</tr>").next().unwrap().contains(">5%"),
            "dòng thứ 2 mang tỉ lệ 5%: {row}"
        );
    }

    /// htmltopdf bỏ qua `width` trên `<th>` — chỉ bộ `col.colN { width }` đọc
    /// từ CSS mới được nó dùng, và chỉ khi tổng bằng đúng bề rộng khung. Mỗi
    /// biến thể cột đều phải có đủ quy tắc, cột Tên hàng hóa rộng nhất, còn
    /// các cột số thì bám sát bề rộng chữ của số liệu (sàn = bề rộng tiêu đề).
    #[test]
    fn col_width_rules_cover_every_column_and_fill_the_frame() {
        let base: serde_json::Value = serde_json::from_str(&fixture()).unwrap();
        let mut discounted = base.clone();
        discounted["hdhhdvu"][0]["stckhau"] = serde_json::json!(200_000);
        let mut two_rates = base.clone();
        two_rates["hdhhdvu"]
            .as_array_mut()
            .expect("mảng hàng hóa")
            .push(serde_json::json!({
                "stt": 2, "tchat": 1, "ten": "Khác tỉ lệ", "dvtinh": "Cái",
                "sluong": 1, "dgia": 500000, "thtien": 500000,
                "stckhau": 0, "ltsuat": "5%", "tsuat": 0.05
            }));
        let mut both = two_rates.clone();
        both["hdhhdvu"][0]["stckhau"] = serde_json::json!(200_000);

        for (name, data) in [
            ("cơ bản", &base),
            ("có Chiết khấu", &discounted),
            ("có Thuế suất", &two_rates),
            ("đủ hai cột tăng thêm", &both),
        ] {
            let html = build_invoice_html(&data.to_string()).expect("dựng HTML được");
            let columns = goods_thead(&html).matches("<th ").count();
            let widths = col_widths_of(&html);
            assert_eq!(
                widths.len(),
                columns,
                "{name}: đủ quy tắc cho {columns} cột"
            );
            assert_eq!(
                widths.iter().sum::<u32>(),
                FRAME_WIDTH_PT,
                "{name}: tổng bằng bề rộng khung"
            );
            // Sàn từng cột — bề rộng tối thiểu để tiêu đề không vỡ dòng.
            assert!(
                widths[0] >= 32 && widths[2] >= 36,
                "{name}: cột STT/ĐVT không dưới sàn {widths:?}"
            );
            assert!(
                widths[3] >= 60 && widths[4] >= 64 && widths[5] >= 64,
                "{name}: Số lượng/Đơn giá/Thành tiền không dưới sàn {widths:?}"
            );
            match columns {
                8 => assert!(
                    widths[6] >= 65 && widths[7] >= 64,
                    "{name}: Chiết khấu/Thuế suất không dưới sàn {widths:?}"
                ),
                7 => assert!(
                    widths[6] >= 64,
                    "{name}: cột tăng thêm không dưới sàn {widths:?}"
                ),
                _ => {}
            }
            assert!(
                widths.iter().skip(3).all(|w| *w <= 66),
                "{name}: cột số bám sát số liệu trong fixture {widths:?}"
            );
            let widest_other = widths
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != 1)
                .map(|(_, w)| *w)
                .max()
                .expect("có cột khác");
            assert!(
                widths[1] >= 160 && widths[1] > widest_other,
                "{name}: cột Tên hàng hóa rộng nhất {widths:?}"
            );
        }
    }

    /// Số liệu lớn phải nở cột theo: htmltopdf chỉ chịu xếp bộ `col.colN`
    /// khi mỗi cột rộng ít nhất bằng bề rộng nó tự đo, hẹp hơn là nó bỏ hết
    /// rồi tự bố trí lại — khi đấy các dòng tổng cộng có chữ dài sẽ kéo cột
    /// số nở to ra (và tên hàng ngắn thì các cột số bị chia phần dư).
    #[test]
    fn column_widths_grow_to_hold_big_numbers() {
        let mut data: serde_json::Value = serde_json::from_str(&fixture()).unwrap();
        data["hdhhdvu"][0]["dgia"] = serde_json::json!(1_234_567_890);
        data["hdhhdvu"][0]["thtien"] = serde_json::json!(1_234_567_890);
        let html = build_invoice_html(&data.to_string()).expect("dựng HTML được");
        let widths = col_widths_of(&html);
        // "1.234.567.890" htmltopdf đo ≈ 83.1pt + 6pt padding → ≥ 91pt.
        assert!(
            widths[4] >= 91 && widths[5] >= 91,
            "cột Đơn giá/Thành tiền nở theo số lớn: {widths:?}"
        );
        assert_eq!(
            widths.iter().sum::<u32>(),
            FRAME_WIDTH_PT,
            "vẫn đầy khung sau khi nở"
        );
        assert!(
            widths[1] >= MIN_NAME_WIDTH_PT,
            "cột Tên vẫn giữ được sàn: {widths:?}"
        );
    }

    /// Số liệu hiệu chuẩn đo trực tiếp trên htmltopdf: bisect bề rộng khai
    /// báo cho tới khi engine chịu xếp, suy ra nó ước `"2.860.000"` lớn hơn
    /// 56.5pt và `"1.234.567.890"` lớn hơn 82.5pt. Ước của mình không được
    /// thấp hơn mức đó, nếu không bộ quy tắc sẽ bị engine bỏ.
    #[test]
    fn estimate_is_never_below_what_htmltopdf_demands() {
        for (text, demanded) in [("2.860.000", 56.5), ("1.234.567.890", 82.5)] {
            let estimate = estimate_text_width_pt(text);
            assert!(
                estimate > demanded,
                "{text}: ước {estimate}pt không vượt {demanded}pt mà engine đòi"
            );
        }
    }

    /// htmltopdf tính bề rộng cột nhỏ nhất theo chữ không tách được — số tiền
    /// như `"1.234.567.890"` sẽ ép cột số nở to. Cho phép ngắt trong từng từ
    /// thì nó đo theo từng ký tự và không bao giờ bỏ bộ `col.colN`.
    #[test]
    fn table_cells_may_break_inside_a_long_token() {
        let html = build_invoice_html(&fixture()).expect("dựng HTML được");
        let table_css = css_block(&html, ".res-tb {");
        assert!(
            table_css.contains("overflow-wrap: break-word"),
            "bảng phải cho ngắt giữa từ: {table_css}"
        );
    }

    /// Toàn bộ đường kẻ bên trong bảng (ngang lẫn dọc) là nét đứt; riêng 4 lề
    /// ngoài — trên, dưới, trái, phải — vẫn nét liền. Khối CSS không được lặp:
    /// một khối `.res-tb` sao chép ở sau sẽ ghi đè mất khai báo nét đứt.
    #[test]
    fn inner_borders_are_dashed_and_the_four_outer_edges_are_solid() {
        let html = build_invoice_html(&fixture()).expect("dựng HTML được");
        for selector in [".res-tb {", ".res-tb tr td {", ".res-tb thead tr th {"] {
            assert_eq!(
                html.matches(selector).count(),
                1,
                "khối {selector} chỉ được viết 1 lần trong stylesheet"
            );
        }
        for selector in [".res-tb tr td {", ".res-tb thead tr th {"] {
            let css = css_block(&html, selector);
            assert!(
                css.contains("border: 0.5px dashed black"),
                "{selector} phải kẻ mọi biên bằng nét đứt: {css}"
            );
        }
        let top = css_block(&html, ".res-tb thead tr th {");
        assert!(
            top.contains("border-top: 0.5px solid black"),
            "viền trên bảng nét liền: {top}"
        );
        let bottom = css_block(&html, ".res-tb tbody tr:last-child td {");
        assert!(
            bottom.contains("border-bottom: 0.5px solid black"),
            "viền dưới bảng nét liền: {bottom}"
        );
        let first = css_block(&html, ".res-tb tr td:first-child {");
        assert!(
            first.contains("border-left: 0.5px solid black"),
            "viền trái bảng nét liền: {first}"
        );
        let last = css_block(&html, ".res-tb tr td:last-child {");
        assert!(
            last.contains("border-right: 0.5px solid black"),
            "viền phải bảng nét liền: {last}"
        );
    }

    /// Nền trang là một khối `position: fixed` phủ hết khung in: htmltopdf đặt
    /// khối fixed vào đúng khung in của tờ giấy (lề ngang 21pt của `@page` tự
    /// bị trừ ra) và vẽ lại nó trên mỗi trang. `height: 100%` bị engine bỏ qua
    /// nên phải khai 297mm — chiều cao A4 trừ lề trên/dưới (= 0).
    #[test]
    fn the_page_background_fills_the_print_area() {
        let html = build_invoice_html(&fixture()).expect("dựng HTML được");
        let bg = css_block(&html, ".bg-container {");
        assert!(bg.contains("position: fixed"), "khối nền là fixed: {bg}");
        assert!(bg.contains("height: 297mm"), "cao đúng khung in A4: {bg}");
        assert!(bg.contains("z-index: -1"), "nền nằm dưới chữ: {bg}");
        assert!(
            bg.contains("background-repeat: no-repeat"),
            "một tấm ảnh, không lặp: {bg}"
        );
        assert!(
            html.contains(r#"class="bg-container""#),
            "trang phải có khối .bg-container"
        );
        assert!(
            !html.contains(".main-page > div {"),
            "không còn gắn nền vào từng khối con (ảnh bị nhân bản)"
        );
    }

    /// Ảnh nền chỉ được nhúng tối đa 1 tấm mỗi trang. Gắn nền vào từng khối nội
    /// dung thì mỗi khối một tấm (hóa đơn mẫu trước đây in ra 5 tấm ảnh giống
    /// hệt nhau); gắn vào khối fixed thì mỗi trang đúng 1 tấm.
    #[test]
    fn the_page_background_is_painted_once_per_page() {
        let pdf = match render_pdf(&fixture()) {
            Ok(pdf) => pdf,
            Err(e) => {
                eprintln!("bỏ qua test render PDF: {e}");
                return;
            }
        };
        let bg = include_bytes!("assets/viewinvoice-bg.jpg");
        let seg = &bg[bg.len() / 3..bg.len() / 3 + 256];
        let embedded = pdf.windows(seg.len()).filter(|w| *w == seg).count();
        let pages = count_pages(&pdf);
        assert!(
            embedded >= 1 && embedded <= pages,
            "mỗi trang tối đa 1 tấm nền, đã nhúng {embedded} tấm cho {pages} trang"
        );
    }

    /// Lấy nội dung 1 khối CSS từ `selector {` tới ngoặc đóng của nó.
    fn css_block<'a>(html: &'a str, selector: &str) -> &'a str {
        let at = html.find(selector).expect("selector có trong CSS");
        let rest = &html[at + selector.len()..];
        &rest[..rest.find('}').expect("khối CSS có ngoặc đóng")]
    }

    /// Đọc các quy tắc `col.colN { width: NNpt }` đã dựng trong HTML.
    fn col_widths_of(html: &str) -> Vec<u32> {
        let mut widths = Vec::new();
        let mut rest = html;
        while let Some(at) = rest.find("col.col") {
            rest = &rest[at + "col.col".len()..];
            if !rest.starts_with(|c: char| c.is_ascii_digit()) {
                continue; // ghi chú CSS nhắc tới `col.colN` — không phải quy tắc
            }
            let end = rest.find('}').expect("quy tắc col.colN có ngoặc đóng");
            let width = rest[..end]
                .split_once("width:")
                .map(|(_, v)| v.trim().trim_end_matches("pt").trim())
                .and_then(|v| v.parse::<u32>().ok())
                .expect("width của cột là số pt");
            widths.push(width);
        }
        widths
    }

    /// 2 bảng con (thuế suất + thành tiền) đã gộp vào bảng hàng hóa.
    #[test]
    fn summary_rows_join_the_goods_table() {
        let html = build_invoice_html(&fixture()).expect("dựng HTML được");

        for gone in [
            r#"<div class="table-horizontal-wrapper">"#,
            "Tổng tiền phí",
            "Tổng tiền chiết khấu thương mại",
            "Tổng tiền chưa thuế",
            "Tổng tiền thuế",
        ] {
            assert!(!html.contains(gone), "`{gone}` đã bị gộp/bỏ");
        }
        // Các dòng tổng cộng giờ nằm trong bảng hàng hóa.
        let table = html
            .split("<table class=\"res-tb\">")
            .nth(1)
            .expect("bảng hàng hóa")
            .split("</table>")
            .next()
            .unwrap();
        for kept in [
            "Cộng tiền hàng",
            "Thuế suất GTGT",
            "Tiền thuế GTGT",
            "Tổng tiền thanh toán bằng số",
            "Số tiền viết bằng chữ",
        ] {
            assert!(table.contains(kept), "thiếu dòng `{kept}` trong bảng");
        }
        assert!(table.contains(">2.000.000<"), "Cộng tiền hàng = chưa thuế");
        assert!(table.contains(">2.160.000<"), "tổng thanh toán bằng số");
        assert!(table.contains("Hai triệu một trăm sáu mươi nghìn đồng"));
        // Nhãn căn trái, giá trị căn phải: các nhãn thẳng cùng 1 đường bên
        // trái, các giá trị kết thúc trên cùng 1 đường bên phải.
        for (label, value) in [
            ("Cộng tiền hàng", "2.000.000"),
            ("Tổng tiền thanh toán bằng số", "2.160.000"),
        ] {
            assert!(
                table.contains(&format!(r#"class="tx-left">{label}"#)),
                "nhãn {label} phải căn trái"
            );
            assert!(
                table.contains(&format!(r#"class="tx-right">{value}"#)),
                "giá trị {value} phải căn phải"
            );
        }
        assert!(
            table.contains(r#"class="tx-right">160.000"#),
            "tiền thuế căn phải cùng đường với các giá trị khác"
        );
        // Hàng thuế: 1 nhãn, các giá trị nằm trong ô của nó.
        assert!(table.contains(">8%<"), "tỉ lệ thuế trong ô value");
        assert!(table.contains(">160.000<"), "tiền thuế trong ô value");

        // Nhiều tỉ lệ → nhiều value chồng nhau, nhãn vẫn chỉ 1 lần.
        let mut data: serde_json::Value = serde_json::from_str(&fixture()).unwrap();
        data["hdhhdvu"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "stt": 2, "tchat": 1, "ten": "Khác tỉ lệ", "dvtinh": "Cái",
                "sluong": 1, "dgia": 500000, "thtien": 500000,
                "stckhau": 0, "ltsuat": "5%", "tsuat": 0.05
            }));
        data["thttltsuat"] = serde_json::json!([
            {"tsuat": "8%", "thtien": 2000000, "tthue": 160000},
            {"tsuat": "5%", "thtien": 500000, "tthue": 25000}
        ]);
        let two_rates = build_invoice_html(&data.to_string()).expect("dựng HTML được");
        let taxes = two_rates
            .split("Cộng tiền hàng")
            .nth(1)
            .expect("có dòng tổng hợp")
            .split("Tổng tiền thanh toán")
            .next()
            .unwrap();
        assert_eq!(
            two_rates.matches(">Thuế suất GTGT<").count(),
            1,
            "nhãn Thuế suất GTGT chỉ 1 lần"
        );
        assert_eq!(
            two_rates.matches(">Tiền thuế GTGT<").count(),
            1,
            "nhãn Tiền thuế GTGT chỉ 1 lần"
        );
        // htmltopdf bỏ `<br>` khi dựng ô bảng nên 2 giá trị sẽ dính vào nhau;
        // mỗi tỉ lệ thuế phải ra một dòng riêng với nhãn để trống bên trái.
        assert_eq!(
            taxes.matches(r#"class="tx-center">"#).count(),
            2,
            "mỗi tỉ lệ thuế 1 dòng: {taxes}"
        );
        assert!(
            !taxes.contains("<br>"),
            "không xếp chồng bằng <br>: {taxes}"
        );
        for cell in ["8%", "160.000", "5%", "25.000"] {
            assert!(
                taxes.contains(&format!(">{cell}<")),
                "thiếu ô {cell}: {taxes}"
            );
        }
    }

    /// Tiêu đề đi cùng hàng với QR (trái) và mẫu số (phải): 2 ô ngoài giữ
    /// bề rộng bằng nhau nên tâm ô giữa trùng tâm trang tuyệt đối, và ô giữa
    /// rộng đủ để tiêu đề/MCCQT không bị vỡ dòng.
    #[test]
    fn title_sits_between_qr_and_invoice_number_on_one_row() {
        let html = build_invoice_html(&fixture()).expect("dựng HTML được");
        for cell in ["top-qr", "top-title", "top-code"] {
            assert!(
                html.contains(&format!(r#"class="top-cell {cell}""#)),
                "thiếu ô {cell}"
            );
        }
        let qr = html.find(r#"class="top-cell top-qr""#).expect("có ô QR");
        let title = html
            .find(r#"class="top-cell top-title""#)
            .expect("có ô tiêu đề");
        let code = html
            .find(r#"class="top-cell top-code""#)
            .expect("có ô mẫu số");
        assert!(qr < title && title < code, "thứ tự QR | tiêu đề | mẫu số");

        let css = template();
        assert!(
            css.contains(".heading-content .top-title {\n            flex: 1;"),
            "ô tiêu đề phải flex: 1 để lấy hết phần còn lại"
        );
        assert!(
            css.contains("flex: 0 0 120pt;"),
            "2 ô ngoài phải giữ bề rộng bằng nhau (120pt mỗi bên)"
        );
        // htmltopdf không kế thừa `text-align` của ô cha xuống block con, và
        // `inline-block` làm ô code bị tính sai bề rộng khi ở trong flex.
        let code_css = css
            .split(".heading-content .code-content {")
            .nth(1)
            .expect("có rule .code-content")
            .split('}')
            .next()
            .unwrap();
        assert!(
            code_css.contains("display: block"),
            "ô code phải là block: {code_css}"
        );
        assert!(
            code_css.contains("text-align: right"),
            "số hóa đơn phải căn phải: {code_css}"
        );
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

    /// Thiếu `ltsuat` thì tỉ lệ vẫn đọc được từ `tsuat` (`0.08` → `"8%"`).
    #[test]
    fn rate_label_falls_back_to_the_numeric_tsuat() {
        let rate = |ltsuat: serde_json::Value, tsuat: serde_json::Value| {
            let row: GoodsRow = serde_json::from_value(serde_json::json!({
                "ltsuat": ltsuat, "tsuat": tsuat
            }))
            .expect("đọc được GoodsRow");
            rate_label(&row)
        };
        assert_eq!(rate(serde_json::json!("8%"), serde_json::Value::Null), "8%");
        assert_eq!(rate(serde_json::Value::Null, serde_json::json!(0.08)), "8%");
        assert_eq!(rate(serde_json::json!("5%"), serde_json::json!(0.05)), "5%");
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

    /// Ngày ký trong khối chữ ký định dạng `dd/mm/yyyy` như ví dụ
    /// `Ký ngày: 31/01/2020`, còn CN vẫn trích đúng.
    #[test]
    fn signature_box_only_when_signed_and_cn_is_extracted() {
        let signed = build_invoice_html(&fixture()).expect("dựng HTML được");
        assert!(signed.contains("Signature Valid"));
        assert!(
            signed.contains("Ký bởi: MOCK CONG TY"),
            "phải trích đúng CN: {}",
            cn_of(&signed)
        );
        assert!(
            signed.contains("Ký ngày: 15/09/2026"),
            "ngày ký phải dd/mm/yyyy: {}",
            cn_of(&signed)
        );
        // Mỗi mục 1 dòng: htmltopdf bỏ qua `display: block` trên <span> nên
        // khối chữ ký bắt buộc dùng <div>.
        assert!(
            signed.contains("</div><div>Ký ngày: "),
            "3 div block liên tiếp: {0}",
            cn_of(&signed)
        );
        assert!(
            !signed.contains(r#"<div class="sign-box"><span>"#),
            "không dùng <span> trong khối chữ ký: {0}",
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
        // Tiêu đề không còn bị đẩy về 2 đầu trang: mỗi tiêu đề chiếm 50% khung
        // và căn giữa trong phần của nó.
        assert!(
            template().contains(
                ".ft-sign .sign-row h3 {\n            flex: 1;\n            text-align: center;"
            ),
            "tiêu đề chữ ký phải căn giữa trong 50% mỗi bên"
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

    /// Giá trị sau nhãn "Ký ngày: " — CN thì lấy sau "Ký bởi: ".
    fn cn_of(html: &str) -> String {
        let start = html
            .find("Ký bởi: ")
            .map(|i| i + "Ký bởi: ".len())
            .unwrap_or(0);
        let seg = &html[start..html.len().min(start + 200)];
        let b = seg.find('<').unwrap_or(0);
        seg[..b].to_string()
    }

    /// Số trang trong file PDF htmltopdf vừa sinh (đếm thẻ `<Page>`). Bảng tổng
    /// cộng đã gộp vào bảng hàng hóa nên hóa đơn gọn đi — hóa đơn mẫu 1 trang,
    /// hóa đơn nhiều dòng vẫn tối đa 2 trang (khối chữ ký không bị vỡ).
    #[test]
    fn renders_at_most_two_a4_pages() {
        let pdf = match render_pdf(&fixture()) {
            Ok(pdf) => pdf,
            Err(e) => {
                eprintln!("bỏ qua test render PDF: {e}");
                return;
            }
        };
        assert!(pdf.starts_with(b"%PDF"), "phải là file PDF hợp lệ");
        let pages = count_pages(&pdf);
        assert!(
            (1..=2).contains(&pages),
            "hóa đơn mẫu phải chiếm 1–2 trang A4 (đã có {pages})"
        );
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

    /// `detail_json` của fixture với túi `ttkhac` do test chỉ định.
    fn with_tt_khac(items: serde_json::Value) -> String {
        let mut data: serde_json::Value = serde_json::from_str(&fixture()).expect("fixture hợp lệ");
        data["ttkhac"] = items;
        data.to_string()
    }

    /// `detail_json` của fixture với vài trường do test ghi đè.
    fn with(overrides: serde_json::Value) -> String {
        let mut data: serde_json::Value = serde_json::from_str(&fixture()).expect("fixture hợp lệ");
        for (key, value) in overrides.as_object().expect("phải là object") {
            data[key] = value.clone();
        }
        data.to_string()
    }

    #[test]
    fn sector_fields_render_before_the_goods_table() {
        // Bài tham khảo mục 3.3 (vận tải) + 3.4 (khách sạn), ghép cùng một
        // hóa đơn để kiểm tra cả hai nhóm trong một lần dựng.
        let html = build_invoice_html(&with_tt_khac(serde_json::json!([
            {"ttruong": "Tên tàu", "kdlieu": "string", "dlieu": "Thuyền Đào Kính"},
            {"ttruong": "Vận đơn", "kdlieu": "string", "dlieu": "WB-2026-01"},
            {"ttruong": "Quốc tịch", "kdlieu": "string", "dlieu": "Việt Nam"},
            {"ttruong": "NGÀY ĐẾN", "kdlieu": "string", "dlieu": "2026-10-05"},
            {"ttruong": "Ngày đi", "kdlieu": "string", "dlieu": "2026-10-07"},
            {"ttruong": "số phòng", "kdlieu": "string", "dlieu": "101"},
            {"ttruong": "RowType", "kdlieu": "numberic", "dlieu": "1"}
        ])))
        .expect("dựng HTML được");

        for text in [
            "Tên tàu",
            "Thuyền Đào Kính",
            "Vận đơn",
            "WB-2026-01",
            "Quốc tịch",
            "Việt Nam",
            "Ngày đến",
            "2026-10-05",
            "Ngày đi",
            "Số phòng",
            "<div>101</div>",
        ] {
            assert!(html.contains(text), "thiếu thông tin {text}");
        }
        let extra = html.find("Thuyền Đào Kính").expect("có dòng vận tải");
        let table = html
            .find(r#"<table class="res-tb""#)
            .expect("có bảng hàng hóa");
        assert!(extra < table, "thông tin đặc thù nằm trước bảng hàng hóa");
        assert!(!html.contains("{{"), "không sót placeholder");
    }

    #[test]
    fn internal_tt_khac_keys_are_never_printed() {
        // `ttkhac` thực tế của cổng HĐĐT toàn khóa nội bộ — không được in ra.
        let html = build_invoice_html(&with_tt_khac(serde_json::json!([
            {"ttruong": "StockTotalAmount", "kdlieu": "numeric", "dlieu": "6949800.0"},
            {"ttruong": "RowType", "kdlieu": "numberic", "dlieu": "1"},
            {"ttruong": "SellerAddress", "kdlieu": "string", "dlieu": "Số 1 ngõ 184 Văn Minh"},
            {"ttruong": "AccountObjectID", "kdlieu": "string", "dlieu": "7d20d6d5-8a91"},
            {"ttruong": "Mã TC", "kdlieu": "string", "dlieu": "RW874AUDAD9"},
            {"ttruong": "ĐCTC", "kdlieu": "string", "dlieu": "https://spv.tracuuhoadon.online/"},
            {"ttruong": "Tiền thuế", "kdlieu": "numeric", "dlieu": "11896"}
        ])))
        .expect("dựng HTML được");

        for leak in [
            "StockTotalAmount",
            ">6949800.0<",
            "SellerAddress",
            "Văn Minh",
            "AccountObjectID",
            "RW874AUDAD9",
            "spv.tracuuhoadon.online",
            ">11896<",
        ] {
            assert!(!html.contains(leak), "khóa nội bộ {leak} không được in ra");
        }
    }

    #[test]
    fn title_falls_back_to_the_type_name_when_the_data_has_none() {
        let html = build_invoice_html(&with(serde_json::json!({
            "tlhdon": null, "thdon": null, "khhdon": "C26MOC"
        })))
        .expect("dựng HTML được");
        assert!(
            html.contains(r#"<div class="main-title">HÓA ĐƠN GIÁ TRỊ GIA TĂNG</div>"#),
            "tiêu đề lấy theo ký hiệu mẫu 1"
        );

        // Không tra được loại hóa đơn thì vẫn còn một tiêu đề chung.
        let html = build_invoice_html(&with(serde_json::json!({
            "tlhdon": null, "thdon": null, "khmshdon": null
        })))
        .expect("dựng HTML được");
        assert!(
            html.contains(r#"<div class="main-title">HÓA ĐƠN</div>"#),
            "tiêu đề chung khi không nhận diện được loại"
        );
    }

    #[test]
    fn title_prefers_the_invoice_name_over_the_sample_number() {
        let html = build_invoice_html(&with(serde_json::json!({
            "tlhdon": null,
            "thdon": "Hóa đơn bán hàng khởi tạo từ máy tính tiền",
            "khmshdon": 2
        })))
        .expect("dựng HTML được");
        assert!(
            html.contains(
                r#"<div class="main-title">HÓA ĐƠN BÁN HÀNG KHỞI TẠO TỪ MÁY TÍNH TIỀN</div>"#
            ),
            "tên hóa đơn do người bán đặt thắng ký hiệu mẫu"
        );
    }

    #[test]
    fn foreign_currency_shows_the_unit_and_the_rate() {
        let html = build_invoice_html(&with(serde_json::json!({
            "dvtte": "USD", "tgia": 25_400
        })))
        .expect("dựng HTML được");
        assert!(html.contains("Đơn vị tiền tệ"), "thiếu tên đồng tiền");
        assert!(html.contains("<div>USD</div>"), "thiếu mã tiền tệ");
        assert!(html.contains("Tỷ giá"), "thiếu nhãn tỷ giá");
        assert!(html.contains("<div>25.400</div>"), "thiếu số tỷ giá");
    }

    #[test]
    fn vnd_keeps_the_currency_rows_out() {
        // Fixture không gửi `dvtte` → không có dòng đồng tiền.
        let html = build_invoice_html(&fixture()).expect("dựng HTML được");
        assert!(!html.contains("Đơn vị tiền tệ"), "không có dòng tiền tệ");
        assert!(!html.contains("Tỷ giá"), "không có dòng tỷ giá");

        // VND với tỷ giá 1 cũng vậy.
        let html = build_invoice_html(&with(serde_json::json!({
            "dvtte": "VND", "tgia": 1.0
        })))
        .expect("dựng HTML được");
        assert!(!html.contains("Đơn vị tiền tệ"), "VND không cần đổi tiền");
        assert!(!html.contains("Tỷ giá"), "VND thì tỷ giá 1 không hiển thị");
    }

    #[test]
    fn fee_row_only_when_there_is_a_fee() {
        // Fixture có `tgtphi: 0` → không có dòng phí.
        let html = build_invoice_html(&fixture()).expect("dựng HTML được");
        assert!(!html.contains("Phí, lệ phí"), "phí bằng 0 thì không in");

        let html = build_invoice_html(&with(serde_json::json!({ "tgtphi": 150_000 })))
            .expect("dựng HTML được");
        assert!(html.contains("Phí, lệ phí"), "thiếu dòng phí");
        assert!(html.contains("<div>150.000</div>"), "thiếu số tiền phí");
    }

    #[test]
    fn note_renders_its_first_non_empty_value_and_hides_secrets() {
        // Dữ liệu thật: "Ghi chú" rỗng đứng trước "Ghi chú hóa đơn" có nội
        // dung, và "Mã số bí mật" là khóa nội bộ không được in ra.
        let html = build_invoice_html(&with_tt_khac(serde_json::json!([
            {"ttruong": "Ghi chú", "kdlieu": "string", "dlieu": ""},
            {"ttruong": "Ghi chú hóa đơn", "kdlieu": "string", "dlieu": "Khách hàng trả sau"},
            {"ttruong": "Mã số bí mật", "kdlieu": "string", "dlieu": "NA18WIOXVE638UG"}
        ])))
        .expect("dựng HTML được");
        assert!(html.contains(">Ghi chú:<"), "thiếu dòng ghi chú");
        assert!(
            html.contains("Khách hàng trả sau"),
            "phải lấy mục ghi chú có nội dung"
        );
        assert!(
            !html.contains("NA18WIOXVE638UG"),
            "khóa bí mật không được in ra"
        );

        // Không có ghi chú thì không có dòng.
        let html = build_invoice_html(&fixture()).expect("dựng HTML được");
        assert!(!html.contains(">Ghi chú:<"), "fixture không có ghi chú");
    }
}
