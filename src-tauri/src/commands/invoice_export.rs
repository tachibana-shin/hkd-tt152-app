//! Quy trình xuất hóa đơn ra bên dịch vụ HĐĐT khác.
//!
//! Hộ không có API phát hành (dùng bên thứ ba) nên phải chép tay. Module này
//! gom phần "chuẩn bị chép": `preflight` kiểm tra trước khi chép (MST, tổng
//! tiền, nhóm ngành, độ dài tên dòng — bên kia chắc chắn từ chối hóa đơn thiếu
//! những thứ này), `build_pack` dựng sẵn đúng nội dung cần nhập (text thuần để
//! dán vào ô tự do, TSV để dán vào bảng — Excel tự tách cột, và JSON).
//!
//! Bấm "Chép để xuất" gọi `mark_exported`: lưu bản chốt vào `invoice_export` và
//! chuyển `draft` → `exported`. Từ đó nếu sửa hóa đơn thì `build_pack` báo
//! `edited_after_export` để người dùng biết phải sửa lại bên kia.
//!
//! `set_status` điều khiển phần còn lại của vòng đời: hủy (`cancelled`) hoặc bị
//! sửa bên kia (`adjusted`); mỗi bước ghi vào `invoice_event` để đối chiếu lại.
#![allow(unused_imports)]

use crate::helpers::*;
use crate::models::*;
use serde_json::json;
use sqlx::SqlitePool;
use tauri::State;

/// Số ký tự tên dòng hàng mà nhiều dịch vụ HĐĐT chỉ nhận tối đa (sau khi bỏ khoảng
/// trắng thừa). Vượt quá thường bị cắt cụt hoặc từ chối → app cảnh báo trước.
const NAME_LIMIT: usize = 50;

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

