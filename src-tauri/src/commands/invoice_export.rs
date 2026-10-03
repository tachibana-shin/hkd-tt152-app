//! Quy trình xuất hóa đơn ra bên dịch vụ HĐĐT khác.
//!
//! Hộ không có API phát hành (dùng bên thứ ba) nên phải chép tay. Module này
//! gom phần "chuẩn bị chép": `preflight` kiểm tra trước khi chép (MST người
//! mua, tổng tiền khớp tổng dòng, nhóm ngành — bên kia chắc chắn từ chối hóa
//! đơn thiếu những thứ này), `build_pack` dựng sẵn đúng nội dung cần nhập
//! (khối dán cho ô text tự do, text đầy đủ và JSON). Tên hàng giữ nguyên vẹn
//! (chỉ gộp khoảng trắng thừa) vì dịch vụ bên kia không giới hạn số ký tự.
//!
//! Hóa đơn bán của hộ KHÔNG tách thuế: `total` là số tiền khách trả, thuế tính
//! cuối kỳ theo tỷ lệ % × doanh thu. Vì vậy khối dán không có cột thuế và
//! không có "tổng thanh toán" — chỉ có tổng tiền.
//!
//! Bấm "Chép để xuất" gọi `mark_exported`: lưu bản chốt vào `invoice_export` và
//! chuyển `draft` → `exported`. Từ đó nếu sửa hóa đơn thì `build_pack` báo
//! `edited_after_export` để người dùng biết phải sửa lại bên kia.
//!
//! `set_status` ghi nhận **hóa đơn điều chỉnh** phát hành bên kia
//! (`adjusted`); mỗi bước ghi vào `invoice_event` để đối chiếu lại.
//!
//! **Không có trạng thái "đã hủy"**: Thông tư 91/2026/TT-BTC Điều 10 quy định từ
//! 01/07/2026 người bán không được tự ý hủy hóa đơn điện tử đã lập. Sai sót thì
//! lập hóa đơn **thay thế** (`replace_invoice_core` trong `commands::invoice`) hoặc
//! **điều chỉnh**; hóa đơn nháp (chưa phát hành) thì cứ sửa hoặc xoá.
#![allow(unused_imports)]

use crate::helpers::*;
use crate::models::*;
use serde_json::json;
use sqlx::{Sqlite, SqlitePool};
use tauri::State;

/// Chuẩn hoá chuỗi để so khớp/tìm kiếm: bỏ dấu tiếng Việt, hạ chữ thường,
/// gộp mọi khoảng trắng thành 1. Dùng chung cho gợi ý ánh xạ mặt hàng.
#[allow(dead_code)] // dùng ở màn Khớp mặt hàng (P0-2)
pub(crate) fn fold_key(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for c in s.trim().chars() {
        let lower = c.to_lowercase().next().unwrap_or(c);
        // Bỏ dấu: `char::to_lowercase` + `NFD` không có ở std, dùng bảng map tối
        // giản cho các dấu hay gặp (đủ dùng cho so khớp tên hàng).
        let ch = match lower {
            'à' | 'á' | 'ạ' | 'ả' | 'ã' | 'â' | 'ầ' | 'ấ' | 'ậ' | 'ẩ' | 'ẫ' | 'ă' | 'ằ' | 'ắ'
            | 'ặ' | 'ẳ' | 'ẵ' => 'a',
            'è' | 'é' | 'ẹ' | 'ẻ' | 'ẽ' | 'ê' | 'ề' | 'ế' | 'ệ' | 'ể' | 'ễ' => {
                'e'
            }
            'ì' | 'í' | 'ị' | 'ỉ' | 'ĩ' => 'i',
            'ò' | 'ó' | 'ọ' | 'ỏ' | 'õ' | 'ô' | 'ồ' | 'ố' | 'ộ' | 'ổ' | 'ỗ' | 'ơ' | 'ờ' | 'ớ'
            | 'ợ' | 'ở' | 'ỡ' => 'o',
            'ù' | 'ú' | 'ụ' | 'ủ' | 'ũ' | 'ư' | 'ừ' | 'ứ' | 'ự' | 'ử' | 'ữ' => {
                'u'
            }
            'ỳ' | 'ý' | 'ỵ' | 'ỷ' | 'ỹ' => 'y',
            'đ' => 'd',
            other => other,
        };
        if ch.is_whitespace() || ch == '-' || ch == '/' || ch == '_' {
            if !prev_space && !out.is_empty() {
                out.push(' ');
                prev_space = true;
            }
            continue;
        }
        prev_space = false;
        out.push(ch);
    }
    out
}

fn fmt_money(v: f64) -> String {
    let n = v.round() as i64;
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(c);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}