fn fmt_qty(v: f64) -> String {
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

/// Rút gọn tên dòng về `NAME_LIMIT` ký tự, cắt ở ranh giới từ.
pub(crate) fn shorten_name(name: &str) -> String {
    let clean = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if clean.chars().count() <= NAME_LIMIT {
        return clean;
    }
    let mut out = String::new();
    for w in clean.split(' ') {
        let next_len = out.chars().count() + if out.is_empty() { 0 } else { 1 } + w.chars().count();
        if next_len > NAME_LIMIT {
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(w);
    }
    if out.is_empty() {
        clean.chars().take(NAME_LIMIT).collect()
    } else {
        out
    }
}

/// 1 dòng hàng trong gói xuất.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct ExportLine {
    pub(crate) stt: i64,
    pub(crate) product_code: String,
    pub(crate) product_name: String,
    /// Tên đã rút gọn để dán (bên kia giới hạn ký tự).
    pub(crate) export_name: String,
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
    /// Mã mặt hàng ở bên kia (lấy từ `product_remote`, rỗng nếu chưa khớp).
    pub(crate) remote_code: String,
    pub(crate) remote_name: String,
    /// true = chưa có trong danh mục bên kia (xem module `product_remote`).
    pub(crate) needs_remote_product: bool,
    pub(crate) warehouse_code: String,
    /// Cảnh báo riêng của dòng (tên quá dài sau rút gọn, chưa có mã bên kia…).
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
    pub(crate) computed_vat: f64,
    pub(crate) text: String,
    pub(crate) tsv: String,
    pub(crate) json: serde_json::Value,
    /// Số bản chốt đã lưu (0 = lần đầu).
    pub(crate) export_count: i64,
    /// Hóa đơn đã bị sửa sau lần chép gần nhất chưa.
    pub(crate) edited_after_export: bool,
}

fn has_errors(checks: &[ExportCheck]) -> bool {
    checks.iter().any(|c| c.level == "error")
}

/// Nạp header + dòng hàng (kèm nhóm ngành và ánh xạ mặt hàng bên kia nếu có).
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

    let groups: Vec<(String, String)> = sqlx::query!("SELECT code, name FROM industry_group")
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|g| (g.code, g.name))
        .collect();

    let items = sqlx::query!(
        r#"SELECT ii.id, p.code, p.name, p.unit, ii.quantity, ii.unit_price, ii.discount,
                  ii.subtotal, ii.industry_code, ii.vat_rate, ii.warehouse_code,
                  COALESCE(pr.remote_code, '') AS remote_code,
                  COALESCE(pr.remote_name, '') AS remote_name
             FROM invoice_item ii
             JOIN product p ON p.id = ii.product_id
             LEFT JOIN product_remote pr ON pr.product_id = p.id
            WHERE ii.invoice_id = ?
            ORDER BY ii.id"#,
        invoice_id
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let mut lines = Vec::with_capacity(items.len());
    for (idx, it) in items.iter().enumerate() {
        let clean = it.name.split_whitespace().collect::<Vec<_>>().join(" ");
        let export_name = shorten_name(&it.name);
        let mut warnings = Vec::new();
        if clean.chars().count() > NAME_LIMIT {
            warnings.push(format!(
                "Tên dài {} ký tự — đã rút gọn còn {} ký tự để dán",
                clean.chars().count(),
                export_name.chars().count()
            ));
        }
        let remote_ok = !it.remote_code.trim().is_empty() || !it.remote_name.trim().is_empty();
        if !remote_ok {
            warnings.push("Chưa có mặt hàng tương ứng ở bên kia — xem màn Khớp mặt hàng".into());
        }
        if it.vat_rate == 0.0 && it.industry_code.trim().is_empty() {
            warnings.push("Thuế 0% nhưng chưa có nhóm ngành".into());
        }
        let industry_name = groups
            .iter()
            .find(|(c, _)| *c == it.industry_code)
            .map(|(_, n)| n.clone())
            .unwrap_or_default();
        lines.push(ExportLine {
            stt: idx as i64 + 1,
            product_code: it.code.clone(),
            product_name: it.name.clone(),
            export_name,
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
            remote_code: it.remote_code.clone(),
            remote_name: it.remote_name.clone(),
            needs_remote_product: !remote_ok,
            warehouse_code: it.warehouse_code.clone(),
            warnings,
        });
    }

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

    let missing_remote = lines.iter().filter(|l| l.needs_remote_product).count();
    if missing_remote > 0 {
        checks.push(ExportCheck {
            level: "warn".into(),
            message: format!(
                "{missing_remote}/{} dòng chưa có mặt hàng tương ứng ở bên kia — dùng màn Khớp mặt hàng để ánh xạ 1 lần",
                lines.len()
            ),
        });
    }

    if header.status == "cancelled" {
        checks.push(ExportCheck {
            level: "error".into(),
            message: "Hóa đơn đã hủy — không thể chép sang bên kia".into(),
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

fn render_text(header: &ExportHeader, lines: &[ExportLine], computed_vat: f64) -> String {
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
    s.push_str("STT\tTên hàng\tĐVT\tSL\tĐơn giá\tChiết khấu\tThành tiền\tThuế\n");
    for l in lines {
        s.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            l.stt,
            l.export_name,
            l.unit,
            fmt_qty(l.quantity),
            fmt_money(l.unit_price),
            fmt_money(l.discount),
            fmt_money(l.amount),
            l.vat_label
        ));
    }
    s.push_str(&format!(
        "\nTổng tiền trước thuế: {}\nThuế GTGT: {}\nTỔNG THANH TOÁN: {}\n",
        fmt_money(header.total),
        fmt_money(computed_vat),
        fmt_money(header.total + computed_vat)
    ));
    s
}

fn render_tsv(lines: &[ExportLine]) -> String {
    let mut s = String::new();
    for l in lines {
        s.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            l.export_name,
            l.unit,
            fmt_qty(l.quantity),
            fmt_money(l.unit_price),
            fmt_money(l.discount),
            fmt_money(l.amount),
            l.vat_label
        ));
    }
    s
}

/// Dựng gói chép hoàn chỉnh (dùng cho cả dialog lẫn bản chốt).
pub(crate) async fn build_pack(pool: &SqlitePool, invoice_id: i64) -> Result<ExportPack, String> {
    let (header, lines) = load_header_and_lines(pool, invoice_id).await?;
    let checks = build_checks(&header, &lines);
    let computed_total: f64 = lines.iter().map(|l| l.amount).sum();
    let computed_vat: f64 = lines.iter().map(|l| l.amount * l.vat_rate).sum();
    let text = render_text(&header, &lines, computed_vat);
    let tsv = render_tsv(&lines);
    let json = json!({
        "number": header.number,
        "date": header.date_vn,
        "customer": { "name": header.customer, "tax_code": header.customer_tax_code },
        "payment_method": "Chuyển khoản",
        "lines": lines,
        "total_before_vat": header.total,
        "vat_amount": computed_vat,
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
        computed_total,
        computed_vat,
        text,
        tsv,
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
    let saved_lines = saved
        .get("lines")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    let now_lines: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM invoice_item WHERE invoice_id = ?",
        invoice_id
    )
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;
    if saved_lines as i64 != now_lines {
        return Ok(false);
    }
    // So tiền từng dòng theo mã hàng + số lượng + giá.
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
    if saved_pairs.len() != now_pairs.len() {
        return Ok(false);
    }
    Ok(saved_pairs
        .iter()
        .zip(now_pairs.iter())
        .all(|(a, b)| a.0 == b.0 && (a.1 - b.1).abs() < 1e-6 && (a.2 - b.2).abs() < 1e-6))
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
    actor: &str,
) -> Result<ExportPack, String> {
    let pack = build_pack(pool, invoice_id).await?;
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
    if pack.header.status == "cancelled" {
        return Err("Hóa đơn đã hủy — không thể chép sang bên kia".into());
    }

    let before = pack.header.status.clone();
    let line_count = pack.lines.len() as i64;
    let payload_json = serde_json::to_string(&pack.json).unwrap_or_else(|_| "{}".into());
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    sqlx::query!(
        r#"INSERT INTO invoice_export (invoice_id, total, line_count, payload_json, payload_text, payload_tsv)
           VALUES (?, ?, ?, ?, ?, ?)"#,
        invoice_id,
        pack.header.total,
        line_count,
        payload_json,
        pack.text,
        pack.tsv
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
    build_pack(pool, invoice_id).await
}

#[tauri::command]
pub(crate) async fn invoice_export_pack(
    state: State<'_, AppState>,
    invoice_id: i64,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let pack = build_pack(&*state.pool.read().await, invoice_id).await?;
    Ok(serde_json::to_string(&pack).unwrap_or_default())
}

#[tauri::command]
pub(crate) async fn invoice_mark_exported(
    state: State<'_, AppState>,
    invoice_id: i64,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let actor = state
        .current_user
        .lock()
        .await
        .as_ref()
        .map(|u| u.username.clone())
        .unwrap_or_default();
    let pack = mark_exported_core(&*state.pool.read().await, invoice_id, &actor).await?;
    audit(&state, "export", "invoice", &pack.header.number).await;
    Ok(serde_json::to_string(&pack).unwrap_or_default())
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

/// Đổi trạng thái hóa đơn: hủy (`cancelled`) hoặc bị sửa bên kia (`adjusted`).
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
    adjust_voucher_no: &str,
    actor: &str,
) -> Result<(), String> {
    if !matches!(to_status, "cancelled" | "adjusted") {
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

    if inv.status == "cancelled" {
        return Err(format!("Hóa đơn {} đã hủy rồi", inv.number));
    }
    if matches!(inv.status.as_str(), "draft" | "exported") && to_status == "adjusted" {
        return Err("Hóa đơn chưa phát hành thì không có chuyện 'bị sửa bên kia'".into());
    }
    let reason = reason.trim();
    if reason.is_empty() {
        return Err("Cần nhập lý do".into());
    }
    // Hủy HĐ đã phát hành → cần phiếu điều chỉnh.
    if to_status == "cancelled" && inv.status == "official" && adjust_voucher_no.trim().is_empty() {
        return Err(
            "Hóa đơn đã phát hành: hãy lập phiếu xuất điều chỉnh (Nợ 511 / Có 131) rồi nhập số phiếu — không thể hủy mà không đảo sổ"
                .into(),
        );
    }
    if to_status == "adjusted" && ref_invoice.trim().is_empty() {
        return Err("Cần nhập số HĐĐT bên kia (ký hiệu/số) để đối chiếu".into());
    }

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    sqlx::query!(
        r#"UPDATE invoice
              SET status = ?,
                  cancel_reason = CASE WHEN ? = 'cancelled' THEN ? ELSE cancel_reason END,
                  adjust_reason = CASE WHEN ? = 'adjusted' THEN ? ELSE adjust_reason END,
                  ref_invoice = CASE WHEN ? <> '' THEN ? ELSE ref_invoice END,
                  adjust_voucher_no = CASE WHEN ? <> '' THEN ? ELSE adjust_voucher_no END
            WHERE id = ?"#,
        to_status,
        to_status,
        reason,
        to_status,
        reason,
        ref_invoice,
        ref_invoice,
        adjust_voucher_no,
        adjust_voucher_no,
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
    adjust_voucher_no: String,
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
        &adjust_voucher_no,
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
    fn shorten_name_cuts_at_word_boundary() {
        let long = "Bình nước lọc trà oolong tươi đóng chai 500ml nhập khẩu chính hãng";
        let out = shorten_name(long);
        assert!(
            out.chars().count() <= NAME_LIMIT,
            "còn {} ký tự",
            out.chars().count()
        );
        // Không cắt giữa từ: chuỗi kết quả vẫn là tiền tố của chuỗi gốc đã gộp space.
        let clean = long.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(clean.starts_with(&out), "phải cắt ở ranh giới từ: {out}");
    }

    #[tokio::test]
    async fn pack_contains_tsv_and_text_for_copy() {
        let pool = test_pool().await;
        let id = seed_invoice(&pool, "HD0001", 30_000.0).await;
        add_line(&pool, id, "P1", "Bình NN Rossi", 2.0, 15_000.0).await;
        add_line(&pool, id, "P2", "Bộ lọc", 1.0, 0.0).await;

        let pack = build_pack(&pool, id).await.unwrap();
        assert_eq!(pack.lines.len(), 2);
        assert!(pack.text.contains("Bình NN Rossi"));
        assert!(pack.text.contains("TỔNG THANH TOÁN: 30.300")); // 30.000 + 1% thuế
                                                                // TSV: mỗi dòng 7 cột, tách bằng tab (dán Excel tự tách cột).
        let tsv_lines: Vec<&str> = pack.tsv.trim().lines().collect();
        assert_eq!(tsv_lines.len(), 2);
        assert_eq!(tsv_lines[0].split('\t').count(), 7);
        assert!(tsv_lines[0].starts_with("Bình NN Rossi\tCái\t2\t15.000"));
        // Không có mặt hàng bên kia → cảnh báo (mức warn, không chặn).
        assert!(pack.lines.iter().all(|l| l.needs_remote_product));
        assert!(pack.checks.iter().all(|c| c.level != "error"));
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

        let pack = build_pack(&pool, id).await.unwrap();
        let errors: Vec<&str> = pack
            .checks
            .iter()
            .filter(|c| c.level == "error")
            .map(|c| c.message.as_str())
            .collect();
        assert!(errors.iter().any(|m| m.contains("MST")), "{errors:?}");
        assert!(errors.iter().any(|m| m.contains("Tổng tiền")), "{errors:?}");

        // Chặn không cho chép.
        let err = mark_exported_core(&pool, id, "admin").await.unwrap_err();
        assert!(err.contains("Chưa thể chép"), "{err}");
    }

    #[tokio::test]
    async fn mark_exported_saves_snapshot_and_flags_later_edits() {
        let pool = test_pool().await;
        let id = seed_invoice(&pool, "HD0003", 30_000.0).await;
        add_line(&pool, id, "P1", "Bình NN Rossi", 2.0, 15_000.0).await;

        let pack = mark_exported_core(&pool, id, "admin").await.unwrap();
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
        let after = build_pack(&pool, id).await.unwrap();
        assert!(
            after.edited_after_export,
            "phải phát hiện HĐ bị sửa sau khi chép"
        );
    }

    #[tokio::test]
    async fn cancel_official_invoice_requires_adjustment_voucher() {
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

        let err = set_status_core(&pool, id, "cancelled", "Khách trả lại", "", "", "admin")
            .await
            .unwrap_err();
        assert!(err.contains("phiếu xuất điều chỉnh"), "{err}");

        set_status_core(
            &pool,
            id,
            "cancelled",
            "Khách trả lại hàng",
            "",
            "PX0009",
            "admin",
        )
        .await
        .unwrap();
        let inv = sqlx::query!(
            r#"SELECT status, cancel_reason, adjust_voucher_no FROM invoice WHERE id = ?"#,
            id
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(inv.status, "cancelled");
        assert_eq!(inv.cancel_reason, "Khách trả lại hàng");
        assert_eq!(inv.adjust_voucher_no, "PX0009");

        // Lịch sử có ghi lại bước chuyển trạng thái.
        let events: Vec<(String, String)> = sqlx::query!(
            "SELECT from_status, to_status FROM invoice_event WHERE invoice_id = ? ORDER BY id",
            id
        )
        .fetch_all(&pool)
        .await
        .unwrap()
        .into_iter()
        .map(|r| (r.from_status, r.to_status))
        .collect();
        assert_eq!(events, vec![("official".into(), "cancelled".into())]);
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

        let err = set_status_core(
            &pool,
            id,
            "adjusted",
            "Bên kia sửa dòng hàng",
            "",
            "",
            "admin",
        )
        .await
        .unwrap_err();
        assert!(err.contains("số HĐĐT bên kia"), "{err}");

        set_status_core(
            &pool,
            id,
            "adjusted",
            "Bên kia sửa dòng hàng",
            "1C26TT152/00000124",
            "",
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

        let pack = mark_exported_core(&pool, id, "admin").await.unwrap();
        assert_eq!(
            pack.header.status, "official",
            "HĐ đã phát hành không được hạ trạng thái"
        );
        assert_eq!(pack.export_count, 1, "vẫn lưu bản chốt để đối chiếu");
    }
}