pub(crate) fn fmt_qty(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 {
        format!("{}", v.round() as i64)
    } else {
        let s = format!("{:.3}", v);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn fmt_pct(v: f64) -> String {
    let pct = v * 100.0;
    let s = if (pct - pct.round()).abs() < 1e-9 {
        format!("{}", pct.round() as i64)
    } else {
        format!("{:.2}", pct)
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    };
    format!("{s}%")
}

/// yyyy-mm-dd → dd/MM/yyyy (dịch vụ khác hay nhập dạng Việt Nam).
fn fmt_date_vn(s: &str) -> String {
    let p: Vec<&str> = s.split('-').collect();
    if p.len() == 3 {
        format!("{}/{}/{}", p[2], p[1], p[0])
    } else {
        s.to_string()
    }
}

/// Tên dòng để dán sang bên kia: giữ nguyên vẹn, chỉ gộp khoảng trắng/dấu xuống
/// dòng thừa (dịch vụ bên kia không giới hạn số ký tự tên hàng).
pub(crate) fn clean_name(name: &str) -> String {
    name.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Số để máy đọc: KHÔNG dấu chấm phân cách nghìn (bên kia tự định dạng khi hiển
/// thị, "15.000" sẽ bị đọc thành 15), tối đa 2 chữ số thập phân, bỏ số 0 thừa.
fn fmt_plain(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    let s = format!("{r}");
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

/// Ô text tự do bên kia tự tách cột theo dấu phân cách, nên tên hàng/đơn vị có
/// chứa đúng ký tự đó sẽ vỡ thành 2 cột và dán ra số liệu sai. Thay bằng khoảng
/// trắng để dòng dán không bị lệch cột.
fn clean_field(text: &str, sep: char) -> String {
    clean_name(&text.replace(['\n', '\r', sep], " "))
}

/// Bố cục cột của 1 dòng dán. Bên kia không có danh mục hàng — mỗi lần phát hành
/// phải nhập lại từng dòng — nên khối dán phải đúng số cột và đúng thứ tự form
/// máy tính tiền nhận. Người dùng chọn 1 bố cục, app nhớ lại cho các hóa đơn sau.
fn paste_columns(layout: &str) -> Result<Vec<&'static str>, String> {
    Ok(match layout {
        "full" => vec![
            "Tên hàng",
            "ĐVT",
            "SL",
            "Đơn giá",
            "Chiết khấu",
            "Thành tiền",
        ],
        "no_discount" => vec!["Tên hàng", "ĐVT", "SL", "Đơn giá", "Thành tiền"],
        "no_unit" => vec!["Tên hàng", "SL", "Đơn giá", "Thành tiền"],
        "amount_only" => vec!["Tên hàng", "SL", "Thành tiền"],
        other => return Err(format!("Bố cục dán không hợp lệ: {other}")),
    })
}

/// Dấu tách cột mà ô text bên kia nhận. Tab an toàn nhất (hiếm khi xuất hiện trong
/// tên hàng) nhưng nhiều phần mềm POS tách theo dấu phẩy — để người dùng chọn.
fn paste_separator(sep: &str) -> Result<char, String> {
    match sep {
        "\t" => Ok('\t'),
        "," => Ok(','),
        ";" => Ok(';'),
        "|" => Ok('|'),
        other => Err(format!("Dấu phân cách không hợp lệ: {other:?}")),
    }
}

/// 1 dòng hàng trong gói xuất.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct ExportLine {
    pub(crate) stt: i64,
    pub(crate) product_code: String,
    /// Tên hàng để dán sang bên kia (giữ nguyên, chỉ gộp khoảng trắng thừa).
    pub(crate) product_name: String,
    pub(crate) unit: String,
    pub(crate) quantity: f64,
    pub(crate) unit_price: f64,
    pub(crate) discount: f64,
    pub(crate) amount: f64,
    pub(crate) industry_code: String,
    pub(crate) industry_name: String,
    pub(crate) vat_rate: f64,
    /// Tên nhóm ngành để dán cho bên kia không cần tra danh mục.
    pub(crate) vat_label: String,
    pub(crate) warehouse_code: String,
    /// Cảnh báo riêng của dòng (vd thuế 0% mà chưa có nhóm ngành).
    pub(crate) warnings: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct ExportCheck {
    /// "error" | "warn" | "ok"
    pub(crate) level: String,
    pub(crate) message: String,
}

/// Header hóa đơn đầy đủ để dựng nội dung chép.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct ExportHeader {
    pub(crate) id: i64,
    pub(crate) number: String,
    pub(crate) date: String,
    pub(crate) date_vn: String,
    pub(crate) customer: String,
    pub(crate) customer_tax_code: String,
    pub(crate) status: String,
    pub(crate) total: f64,
    pub(crate) vat_amount: f64,
    pub(crate) e_invoice_no: String,
    pub(crate) e_invoice_symbol: String,
    pub(crate) voucher_no: String,
    pub(crate) biz_name: String,
    pub(crate) biz_tax_code: String,
    pub(crate) biz_address: String,
    pub(crate) biz_phone: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct ExportPack {
    pub(crate) header: ExportHeader,
    pub(crate) lines: Vec<ExportLine>,
    pub(crate) checks: Vec<ExportCheck>,
    /// Tổng tiền tính lại từ dòng (đối chiếu với `header.total`).
    pub(crate) computed_total: f64,
    /// Khối dán vào ô text tự do bên kia: mỗi dòng 1 dòng hàng, cột tách theo
    /// `paste_sep`, không chứa thuế suất (bên kia không nhập, tự áp tỷ lệ %).
    pub(crate) paste: String,
    /// Bố cục + dấu phân cách đã dùng, để UI hiển thị và lưu lại lựa chọn.
    pub(crate) paste_layout: String,
    pub(crate) paste_sep: String,
    /// Nhãn các cột theo đúng thứ tự trong `paste` (để người dùng đối chiếu).
    pub(crate) paste_columns: Vec<String>,
    pub(crate) text: String,
    pub(crate) json: serde_json::Value,
    /// Số bản chốt đã lưu (0 = lần đầu).
    pub(crate) export_count: i64,
    /// Hóa đơn đã bị sửa sau lần chép gần nhất chưa.
    pub(crate) edited_after_export: bool,
}

fn has_errors(checks: &[ExportCheck]) -> bool {
    checks.iter().any(|c| c.level == "error")
}

/// Dòng hàng thô từ `invoice_item` + `product` (dùng chung để dựng ExportLine).
#[derive(sqlx::FromRow)]
struct RawItem {
    /// Hóa đơn chủ — dựng gói cho 1 hóa đơn thì bằng id đang xét, gom cho cả
    /// hàng chờ xuất thì lọc theo cột này.
    invoice_id: i64,
    code: String,
    name: String,
    unit: String,
    quantity: f64,
    unit_price: f64,
    discount: f64,
    subtotal: f64,
    industry_code: String,
    vat_rate: f64,
    warehouse_code: String,
}

fn to_export_line(idx: usize, it: &RawItem, groups: &[(String, String)]) -> ExportLine {
    let mut warnings = Vec::new();
    if it.vat_rate == 0.0 && it.industry_code.trim().is_empty() {
        warnings.push("Thuế 0% nhưng chưa có nhóm ngành".into());
    }
    let industry_name = groups
        .iter()
        .find(|(c, _)| *c == it.industry_code)
        .map(|(_, n)| n.clone())
        .unwrap_or_default();
    ExportLine {
        stt: idx as i64 + 1,
        product_code: it.code.clone(),
        product_name: clean_name(&it.name),
        unit: it.unit.clone(),
        quantity: it.quantity,
        unit_price: it.unit_price,
        discount: it.discount,
        amount: it.subtotal - it.discount,
        industry_code: it.industry_code.clone(),
        industry_name: industry_name.clone(),
        vat_rate: it.vat_rate,
        // fmt_pct đã kèm dấu % — chỉ ghép tên nhóm ngành.
        vat_label: format!("{} {}", fmt_pct(it.vat_rate), industry_name),
        warehouse_code: it.warehouse_code.clone(),
        warnings,
    }
}

async fn load_industry_groups(pool: &SqlitePool) -> Result<Vec<(String, String)>, String> {
    Ok(sqlx::query!("SELECT code, name FROM industry_group")
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|g| (g.code, g.name))
        .collect::<Vec<_>>())
}

/// Nạp header + dòng hàng của 1 hóa đơn.
async fn load_header_and_lines(
    pool: &SqlitePool,
    invoice_id: i64,
) -> Result<(ExportHeader, Vec<ExportLine>), String> {
    let row = sqlx::query!(
        r#"SELECT i.id, i.number, i.date, i.customer, i.customer_tax_code, i.status,
                  i.total, i.vat_amount, i.e_invoice_no, i.e_invoice_symbol,
                  i.voucher_no,
                  COALESCE(b.name, '') AS biz_name, COALESCE(b.tax_code, '') AS biz_tax_code,
                  COALESCE(b.address, '') AS biz_address, COALESCE(b.phone, '') AS biz_phone
             FROM invoice i
             LEFT JOIN business b ON b.id = 1
            WHERE i.id = ?"#,
        invoice_id
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "Không tìm thấy hóa đơn".to_string())?;

    let groups = load_industry_groups(pool).await?;

    let items: Vec<RawItem> = sqlx::query_as!(
        RawItem,
        r#"SELECT ii.invoice_id, p.code,
                  -- Tên in = tên người dùng chọn trên dòng (alias) nếu có (F5).
                  COALESCE(NULLIF(ii.line_name, ''), p.name) AS "name!: String",
                  p.unit, ii.quantity, ii.unit_price,
                  ii.discount, ii.subtotal, ii.industry_code, ii.vat_rate, ii.warehouse_code
             FROM invoice_item ii
             JOIN product p ON p.id = ii.product_id
            WHERE ii.invoice_id = ?
            ORDER BY ii.id"#,
        invoice_id
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let lines: Vec<ExportLine> = items
        .iter()
        .enumerate()
        .map(|(idx, it)| to_export_line(idx, it, &groups))
        .collect();

    let date_vn = fmt_date_vn(&row.date);
    let header = ExportHeader {
        id: row.id,
        number: row.number,
        date: row.date,
        date_vn,
        customer: row.customer,
        customer_tax_code: row.customer_tax_code,
        status: row.status,
        total: row.total,
        vat_amount: row.vat_amount,
        e_invoice_no: row.e_invoice_no,
        e_invoice_symbol: row.e_invoice_symbol,
        voucher_no: row.voucher_no,
        biz_name: row.biz_name,
        biz_tax_code: row.biz_tax_code,
        biz_address: row.biz_address,
        biz_phone: row.biz_phone,
    };
    Ok((header, lines))
}

/// Kiểm tra trước khi chép: lỗi chặn, cảnh báo nhắc.
fn build_checks(header: &ExportHeader, lines: &[ExportLine]) -> Vec<ExportCheck> {
    let mut checks = Vec::new();

    // MST người mua
    let mst = header.customer_tax_code.trim();
    if mst.is_empty() {
        checks.push(ExportCheck {
            level: "warn".into(),
            message: "Thiếu MST người mua — bên kia sẽ hỏi lại, nên ghi sẵn MST hộ cá nhân (10 số bắt đầu bằng 0)".into(),
        });
    } else if mst.len() < 10 || !mst.chars().all(|c| c.is_ascii_digit()) {
        checks.push(ExportCheck {
            level: "error".into(),
            message: format!("MST người mua '{mst}' không hợp lệ (phải là 10–13 chữ số)"),
        });
    }

    if lines.is_empty() {
        checks.push(ExportCheck {
            level: "error".into(),
            message: "Hóa đơn chưa có dòng hàng".into(),
        });
    }

    // Tổng tiền phải khớp tổng dòng — lệch là do sửa tay, sẽ ra HĐ sai.
    let sum: f64 = lines.iter().map(|l| l.amount).sum();
    if (sum - header.total).abs() > 1.0 {
        checks.push(ExportCheck {
            level: "error".into(),
            message: format!(
                "Tổng tiền hóa đơn {} ≠ tổng dòng {} — kiểm tra lại trước khi chép",
                fmt_money(header.total),
                fmt_money(sum)
            ),
        });
    }

    // Thuế: hộ kê theo doanh thu, mỗi dòng có thể thuế suất khác nhau.
    let no_industry: Vec<&str> = lines
        .iter()
        .filter(|l| l.industry_code.trim().is_empty())
        .map(|l| l.product_name.as_str())
        .collect();
    if !no_industry.is_empty() {
        checks.push(ExportCheck {
            level: "error".into(),
            message: format!(
                "Dòng thiếu nhóm ngành: {}",
                no_industry
                    .iter()
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        });
    }

    if matches!(header.status.as_str(), "replaced" | "cancelled") {
        checks.push(ExportCheck {
            level: "error".into(),
            message: "Hóa đơn đã bị thay thế — xem hóa đơn thay thế để chép".into(),
        });
    }
    if header.status == "official" {
        checks.push(ExportCheck {
            level: "warn".into(),
            message: format!(
                "Hóa đơn đã phát hành{} — nếu bên kia hủy/sửa, cập nhật trạng thái ở đây",
                if header.e_invoice_no.is_empty() {
                    String::new()
                } else {
                    format!(" ({}/{})", header.e_invoice_symbol, header.e_invoice_no)
                }
            ),
        });
    }
    checks
}

fn render_text(header: &ExportHeader, lines: &[ExportLine]) -> String {
    let mut s = String::new();
    s.push_str("BỞI XUẤT HÓA ĐƠN (dữ liệu để nhập sang dịch vụ khác)\n");
    s.push_str(&format!("Hóa đơn nội bộ: {}\n", header.number));
    s.push_str(&format!("Ngày: {}\n", header.date_vn));
    s.push_str(&format!(
        "Người mua: {}{}\n",
        header.customer,
        if header.customer_tax_code.trim().is_empty() {
            String::new()
        } else {
            format!(" — MST {}", header.customer_tax_code.trim())
        }
    ));
    s.push_str("Phương thức thanh toán: Chuyển khoản\n\n");
    s.push_str("DÒNG HÀNG (cột cách nhau bằng TAB):\n");
    s.push_str("STT\tTên hàng\tĐVT\tSL\tĐơn giá\tChiết khấu\tThành tiền\n");
    for l in lines {
        s.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            l.stt,
            l.product_name,
            l.unit,
            fmt_qty(l.quantity),
            fmt_money(l.unit_price),
            fmt_money(l.discount),
            fmt_money(l.amount)
        ));
    }
    // Không tách thuế trên hóa đơn (hộ kê theo doanh thu) — tổng là số tiền khách
    // trả, không cộng thêm thuế tính cuối kỳ.
    s.push_str(&format!("\nTổng tiền: {}\n", fmt_money(header.total)));
    s
}

/// Dựng khối dán: mỗi dòng 1 dòng hàng, cột theo bố cục đã chọn, số ở dạng máy
/// đọc được. Cố ý KHÔNG kèm thuế suất — bên kia tự tính theo tỷ lệ % trong hồ sơ.
fn render_paste(lines: &[ExportLine], layout: &str, sep: char) -> String {
    let mut s = String::new();
    for l in lines {
        let name = clean_field(&l.product_name, sep);
        let unit = clean_field(&l.unit, sep);
        let cells: Vec<String> = match layout {
            "full" => vec![
                name,
                unit,
                fmt_plain(l.quantity),
                fmt_plain(l.unit_price),
                fmt_plain(l.discount),
                fmt_plain(l.amount),
            ],
            "no_discount" => vec![
                name,
                unit,
                fmt_plain(l.quantity),
                fmt_plain(l.unit_price),
                fmt_plain(l.amount),
            ],
            "no_unit" => vec![
                name,
                fmt_plain(l.quantity),
                fmt_plain(l.unit_price),
                fmt_plain(l.amount),
            ],
            _ => vec![name, fmt_plain(l.quantity), fmt_plain(l.amount)],
        };
        s.push_str(&cells.join(&sep.to_string()));
        s.push('\n');
    }
    s
}

/// Dựng gói chép hoàn chỉnh (dùng cho cả dialog lẫn bản chốt).
pub(crate) async fn build_pack(
    pool: &SqlitePool,
    invoice_id: i64,
    layout: &str,
    sep: &str,
) -> Result<ExportPack, String> {
    let columns = paste_columns(layout)?;
    let sep_char = paste_separator(sep)?;
    let (header, lines) = load_header_and_lines(pool, invoice_id).await?;
    let checks = build_checks(&header, &lines);
    let computed_total: f64 = lines.iter().map(|l| l.amount).sum();
    let text = render_text(&header, &lines);
    let paste = render_paste(&lines, layout, sep_char);
    let json = json!({
        "number": header.number,
        "date": header.date_vn,
        "customer": { "name": header.customer, "tax_code": header.customer_tax_code },
        "payment_method": "Chuyển khoản",
        "lines": lines,
        "paste": { "layout": layout, "separator": sep, "columns": columns, "block": paste },
        "total": header.total,
    });

    let export_count: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM invoice_export WHERE invoice_id = ?",
        invoice_id
    )
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;

    // Đã chép trước mà giờ hóa đơn lệch (tiền/dòng) → cảnh báo để sửa bên kia.
    let edited_after_export = if export_count == 0 {
        false
    } else {
        match latest_snapshot_total(pool, invoice_id).await? {
            Some(before) => {
                (before - header.total).abs() > 1.0
                    || !lines_equal_snapshot(pool, invoice_id).await?
            }
            None => false,
        }
    };

    Ok(ExportPack {
        header,
        lines,
        checks,
        paste,
        paste_layout: layout.to_string(),
        paste_sep: sep.to_string(),
        paste_columns: columns.iter().map(|c| c.to_string()).collect(),
        computed_total,
        text,
        json,
        export_count,
        edited_after_export,
    })
}

async fn latest_snapshot_total(pool: &SqlitePool, invoice_id: i64) -> Result<Option<f64>, String> {
    let row = sqlx::query!(
        r#"SELECT total FROM invoice_export WHERE invoice_id = ? ORDER BY id DESC LIMIT 1"#,
        invoice_id
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(row.map(|r| r.total))
}

/// So 2 danh sách (mã hàng, SL, đơn giá) của bản chốt và hiện tại.
fn pairs_match(saved: &[(String, f64, f64)], now: &[(String, f64, f64)]) -> bool {
    saved.len() == now.len()
        && saved
            .iter()
            .zip(now.iter())
            .all(|(a, b)| a.0 == b.0 && (a.1 - b.1).abs() < 1e-6 && (a.2 - b.2).abs() < 1e-6)
}

/// Bản chốt (đã chép sang bên kia) còn khớp với hóa đơn hiện tại không.
fn snapshot_differs(
    saved_total: f64,
    now_total: f64,
    saved_pairs: &[(String, f64, f64)],
    now_pairs: &[(String, f64, f64)],
) -> bool {
    (saved_total - now_total).abs() > 1.0 || !pairs_match(saved_pairs, now_pairs)
}

/// So từng dòng bản chốt với hiện tại (số lượng / đơn giá / chiết khấu).
async fn lines_equal_snapshot(pool: &SqlitePool, invoice_id: i64) -> Result<bool, String> {
    let Some(snapshot) = sqlx::query!(
        r#"SELECT payload_json FROM invoice_export WHERE invoice_id = ? ORDER BY id DESC LIMIT 1"#,
        invoice_id
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    else {
        return Ok(true);
    };
    let saved: serde_json::Value = serde_json::from_str(&snapshot.payload_json).unwrap_or_default();
    // So từng dòng theo mã hàng + số lượng + đơn giá.
    let saved_pairs: Vec<(String, f64, f64)> = saved_lines_vec(&saved);
    let now_pairs: Vec<(String, f64, f64)> = sqlx::query!(
        r#"SELECT p.code, ii.quantity, ii.unit_price
             FROM invoice_item ii JOIN product p ON p.id = ii.product_id
            WHERE ii.invoice_id = ? ORDER BY ii.id"#,
        invoice_id
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?
    .into_iter()
    .map(|r| (r.code, r.quantity, r.unit_price))
    .collect();
    Ok(pairs_match(&saved_pairs, &now_pairs))
}

fn saved_lines_vec(v: &serde_json::Value) -> Vec<(String, f64, f64)> {
    v.get("lines")
        .and_then(|l| l.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|it| {
                    Some((
                        it.get("product_code")?.as_str()?.to_string(),
                        it.get("quantity")?.as_f64()?,
                        it.get("unit_price")?.as_f64()?,
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Bấm "Chép để xuất": lưu bản chốt + chuyển `draft` → `exported`.
/// Trả về gói để UI hiển thị luôn (và để test không cần gọi 2 lệnh).
async fn mark_exported_core(
    pool: &SqlitePool,
    invoice_id: i64,
    layout: &str,
    sep: &str,
    actor: &str,
) -> Result<ExportPack, String> {
    let pack = build_pack(pool, invoice_id, layout, sep).await?;
    if has_errors(&pack.checks) {
        let msgs: Vec<String> = pack
            .checks
            .iter()
            .filter(|c| c.level == "error")
            .map(|c| c.message.clone())
            .collect();
        return Err(format!(
            "Chưa thể chép — cần sửa trước:\n• {}",
            msgs.join("\n• ")
        ));
    }
    if matches!(pack.header.status.as_str(), "replaced" | "cancelled") {
        return Err("Hóa đơn đã bị thay thế — không thể chép sang bên kia".into());
    }

    let before = pack.header.status.clone();
    let line_count = pack.lines.len() as i64;
    let payload_json = serde_json::to_string(&pack.json).unwrap_or_else(|_| "{}".into());
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    sqlx::query!(
        r#"INSERT INTO invoice_export (invoice_id, total, line_count, payload_json, payload_text, payload_paste)
           VALUES (?, ?, ?, ?, ?, ?)"#,
        invoice_id,
        pack.header.total,
        line_count,
        payload_json,
        pack.text,
        pack.paste
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    // Hóa đơn đã phát hành thì không hạ trạng thái về "đã xuất".
    if before != "official" {
        sqlx::query!(
            "UPDATE invoice SET status = 'exported', exported_at = datetime('now') WHERE id = ?",
            invoice_id
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        sqlx::query!(
            r#"INSERT INTO invoice_event (invoice_id, from_status, to_status, reason, actor)
               VALUES (?, ?, 'exported', ?, ?)"#,
            invoice_id,
            before,
            "Chép dữ liệu sang dịch vụ HĐĐT khác",
            actor
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    build_pack(pool, invoice_id, layout, sep).await
}

#[tauri::command]
pub(crate) async fn invoice_export_pack(
    state: State<'_, AppState>,
    invoice_id: i64,
    paste_layout: String,
    paste_sep: String,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let pack = build_pack(
        &*state.pool.read().await,
        invoice_id,
        &paste_layout,
        &paste_sep,
    )
    .await?;
    Ok(serde_json::to_string(&pack).unwrap_or_default())
}

#[tauri::command]
pub(crate) async fn invoice_mark_exported(
    state: State<'_, AppState>,
    invoice_id: i64,
    paste_layout: String,
    paste_sep: String,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let actor = state
        .current_user
        .lock()
        .await
        .as_ref()
        .map(|u| u.username.clone())
        .unwrap_or_default();
    let pack = mark_exported_core(
        &*state.pool.read().await,
        invoice_id,
        &paste_layout,
        &paste_sep,
        &actor,
    )
    .await?;
    audit(&state, "export", "invoice", &pack.header.number).await;
    Ok(serde_json::to_string(&pack).unwrap_or_default())
}

/// 1 dòng trong hàng chờ xuất: hóa đơn + kết quả kiểm tra trước khi chép.
#[derive(serde::Serialize)]
pub(crate) struct QueueItem {
    pub(crate) invoice: InvoiceRow,
    pub(crate) checks: Vec<ExportCheck>,
    pub(crate) error_count: usize,
    pub(crate) warn_count: usize,
    /// Đã chép sang bên kia ít nhất 1 lần.
    pub(crate) copied: bool,
    /// Đã chép xong mà hóa đơn bị sửa sau đó → phải sửa lại bên kia.
    pub(crate) edited_after_export: bool,
}

/// Hàng chờ xuất: hóa đơn nháp hoặc đã chép mà chưa phát hành, kèm checklist.
///
/// Gom bằng 3 truy vấn cho cả danh sách (hóa đơn · dòng hàng · bản chốt gần nhất)
/// thay vì dựng gói chép riêng cho từng hóa đơn — màn này chỉ cần biết "có chép
/// được không", không cần dựng khối dán.
/// Danh sách hóa đơn của hàng chờ xuất (chưa giới hạn) — dùng cho export và
/// cho màn hàng chờ khi dữ liệu nhỏ.
pub(crate) async fn queue_items(pool: &SqlitePool) -> Result<Vec<QueueItem>, String> {
    let invoices: Vec<InvoiceRow> = sqlx::query_as!(
        InvoiceRow,
        r#"SELECT id, number, date, customer, customer_tax_code, total, vat_amount,
                  status, e_invoice_no, e_invoice_symbol, e_invoice_date, voucher_no,
                  exported_at, replace_reason, adjust_reason, ref_invoice, adjust_voucher_no,
                  replaces_invoice_id
             FROM invoice
            WHERE status IN ('draft', 'exported')
            ORDER BY date ASC, id ASC"#
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    if invoices.is_empty() {
        return Ok(Vec::new());
    }
    assemble_queue_items(pool, invoices).await
}

/// Ghép dòng hàng + bản chốt + cờ "sửa sau khi xuất" cho một tập hóa đơn.
///
/// Tách riêng khỏi `queue_items` để màn hàng chờ phân trang: chỉ truy vấn dữ liệu
/// phụ cho đúng các hóa đơn của trang đang xem, thay vì toàn bộ hàng chờ.
async fn assemble_queue_items(
    pool: &SqlitePool,
    invoices: Vec<InvoiceRow>,
) -> Result<Vec<QueueItem>, String> {
    if invoices.is_empty() {
        return Ok(Vec::new());
    }
    // Danh sách id của trang, dùng để giới hạn 3 truy vấn phụ bên dưới.
    let ids: Vec<i64> = invoices.iter().filter_map(|i| i.id).collect();

    let groups = load_industry_groups(pool).await?;

    // Ba truy vấn phụ dưới đây dùng QueryBuilder vì danh sách id của trang là
    // động — mọi giá trị đều được bind, không ghép chuỗi SQL.
    let mut qb = sqlx::QueryBuilder::<Sqlite>::new(
        "SELECT ii.invoice_id, p.code,
                COALESCE(NULLIF(ii.line_name, ''), p.name) AS name,
                p.unit, ii.quantity, ii.unit_price,
                ii.discount, ii.subtotal, ii.industry_code, ii.vat_rate, ii.warehouse_code
           FROM invoice_item ii
           JOIN product p ON p.id = ii.product_id
           JOIN invoice i ON i.id = ii.invoice_id
          WHERE i.id IN (",
    );
    {
        let mut sep = qb.separated(", ");
        for id in &ids {
            sep.push_bind(id);
        }
    }
    qb.push(") ORDER BY ii.invoice_id, ii.id");
    let items: Vec<RawItem> = qb
        .build_query_as::<RawItem>()
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

    // Bản chốt gần nhất của mỗi hóa đơn trong trang.
    let mut qb = sqlx::QueryBuilder::<Sqlite>::new(
        "SELECT ie.invoice_id, ie.total, ie.payload_json
           FROM invoice_export ie
           JOIN (SELECT invoice_id, MAX(id) AS mid FROM invoice_export GROUP BY invoice_id) m
             ON m.mid = ie.id
          WHERE ie.invoice_id IN (",
    );
    {
        let mut sep = qb.separated(", ");
        for id in &ids {
            sep.push_bind(id);
        }
    }
    qb.push(")");
    let snapshots: Vec<(i64, f64, String)> = qb
        .build_query_as::<(i64, f64, String)>()
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

    // (mã hàng, SL, đơn giá) hiện tại của các hóa đơn trong trang.
    let mut qb = sqlx::QueryBuilder::<Sqlite>::new(
        "SELECT ii.invoice_id, p.code, ii.quantity, ii.unit_price
           FROM invoice_item ii
           JOIN product p ON p.id = ii.product_id
           JOIN invoice i ON i.id = ii.invoice_id
          WHERE i.id IN (",
    );
    {
        let mut sep = qb.separated(", ");
        for id in &ids {
            sep.push_bind(id);
        }
    }
    qb.push(") ORDER BY ii.invoice_id, ii.id");
    let now_pairs: Vec<(i64, String, f64, f64)> = qb
        .build_query_as::<(i64, String, f64, f64)>()
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

    let mut out = Vec::with_capacity(invoices.len());
    for inv in invoices {
        let Some(id) = inv.id else { continue };
        let own: Vec<&RawItem> = items.iter().filter(|r| r.invoice_id == id).collect();
        let lines: Vec<ExportLine> = own
            .iter()
            .enumerate()
            .map(|(idx, it)| to_export_line(idx, it, &groups))
            .collect();
        // Header rút gọn: checklist không dùng thông tin hồ sơ DN.
        let header = ExportHeader {
            id,
            number: inv.number.clone(),
            date: inv.date.clone(),
            date_vn: fmt_date_vn(&inv.date),
            customer: inv.customer.clone(),
            customer_tax_code: inv.customer_tax_code.clone(),
            status: inv.status.clone(),
            total: inv.total,
            vat_amount: inv.vat_amount,
            e_invoice_no: inv.e_invoice_no.clone(),
            e_invoice_symbol: inv.e_invoice_symbol.clone(),
            voucher_no: inv.voucher_no.clone(),
            biz_name: String::new(),
            biz_tax_code: String::new(),
            biz_address: String::new(),
            biz_phone: String::new(),
        };
        let checks = build_checks(&header, &lines);

        let snap = snapshots.iter().find(|(iid, _, _)| *iid == id);
        let edited_after_export = snap.is_some_and(|(_, saved_total, json)| {
            let saved: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
            let saved_pairs = saved_lines_vec(&saved);
            let now: Vec<(String, f64, f64)> = now_pairs
                .iter()
                .filter(|(iid, _, _, _)| *iid == id)
                .map(|(_, code, qty, price)| (code.clone(), *qty, *price))
                .collect();
            snapshot_differs(*saved_total, inv.total, &saved_pairs, &now)
        });

        out.push(QueueItem {
            error_count: checks.iter().filter(|c| c.level == "error").count(),
            warn_count: checks.iter().filter(|c| c.level == "warn").count(),
            checks,
            copied: snap.is_some(),
            edited_after_export,
            invoice: inv,
        });
    }
    Ok(out)
}

#[tauri::command]
pub(crate) async fn invoice_queue(state: State<'_, AppState>) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let items = queue_items(&*state.pool.read().await).await?;
    Ok(serde_json::to_string(&items).unwrap_or_default())
}

/// Chờ xuất HĐĐT — phân trang server-side (hóa đơn nháp + đã xuất chờ xử lý).
///
/// Trang hóa đơn lấy trước (có lọc + LIMIT/OFFSET), rồi mới ghép dòng hàng và bản
/// chốt CHO ĐÚNG các hóa đơn của trang — không kéo dữ liệu của cả hàng chờ về.
#[tauri::command]
pub(crate) async fn invoice_queue_page(
    state: State<'_, AppState>,
    lazy_event: String,
    status: String,
    from_date: String,
    to_date: String,
) -> Result<String, String> {
    let ev: crate::commands::page::PageEvent =
        serde_json::from_str(&lazy_event).map_err(|e| format!("lazy_event lỗi: {}", e))?;
    let page = ev.page();
    // Hàng chờ chỉ gồm hóa đơn nháp + đã xuất chờ xử lý: đây là điều kiện gốc, nối
    // thêm điều kiện khác sau (điều kiện gốc phải có mệnh đề WHERE riêng, không
    // phụ thuộc việc có từ khoá hay không).
    let (filter_sql, mut params) = crate::commands::page::global_where(
        ev.global_keyword(),
        &["number", "customer", "voucher_no", "e_invoice_no"],
    );
    let mut where_sql = String::from(" WHERE status IN ('draft', 'exported')");
    if let Some(rest) = filter_sql.strip_prefix(" WHERE ") {
        where_sql.push_str(" AND (");
        where_sql.push_str(rest);
        where_sql.push(')');
    }
    if !status.is_empty() {
        where_sql.push_str(" AND status = ?");
        params.push(crate::commands::page::BindVal(status));
    }
    if !from_date.is_empty() {
        where_sql.push_str(" AND date >= ?");
        params.push(crate::commands::page::BindVal(from_date));
    }
    if !to_date.is_empty() {
        where_sql.push_str(" AND date <= ?");
        params.push(crate::commands::page::BindVal(to_date));
    }
    let order = crate::commands::page::order_by(
        &[
            ("id", "id"),
            ("date", "date"),
            ("number", "number"),
            ("customer", "customer"),
        ],
        &ev,
        "date ASC, id ASC",
    );
    let sql = format!(
        "SELECT id, number, date, customer, customer_tax_code, total, vat_amount,
                status, e_invoice_no, e_invoice_symbol, e_invoice_date, voucher_no,
                exported_at, replace_reason, adjust_reason, ref_invoice, adjust_voucher_no,
                replaces_invoice_id
         FROM invoice{} ORDER BY {} LIMIT ? OFFSET ?",
        where_sql, order
    );
    let count_sql = format!("SELECT COUNT(*) FROM invoice{where_sql}");
    let pool = state.pool.read().await;
    let (invoices, total): (Vec<InvoiceRow>, i64) =
        crate::commands::page::fetch_page(&pool, &sql, &count_sql, &params, &page).await?;
    let items = assemble_queue_items(&pool, invoices).await?;
    Ok(
        serde_json::to_string(&crate::commands::audit::PageResult { rows: items, total })
            .unwrap_or_default(),
    )
}

/// Lịch sử trạng thái của 1 hóa đơn (đối chiếu lại với bên kia).
#[tauri::command]
pub(crate) async fn invoice_events(
    state: State<'_, AppState>,
    invoice_id: i64,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let rows = sqlx::query!(
        r#"SELECT id, at, from_status, to_status, reason, actor, ref AS ref_no
             FROM invoice_event WHERE invoice_id = ? ORDER BY id DESC"#,
        invoice_id
    )
    .fetch_all(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    let out: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            json!({
                "id": r.id, "at": r.at, "from_status": r.from_status, "to_status": r.to_status,
                "reason": r.reason, "actor": r.actor, "ref": r.ref_no
            })
        })
        .collect();
    Ok(serde_json::to_string(&out).unwrap_or_default())
}

/// Ghi nhận hóa đơn ĐIỀU CHỈNH đã phát hành bên kia (`adjusted`).
///
/// Ràng buộc kế toán: hóa đơn **đã phát hành** mà hủy thì bắt buộc phải có
/// phiếu xuất điều chỉnh (`adjust_voucher_no`) để đảo doanh thu — không cho
/// hủy "trên giấy". Bị sửa bên kia thì phải nhập số HĐĐT liên quan.
async fn set_status_core(
    pool: &SqlitePool,
    invoice_id: i64,
    to_status: &str,
    reason: &str,
    ref_invoice: &str,
    actor: &str,
) -> Result<(), String> {
    // "Hủy" không còn là cách xử lý hợp lệ (TT 91/2026 Điều 10): hóa đơn đã lập
    // chỉ thay thế / điều chỉnh. Thay thế đi qua `replace_invoice_core`.
    if to_status != "adjusted" {
        return Err(format!("Không hỗ trợ chuyển sang trạng thái '{to_status}'"));
    }
    let inv = sqlx::query!(
        r#"SELECT number, status, e_invoice_no, e_invoice_symbol FROM invoice WHERE id = ?"#,
        invoice_id
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "Không tìm thấy hóa đơn".to_string())?;

    if matches!(inv.status.as_str(), "replaced" | "cancelled") {
        return Err(format!("Hóa đơn {} đã bị thay thế rồi", inv.number));
    }
    if matches!(inv.status.as_str(), "draft" | "exported") {
        return Err("Hóa đơn chưa phát hành thì không có chuyện 'bị sửa bên kia'".into());
    }
    let reason = reason.trim();
    if reason.is_empty() {
        return Err("Cần nhập lý do".into());
    }
    if ref_invoice.trim().is_empty() {
        return Err("Cần nhập số HĐĐT điều chỉnh bên kia (ký hiệu/số) để đối chiếu".into());
    }

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    // Ghi nhận HĐĐT điều chỉnh: lý do + số HĐĐT liên quan. Số phiếu điều chỉnh đã
    // ghi khi lập phiếu (màn Xuất kho) nên không nhập ở đây nữa.
    sqlx::query!(
        r#"UPDATE invoice
              SET status = ?,
                  adjust_reason = ?,
                  ref_invoice = ?
            WHERE id = ?"#,
        to_status,
        reason,
        ref_invoice,
        invoice_id
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    sqlx::query!(
        r#"INSERT INTO invoice_event (invoice_id, from_status, to_status, reason, actor, ref)
           VALUES (?, ?, ?, ?, ?, ?)"#,
        invoice_id,
        inv.status,
        to_status,
        reason,
        actor,
        ref_invoice
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub(crate) async fn invoice_set_status(
    state: State<'_, AppState>,
    invoice_id: i64,
    to_status: String,
    reason: String,
    ref_invoice: String,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let actor = state
        .current_user
        .lock()
        .await
        .as_ref()
        .map(|u| u.username.clone())
        .unwrap_or_default();
    set_status_core(
        &*state.pool.read().await,
        invoice_id,
        &to_status,
        &reason,
        &ref_invoice,
        &actor,
    )
    .await?;
    audit(
        &state,
        &to_status,
        "invoice",
        &format!("invoice_id={invoice_id}"),
    )
    .await;
    Ok("ok".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;

    async fn seed_invoice(pool: &SqlitePool, number: &str, total: f64) -> i64 {
        sqlx::query!(
            r#"INSERT INTO invoice (number, date, customer, customer_tax_code, total, status)
               VALUES (?, '2026-09-10', 'Công ty X', '0101234567', ?, 'draft')"#,
            number,
            total
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query_scalar!(
            r#"SELECT id as "id!" FROM invoice WHERE number = ?"#,
            number
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    async fn add_line(pool: &SqlitePool, inv: i64, code: &str, name: &str, qty: f64, price: f64) {
        sqlx::query!(
            "INSERT INTO product (code, name, unit, industry_code) VALUES (?, ?, 'Cái', 'PPHH')",
            code,
            name
        )
        .execute(pool)
        .await
        .unwrap();
        let product_id: i64 =
            sqlx::query_scalar!(r#"SELECT id as "id!" FROM product WHERE code = ?"#, code)
                .fetch_one(pool)
                .await
                .unwrap();
        let subtotal = qty * price;
        sqlx::query!(
            r#"INSERT INTO invoice_item (invoice_id, product_id, quantity, unit_price, subtotal,
                                        discount, industry_code, vat_rate)
               VALUES (?, ?, ?, ?, ?, 0, 'PPHH', 0.01)"#,
            inv,
            product_id,
            qty,
            price,
            subtotal
        )
        .execute(pool)
        .await
        .unwrap();
    }

    #[test]
    fn fold_key_strips_diacritics_and_spaces() {
        assert_eq!(fold_key("  Bình  NN\tRossi "), "binh nn rossi");
        assert_eq!(fold_key("Bộ-lọc/nước"), "bo loc nuoc");
        assert_eq!(fold_key("Đường Phan_Đình Phùng"), "duong phan dinh phung");
    }

    #[test]
    fn clean_name_keeps_full_text_and_only_squeezes_spaces() {
        // Không cắt bớt chữ: dịch vụ bên kia không giới hạn ký tự tên hàng.
        let long = "Bình nước lọc trà oolong tươi đóng chai 500ml nhập khẩu chính hãng";
        assert_eq!(clean_name(long), long);
        assert_eq!(clean_name("  Bình\n nước  lọc "), "Bình nước lọc");
    }

    #[tokio::test]
    async fn pack_contains_paste_block_and_text() {
        let pool = test_pool().await;
        let id = seed_invoice(&pool, "HD0001", 30_000.0).await;
        add_line(&pool, id, "P1", "Bình NN Rossi", 2.0, 15_000.0).await;
        add_line(&pool, id, "P2", "Bộ lọc", 1.0, 0.0).await;

        let pack = build_pack(&pool, id, "full", "\t").await.unwrap();
        assert_eq!(pack.lines.len(), 2);
        assert!(pack.text.contains("Bình NN Rossi"));
        // Hóa đơn bán của hộ không tách thuế: tổng là số tiền khách trả.
        assert!(pack.text.contains("Tổng tiền: 30.000"));
        assert!(
            !pack.text.to_lowercase().contains("thuế"),
            "khối dán không được nhắc thuế: {}",
            pack.text
        );
        // Khối dán: mỗi dòng 1 dòng hàng, 6 cột theo bố cục "full".
        let paste_lines: Vec<&str> = pack.paste.trim().lines().collect();
        assert_eq!(paste_lines.len(), 2);
        assert_eq!(paste_lines[0].split('\t').count(), 6);
        assert_eq!(paste_lines[0], "Bình NN Rossi\tCái\t2\t15000\t0\t30000");
        // Số không dấu chấm phân cách nghìn để bên kia đọc đúng (15.000 → 15).
        assert!(!pack.paste.contains("15.000"), "{}", pack.paste);
        // Bên kia không nhập thuế suất → không kèm cột thuế.
        assert!(!pack.paste.contains('%'), "{}", pack.paste);
        assert_eq!(pack.paste_columns.len(), 6);
        assert!(pack.checks.iter().all(|c| c.level != "error"));
    }

    #[test]
    fn plain_numbers_have_no_thousand_separator() {
        assert_eq!(fmt_plain(15_000.0), "15000");
        assert_eq!(fmt_plain(1_250_000.0), "1250000");
        assert_eq!(fmt_plain(2.5), "2.5");
        assert_eq!(fmt_plain(0.0), "0");
    }

    #[tokio::test]
    async fn paste_layout_and_separator_reshape_the_block() {
        let pool = test_pool().await;
        let id = seed_invoice(&pool, "HD0010", 30_000.0).await;
        add_line(&pool, id, "P1", "Bình NN Rossi", 2.0, 15_000.0).await;

        // Bố cục rút gọn + dấu phẩy (nhiều phần mềm POS tách theo dấu phẩy).
        let pack = build_pack(&pool, id, "no_discount", ",").await.unwrap();
        assert_eq!(pack.paste.trim(), "Bình NN Rossi,Cái,2,15000,30000");
        assert_eq!(
            pack.paste_columns,
            vec!["Tên hàng", "ĐVT", "SL", "Đơn giá", "Thành tiền"]
        );

        let pack = build_pack(&pool, id, "amount_only", ";").await.unwrap();
        assert_eq!(pack.paste.trim(), "Bình NN Rossi;2;30000");

        // Bố cục / dấu phân cách lạ → báo lỗi thay vì dựng ra khối dán sai.
        assert!(build_pack(&pool, id, "khong_co", ",").await.is_err());
        assert!(build_pack(&pool, id, "full", "@").await.is_err());
    }

    #[tokio::test]
    async fn ten_hang_chua_dau_phan_cach_duoc_thay_bang_khoang_trang() {
        // Ô text tự do tách cột theo dấu phân cách: tên chứa đúng dấu đó sẽ vỡ
        // thành 2 cột và số liệu dán ra sai.
        let pool = test_pool().await;
        let id = seed_invoice(&pool, "HD0011", 30_000.0).await;
        add_line(&pool, id, "P1", "Bình lọc, loại lớn", 2.0, 15_000.0).await;

        let pack = build_pack(&pool, id, "no_discount", ",").await.unwrap();
        assert_eq!(pack.paste.trim(), "Bình lọc loại lớn,Cái,2,15000,30000");
        // Với Tab thì dấu phẩy trong tên vô hại, giữ nguyên.
        let pack = build_pack(&pool, id, "no_discount", "\t").await.unwrap();
        assert!(pack.paste.starts_with("Bình lọc, loại lớn\t"));
    }

    #[tokio::test]
    async fn preflight_blocks_invalid_tax_code_and_total() {
        let pool = test_pool().await;
        sqlx::query!(
            r#"INSERT INTO invoice (number, date, customer, customer_tax_code, total, status)
               VALUES ('HD0002', '2026-09-10', 'Công ty X', 'MST-SAI', 10.0, 'draft')"#
        )
        .execute(&pool)
        .await
        .unwrap();
        let id: i64 =
            sqlx::query_scalar!(r#"SELECT id as "id!" FROM invoice WHERE number = 'HD0002'"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        add_line(&pool, id, "P1", "Hàng A", 1.0, 20_000.0).await; // tổng dòng 20.000 ≠ 10.000

        let pack = build_pack(&pool, id, "full", "\t").await.unwrap();
        let errors: Vec<&str> = pack
            .checks
            .iter()
            .filter(|c| c.level == "error")
            .map(|c| c.message.as_str())
            .collect();
        assert!(errors.iter().any(|m| m.contains("MST")), "{errors:?}");
        assert!(errors.iter().any(|m| m.contains("Tổng tiền")), "{errors:?}");

        // Chặn không cho chép.
        let err = mark_exported_core(&pool, id, "full", "\t", "admin")
            .await
            .unwrap_err();
        assert!(err.contains("Chưa thể chép"), "{err}");
    }

    #[tokio::test]
    async fn mark_exported_saves_snapshot_and_flags_later_edits() {
        let pool = test_pool().await;
        let id = seed_invoice(&pool, "HD0003", 30_000.0).await;
        add_line(&pool, id, "P1", "Bình NN Rossi", 2.0, 15_000.0).await;

        let pack = mark_exported_core(&pool, id, "full", "\t", "admin")
            .await
            .unwrap();
        assert_eq!(pack.header.status, "exported");
        assert_eq!(pack.export_count, 1);
        assert!(!pack.edited_after_export);

        // Sửa hóa đơn sau khi đã chép (giả lập người dùng sửa tay) → phải báo lệch.
        sqlx::query!(
            "UPDATE invoice_item SET quantity = 3.0 WHERE invoice_id = ?",
            id
        )
        .execute(&pool)
        .await
        .unwrap();
        let after = build_pack(&pool, id, "full", "\t").await.unwrap();
        assert!(
            after.edited_after_export,
            "phải phát hiện HĐ bị sửa sau khi chép"
        );
    }

    /// Hóa đơn đã phát hành KHÔNG hủy được (TT 91/2026 Điều 10) — chỉ thay thế.
    #[tokio::test]
    async fn hoa_don_da_phat_hanh_khong_ho_duoc_huy_bien_dinh() {
        let pool = test_pool().await;
        let id = seed_invoice(&pool, "HD0004", 30_000.0).await;
        add_line(&pool, id, "P1", "Bình NN Rossi", 2.0, 15_000.0).await;
        sqlx::query!(
            "UPDATE invoice SET status = 'official', e_invoice_no = '00000123', e_invoice_symbol = '1C26TT152' WHERE id = ?",
            id
        )
        .execute(&pool)
        .await
        .unwrap();

        let err = set_status_core(&pool, id, "cancelled", "Khách trả lại", "", "admin")
            .await
            .unwrap_err();
        assert!(err.contains("Không hỗ trợ chuyển sang trạng thái"), "{err}");
    }

    #[tokio::test]
    async fn adjusted_status_requires_reference_number() {
        let pool = test_pool().await;
        let id = seed_invoice(&pool, "HD0005", 30_000.0).await;
        add_line(&pool, id, "P1", "Bình NN Rossi", 2.0, 15_000.0).await;
        sqlx::query!("UPDATE invoice SET status = 'official' WHERE id = ?", id)
            .execute(&pool)
            .await
            .unwrap();

        let err = set_status_core(&pool, id, "adjusted", "Bên kia sửa dòng hàng", "", "admin")
            .await
            .unwrap_err();
        assert!(err.contains("số HĐĐT điều chỉnh"), "{err}");

        set_status_core(
            &pool,
            id,
            "adjusted",
            "Bên kia sửa dòng hàng",
            "1C26TT152/00000124",
            "admin",
        )
        .await
        .unwrap();
        let inv = sqlx::query!("SELECT status, ref_invoice FROM invoice WHERE id = ?", id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(inv.status, "adjusted");
        assert_eq!(inv.ref_invoice, "1C26TT152/00000124");
    }

    #[tokio::test]
    async fn re_export_official_invoice_keeps_status() {
        let pool = test_pool().await;
        let id = seed_invoice(&pool, "HD0006", 30_000.0).await;
        add_line(&pool, id, "P1", "Bình NN Rossi", 2.0, 15_000.0).await;
        sqlx::query!("UPDATE invoice SET status = 'official' WHERE id = ?", id)
            .execute(&pool)
            .await
            .unwrap();

        let pack = mark_exported_core(&pool, id, "full", "\t", "admin")
            .await
            .unwrap();
        assert_eq!(
            pack.header.status, "official",
            "HĐ đã phát hành không được hạ trạng thái"
        );
        assert_eq!(pack.export_count, 1, "vẫn lưu bản chốt để đối chiếu");
    }

    #[tokio::test]
    async fn hang_cho_xuat_chi_lay_hoa_don_chua_phat_hanh() {
        let pool = test_pool().await;
        // Nháp: có trong hàng chờ.
        let d1 = seed_invoice(&pool, "HDQ1", 30_000.0).await;
        add_line(&pool, d1, "P1", "Bình NN Rossi", 2.0, 15_000.0).await;
        // Đã chép: cũng trong hàng chờ.
        let d2 = seed_invoice(&pool, "HDQ2", 30_000.0).await;
        add_line(&pool, d2, "P2", "Bộ lọc", 2.0, 15_000.0).await;
        mark_exported_core(&pool, d2, "no_discount", ",", "admin")
            .await
            .unwrap();
        // Đã phát hành: không còn việc chép → không có trong hàng chờ.
        let d3 = seed_invoice(&pool, "HDQ3", 30_000.0).await;
        add_line(&pool, d3, "P3", "Bình lọc", 2.0, 15_000.0).await;
        sqlx::query!("UPDATE invoice SET status = 'official' WHERE id = ?", d3)
            .execute(&pool)
            .await
            .unwrap();

        let q = queue_items(&pool).await.unwrap();
        let numbers: Vec<&str> = q.iter().map(|i| i.invoice.number.as_str()).collect();
        assert_eq!(numbers, vec!["HDQ1", "HDQ2"]);
        let d2row = q.iter().find(|i| i.invoice.number == "HDQ2").unwrap();
        assert!(d2row.copied, "hóa đơn đã chép thì phải đánh dấu copied");
        assert!(!d2row.edited_after_export);
        assert_eq!(d2row.error_count, 0);
    }

    #[tokio::test]
    async fn hang_cho_xuat_bao_loi_chan_khong_thay_bien_khong_hop_le() {
        let pool = test_pool().await;
        let id = seed_invoice(&pool, "HDQ4", 30_000.0).await;
        add_line(&pool, id, "P1", "Bình NN Rossi", 2.0, 15_000.0).await;
        // MST sai → lỗi chặn chép.
        sqlx::query!(
            "UPDATE invoice SET customer_tax_code = 'MST-SAI' WHERE id = ?",
            id
        )
        .execute(&pool)
        .await
        .unwrap();

        let q = queue_items(&pool).await.unwrap();
        let row = &q[0];
        assert_eq!(row.error_count, 1, "checks: {:?}", row.checks);
        assert!(row
            .checks
            .iter()
            .any(|c| c.level == "error" && c.message.contains("MST")));
        assert_eq!(row.warn_count, 0);
    }

    #[tokio::test]
    async fn hang_cho_xuat_bao_hoa_don_sua_sau_khi_chep() {
        let pool = test_pool().await;
        let id = seed_invoice(&pool, "HDQ5", 30_000.0).await;
        add_line(&pool, id, "P1", "Bình NN Rossi", 2.0, 15_000.0).await;
        mark_exported_core(&pool, id, "no_discount", ",", "admin")
            .await
            .unwrap();

        // Sửa SL sau khi đã chép → phải báo để người dùng sửa lại bên kia.
        sqlx::query!(
            "UPDATE invoice_item SET quantity = 3.0 WHERE invoice_id = ?",
            id
        )
        .execute(&pool)
        .await
        .unwrap();
        let q = queue_items(&pool).await.unwrap();
        assert!(q[0].copied);
        assert!(
            q[0].edited_after_export,
            "phải phát hiện hóa đơn lệch với bản chốt"
        );
    }

    // ─── F5 — GÓI XUẤT HĐĐT / IN DÙNG TÊN NGƯỜI DÙNG CHỌN TRÊN DÒNG ───

    #[tokio::test]
    async fn goi_xuat_dung_ten_duoc_chon_khong_fallback_ten_chinh() {
        let pool = test_pool().await;
        let id = seed_invoice(&pool, "HD-AL", 20_000.0).await;
        add_line(&pool, id, "PAL", "Bột mì", 2.0, 10_000.0).await;
        // Người dùng đã chọn tên khác trên dòng hóa đơn.
        sqlx::query!(
            "UPDATE invoice_item SET line_name = 'Mì flour'
              WHERE invoice_id = (SELECT id FROM invoice WHERE number = 'HD-AL')"
        )
        .execute(&pool)
        .await
        .unwrap();

        let (_, lines) = load_header_and_lines(&pool, id).await.unwrap();
        assert_eq!(lines[0].product_name, "Mì flour");

        // Dòng chưa chọn tên khác → dùng tên sản phẩm.
        let id2 = seed_invoice(&pool, "HD-AL2", 10_000.0).await;
        add_line(&pool, id2, "PAL2", "Đường", 1.0, 10_000.0).await;
        let (_, lines2) = load_header_and_lines(&pool, id2).await.unwrap();
        assert_eq!(lines2[0].product_name, "Đường");
    }
}
