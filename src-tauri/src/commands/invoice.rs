#![allow(unused_imports)]

use crate::helpers::*;
use crate::models::*;
use serde_json::json;
use sqlx::sqlite::{SqlitePoolOptions, SqliteTransaction};
use sqlx::{Sqlite, SqlitePool};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use tauri::{AppHandle, Manager, State};
#[tauri::command]
pub(crate) async fn get_invoices(state: State<'_, AppState>) -> Result<String, String> {
    let rows: Vec<InvoiceRow> = sqlx::query_as!(
        InvoiceRow,
        "SELECT id, number, date, customer, customer_tax_code, total, vat_amount,
                status, e_invoice_no, e_invoice_symbol, e_invoice_date, voucher_no,
                exported_at, replace_reason, adjust_reason, ref_invoice, adjust_voucher_no,
                replaces_invoice_id
         FROM invoice ORDER BY date DESC, id DESC"
    )
    .fetch_all(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    Ok(serde_json::to_string(&rows).unwrap_or_default())
}

/// Hóa đơn — phân trang server-side (màn Hóa đơn không lọc theo ngày nên danh
/// sách phình to theo thời gian; kéo hết về client rồi cắt trang là đơ).
#[tauri::command]
pub(crate) async fn get_invoices_page(
    state: State<'_, AppState>,
    lazy_event: String,
    only_duplicates: bool,
) -> Result<String, String> {
    invoices_page_core(&*state.pool.read().await, &lazy_event, only_duplicates).await
}

/// Ngày dùng để sắp xếp danh sách hóa đơn: ưu tiên "Ngày HĐĐT"; hóa đơn chưa
/// ghi ngày HĐĐT (nháp / dữ liệu cũ) lấy ngày hóa đơn để không bị dồn xuống cuối.
const INVOICE_SORT_DATE: &str = "COALESCE(NULLIF(e_invoice_date, ''), date)";

/// Tách khỏi lệnh Tauri để test được phần truy vấn/sắp xếp mà không cần State.
async fn invoices_page_core(
    pool: &SqlitePool,
    lazy_event: &str,
    only_duplicates: bool,
) -> Result<String, String> {
    let ev: crate::commands::page::PageEvent =
        serde_json::from_str(lazy_event).map_err(|e| format!("lazy_event lỗi: {}", e))?;
    let page = ev.page();
    let (mut where_sql, params) = crate::commands::page::global_where(
        ev.global_keyword(),
        &["number", "customer", "voucher_no", "e_invoice_no"],
    );
    // Nút "Chỉ hiện hóa đơn trùng số" — lọc ở server để không bỏ sót bản trùng
    // nằm ở trang khác (trước đây lọc client trên toàn bộ danh sách đã tải).
    if only_duplicates {
        where_sql.push_str(
            " AND number IN (SELECT number FROM invoice GROUP BY number HAVING COUNT(*) > 1)",
        );
    }
    // Mặc định mới nhất theo Ngày HĐĐT (yêu cầu #7); id giữ thứ tự ổn định khi
    // nhiều hóa đơn cùng ngày để phân trang không nhảy dòng.
    let fallback = format!("{INVOICE_SORT_DATE} DESC");
    let order = format!(
        "{}, id DESC",
        crate::commands::page::order_by(
            &[
                ("id", "id"),
                ("date", "date"),
                ("number", "number"),
                ("customer", "customer"),
                ("e_invoice_date", INVOICE_SORT_DATE),
            ],
            &ev,
            &fallback,
        )
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
    let (rows, total): (Vec<InvoiceRow>, i64) =
        crate::commands::page::fetch_page(pool, &sql, &count_sql, &params, &page).await?;
    Ok(
        serde_json::to_string(&crate::commands::audit::PageResult { rows, total })
            .unwrap_or_default(),
    )
}

#[tauri::command]
pub(crate) async fn get_invoice_detail(
    state: State<'_, AppState>,
    id: i64,
) -> Result<String, String> {
    let inv: InvoiceRow = sqlx::query_as!(
        InvoiceRow,
        "SELECT id, number, date, customer, customer_tax_code, total, vat_amount,
                status, e_invoice_no, e_invoice_symbol, e_invoice_date, voucher_no,
                exported_at, replace_reason, adjust_reason, ref_invoice, adjust_voucher_no,
                replaces_invoice_id
         FROM invoice WHERE id = ?",
        id
    )
    .fetch_one(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    let items: Vec<InvoiceItemRow> = sqlx::query_as::<_, InvoiceItemRow>(
        "SELECT ii.id, ii.invoice_id, p.code AS product_code,
                -- Tên hiển thị: ưu tiên tên người dùng chọn trên dòng (có thể là
                -- tên khác / alias) — không fallback về tên chính (F5).
                COALESCE(NULLIF(ii.line_name, ''), p.name) AS product_name,
                p.unit, ii.quantity, ii.unit_price, ii.subtotal,
                ii.discount, ii.warehouse_code,
                ii.industry_code, ii.vat_rate, ii.pit_rate, ii.line_name
         FROM invoice_item ii
         JOIN product p ON p.id = ii.product_id
         WHERE ii.invoice_id = ?
         ORDER BY ii.id",
    )
    .bind(id)
    .fetch_all(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "invoice": inv, "items": items }).to_string())
}

/// Nhãn thuế suất hiển thị trên cột "Thuế suất" — đúng dạng chuỗi `"8%"` mà
/// bảng tổng hợp của cổng HĐĐT (`thttltsuat.tsuat`) đang dùng.
fn vat_rate_label(rate: f64) -> String {
    format!("{}%", (rate * 100.0).round() as i64)
}

/// Dựng `detail_json` cho hóa đơn **nội bộ** để xem trước PDF ngay ở màn
/// Chờ xuất HĐĐT — cùng khung dữ liệu với chi tiết tải từ cổng HĐĐT
/// (`hddt_purchase_invoice.detail_json`), render qua [`crate::commands::pdf`].
///
/// Khác hóa đơn của cổng ở chỗ hộ kinh doanh **không tách thuế GTGT** trên hóa
/// đơn (`invoice.vat_amount` luôn 0, xem `save_invoice_core`) nên bảng tổng hợp
/// theo thuế suất để trống — chỉ cột "Thuế suất" từng dòng (khi xài nhiều tỉ lệ)
/// và bảng thành tiền + chiết khấu là giữ lại cho đúng khung in.
pub(crate) async fn invoice_draft_detail_json(
    pool: &SqlitePool,
    id: i64,
) -> Result<String, String> {
    let invoice = sqlx::query!(
        "SELECT number, date, customer, customer_tax_code, total, vat_amount
           FROM invoice WHERE id = ?",
        id
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| format!("Không tìm thấy hóa đơn #{id}"))?;

    // Bên bán: hồ sơ hộ kinh doanh (ký hiệu HĐĐT của hộ = ký hiệu hóa đơn).
    let (nb_name, nb_tax_code, nb_address, nb_phone, nb_symbol) = match sqlx::query!(
        "SELECT name, tax_code, address, phone, hddt_symbol FROM business WHERE id = 1"
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    {
        Some(b) => (b.name, b.tax_code, b.address, b.phone, b.hddt_symbol),
        None => (
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        ),
    };

    let items = sqlx::query!(
        "SELECT COALESCE(NULLIF(ii.line_name, ''), p.name) AS \"name!: String\", p.unit AS unit,
                ii.quantity, ii.unit_price,
                ii.subtotal, ii.discount, ii.vat_rate
           FROM invoice_item ii
           JOIN product p ON p.id = ii.product_id
          WHERE ii.invoice_id = ?
          ORDER BY ii.id",
        id
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    // `thtien` = thành tiền TRƯỚC chiết khấu, `stckhau` = tiền chiết khấu (như
    // hóa đơn cổng) → "Cộng tiền hàng" là cộng thành tiền, tổng thanh toán là
    // `invoice.total` đã trừ chiết khấu.
    let mut rows = Vec::with_capacity(items.len());
    let mut gross = 0.0;
    for (i, item) in items.iter().enumerate() {
        let line_gross = round2(item.quantity * item.unit_price);
        gross += line_gross;
        rows.push(json!({
            "stt": i + 1,
            "tchat": 1,
            "ten": item.name,
            "dvtinh": item.unit,
            "sluong": item.quantity,
            "dgia": round2(item.unit_price),
            "thtien": line_gross,
            "stckhau": round2(item.discount),
            "ltsuat": vat_rate_label(item.vat_rate),
            "tsuat": item.vat_rate,
        }));
    }
    let gross = round2(gross);
    let total = round2(invoice.total);

    Ok(json!({
        // Mẫu số để trống (hộ chưa khai trong hồ sơ), ký hiệu lấy từ hồ sơ HKD.
        "khmshdon": "",
        "khhdon": nb_symbol,
        "shdon": invoice.number,
        "nky": invoice.date,
        "tchat": 1,
        "nbten": nb_name,
        "nbmst": nb_tax_code,
        "nbdchi": nb_address,
        "nbsdthoai": nb_phone,
        "nmten": invoice.customer,
        "nmmst": invoice.customer_tax_code,
        "tgtcthue": gross,
        "tgtthue": round2(invoice.vat_amount),
        "tgtphi": 0,
        "ttcktmai": round2(gross - total),
        "tgtttbso": total,
        "tgtttbchu": vietnamese_amount_in_words(total),
        "dvtte": "VND",
        "tgia": 1,
        "qrcode": "",
        "hdhhdvu": rows,
        "thttltsuat": Vec::<serde_json::Value>::new(),
    })
    .to_string())
}

/// QR in trên bản xem trước — hộ chưa có trang xác minh riêng nên tạm trỏ về
/// trang dự án; cứ có URL là PDF mới vẽ ô QR (để trống thì bỏ ô, xem `qr.rs`).
const PREVIEW_QR_URL: &str = "https://github.com/tachibana-shin";

/// Dựng `detail_json` cho màn **xem trước** PDF (Chờ xuất HĐĐT, tab Hóa đơn).
///
/// Khác [`invoice_draft_detail_json`] — bản đó dùng chung cho ZIP báo cáo kỳ
/// thuế nên giữ nguyên — bản xem trước phân rõ 2 nhánh:
/// * nháp / đã chép (chưa có số hiệu của cổng): in **HÓA ĐƠN NHÁP**, mẫu số và
///   ký hiệu tạm ghi `000000` để không ai tưởng đây là hóa đơn thật;
/// * đã phát hành: giữ nguyên khung in, ưu tiên số/ký hiệu/ngày HĐĐT đã ghi
///   (ký hiệu lấy từ hồ sơ hộ khi chưa ghi), chỉ thêm QR.
pub(crate) async fn invoice_preview_json(pool: &SqlitePool, id: i64) -> Result<String, String> {
    let mut data: serde_json::Value =
        serde_json::from_str(&invoice_draft_detail_json(pool, id).await?)
            .map_err(|e| e.to_string())?;

    let invoice = sqlx::query!(
        "SELECT status, e_invoice_no, e_invoice_symbol, e_invoice_date
           FROM invoice WHERE id = ?",
        id
    )
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;
    if matches!(invoice.status.as_str(), "draft" | "exported") {
        data["tlhdon"] = json!("HÓA ĐƠN NHÁP");
        data["khmshdon"] = json!("000000");
        data["khhdon"] = json!("000000");
    } else {
        // Hóa đơn đã phát hành: số/ký hiệu/ngày trên PDF phải lấy từ HĐĐT thật
        // (bản nội bộ chỉ dùng khi chưa ghi nhận số HĐĐT).
        if !invoice.e_invoice_no.trim().is_empty() {
            data["shdon"] = json!(invoice.e_invoice_no.trim());
        }
        if !invoice.e_invoice_symbol.trim().is_empty() {
            data["khhdon"] = json!(invoice.e_invoice_symbol.trim());
        }
        if !invoice.e_invoice_date.trim().is_empty() {
            data["nky"] = json!(invoice.e_invoice_date.trim());
        }
    }
    data["qrcode"] = json!(PREVIEW_QR_URL);
    Ok(data.to_string())
}

/// Xem trước PDF hóa đơn nội bộ (nháp hoặc đã phát hành) ở màn Chờ xuất HĐĐT
/// và tab Hóa đơn.
#[tauri::command]
pub(crate) async fn invoice_draft_detail(
    state: State<'_, AppState>,
    id: i64,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let pool = state.pool.read().await;
    invoice_preview_json(&pool, id).await
}

/// 1 dòng hóa đơn đã qua kiểm tra (có sẵn mã sản phẩm, nhóm ngành, tỷ lệ thuế).
struct PreparedLine {
    product_id: i64,
    quantity: f64,
    unit_price: f64,
    /// Giá trị dòng = Thành tiền (SL × Đơn giá) − Tiền CK (chiết khấu).
    subtotal: f64,
    industry_code: String,
    vat_rate: f64,
    pit_rate: f64,
    discount: f64,
    warehouse_code: String,
    /// Tên người dùng chọn trên dòng (tên khác / alias) — rỗng = tên sản phẩm.
    line_name: String,
}

struct PreparedInvoice {
    total: f64,
    tax_payable: f64,
    lines: Vec<PreparedLine>,
    /// Dòng thiếu tồn (rỗng = đủ) theo ĐÚNG kho xuất trên dòng, tính trên TỒN
    /// CÒN TRỐNG sau khi trừ phần hàng đã bị các hóa đơn nháp khác "giữ chỗ"
    /// (giả lập trừ kho cho nháp — nháp chưa ghi phiếu xuất lần nào nhưng hàng
    /// đã được hứa). Nháp KHÔNG bị chặn: HĐĐT đã xuất vẫn cần thời gian mới
    /// lên cổng thông tin điện tử, nên app hỏi người dùng rồi vẫn lưu — nhưng
    /// phải trả danh sách này về để báo.
    shortage: Vec<String>,
}

/// Phần số lượng MẶT HÀNG đang bị các hóa đơn nháp KHÁC "giữ chỗ".
///
/// Giả lập trừ kho cho nháp (issue #5): nháp chưa ghi phiếu xuất lần nào nên
/// chưa trừ kho thật, nhưng số hàng đã được hứa cho hóa đơn đó — lập thêm hóa
/// đơn nháp nữa phải coi phần này đã hết. Chỉ đếm nháp CHƯA gắn phiếu xuất:
/// hóa đơn đã gắn phiếu xuất là kho đã trừ thật, trừ tiếp là tính hai lần.
/// Dòng nháp không khai kho (rỗng) lấy máy nào cũng được nên tính vào MỌI kho.
/// `except_id` mở trừ chính hóa đơn đang xét (lúc sửa).
async fn draft_reserved_qty(
    tx: &mut SqliteTransaction<'_>,
    product_id: i64,
    warehouse_code: &str,
    except_id: Option<i64>,
) -> Result<f64, String> {
    let wh = warehouse_code.trim();
    sqlx::query_scalar!(
        r#"SELECT COALESCE(SUM(ii.quantity), 0.0) as "qty!: f64"
             FROM invoice_item ii
             JOIN invoice i ON i.id = ii.invoice_id
            WHERE ii.product_id = ?
              AND i.status = 'draft'
              AND COALESCE(i.voucher_no, '') = ''
              AND (? = '' OR COALESCE(ii.warehouse_code, '') = ? OR COALESCE(ii.warehouse_code, '') = '')
              AND (? IS NULL OR i.id <> ?)"#,
        product_id,
        wh,
        wh,
        except_id,
        except_id,
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(|e| e.to_string())
}

/// Kiểm tra + dựng dòng hóa đơn từ input người dùng: nhóm ngành phải hợp lệ,
/// soi tồn kho theo ĐÚNG kho xuất trên từng dòng (rỗng → tổng mọi kho) và tính
/// tổng tiền. Dùng chung cho lập mới và sửa hóa đơn nháp.
///
/// `except_id` = hóa đơn đang xét (sửa nháp thì bỏ qua chính nó — không tự soi
/// phần "giữ chỗ" của chính mình). `None` khi lập hóa đơn mới.
async fn prepare_invoice(
    tx: &mut SqliteTransaction<'_>,
    items: &[InvoiceItemInput],
    except_id: Option<i64>,
) -> Result<PreparedInvoice, String> {
    if items.is_empty() {
        return Err("Chưa có mặt hàng nào trong hóa đơn".into());
    }

    let mut basis: Vec<(String, f64, f64)> = Vec::with_capacity(items.len());
    let mut shortage: Vec<String> = Vec::new();
    for item in items {
        let product = resolve_product(tx, &item.product_code).await?;

        // Nhóm ngành trên dòng: ưu tiên khai trên dòng, rỗng → nhóm ngành
        // mặc định của sản phẩm (cơ sở tỷ lệ thuế bán ra).
        let industry = if item.industry_code.trim().is_empty() {
            product.industry_code.clone()
        } else {
            item.industry_code.trim().to_string()
        };
        if industry.is_empty() {
            return Err(format!(
                "Sản phẩm '{}' chưa có nhóm ngành — hãy gán nhóm ngành cho sản phẩm (màn Sản phẩm) hoặc chọn nhóm ngành trên dòng",
                item.product_code
            ));
        }
        let group = sqlx::query!(
            "SELECT vat_rate, pit_rate FROM industry_group WHERE code = ?",
            industry
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(|_| format!("Nhóm ngành '{}' không hợp lệ", industry))?;
        let (vat_rate, pit_rate) = (group.vat_rate, group.pit_rate);
        basis.push((industry, vat_rate, pit_rate));

        // Sản phẩm dịch vụ (nhân công...) không theo dõi tồn kho → bỏ qua kiểm tra.
        if product.is_service {
            continue;
        }
        // Kiểm tra tồn theo ĐÚNG kho xuất trên dòng (rỗng → tổng mọi kho).
        let available: f64 = if item.warehouse_code.trim().is_empty() {
            sqlx::query_scalar!(
                r#"SELECT COALESCE(SUM(quantity), 0.0) as "qty!: f64" FROM stock_lot
                   WHERE product_id = ? AND depleted = 0"#,
                product.id
            )
            .fetch_one(&mut **tx)
            .await
            .map_err(|e| e.to_string())?
        } else {
            let wh_id = resolve_warehouse(tx, &item.warehouse_code, &item.product_code).await?;
            sqlx::query_scalar!(
                r#"SELECT COALESCE(SUM(quantity), 0.0) as "qty!: f64" FROM stock_lot
                   WHERE product_id = ? AND warehouse_id = ? AND depleted = 0"#,
                product.id,
                wh_id
            )
            .fetch_one(&mut **tx)
            .await
            .map_err(|e| e.to_string())?
        };
        // Giả lập trừ kho cho nháp: phần hàng đã bị các hóa đơn nháp khác
        // "giữ chỗ" không còn trống, lập tiếp phải coi là hết hàng.
        let reserved = draft_reserved_qty(tx, product.id, &item.warehouse_code, except_id).await?;
        let free = (available - reserved).max(0.0);
        let where_str = if item.warehouse_code.trim().is_empty() {
            "tổng các kho"
        } else {
            "kho đã chọn"
        };
        if item.quantity - free > 1e-9 {
            if reserved > 1e-9 {
                shortage.push(format!(
                    "{} ({}): cần {:.2}, tồn {} chỉ còn {:.2} — tổng tồn {:.2}, các hóa đơn nháp khác đã chiếm {:.2}",
                    product.name, item.product_code, item.quantity, where_str, free, available, reserved
                ));
            } else {
                shortage.push(format!(
                    "{} ({}): cần {:.2}, tồn {} {:.2}",
                    product.name, item.product_code, item.quantity, where_str, available
                ));
            }
        }
    }

    // Hóa đơn bán hàng của HKD không tách thuế GTGT (không ghi thuế trên hóa đơn).
    // Số thuế phải nộp tính theo tỷ lệ nhóm ngành × doanh thu (tax_payable) để
    // theo dõi; trường "Thuế suất GTGT" của sản phẩm là thuế đầu vào, không dùng.
    let mut total = 0.0;
    let mut tax_payable = 0.0;
    let mut lines = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        let product = resolve_product(tx, &item.product_code).await?;
        let subtotal = round2(item.quantity * item.unit_price - item.discount);
        total += subtotal;
        tax_payable += subtotal * basis[i].1;
        let (basis_code, basis_vat, basis_pit) = basis[i].clone();
        lines.push(PreparedLine {
            product_id: product.id,
            quantity: item.quantity,
            unit_price: item.unit_price,
            subtotal,
            industry_code: basis_code,
            vat_rate: basis_vat,
            pit_rate: basis_pit,
            discount: item.discount,
            warehouse_code: item.warehouse_code.clone(),
            line_name: item.line_name.clone(),
        });
    }

    // Thiếu tồn không chặn ở đây (xem `PreparedInvoice.shortage`): nháp được lưu
    // dù thiếu — cánh cửa chặn là bước ghi số HĐĐT (`link_hddt_core`) và lúc lập
    // phiếu xuất, khi hóa đơn đã thành thật.
    Ok(PreparedInvoice {
        total,
        tax_payable,
        lines,
        shortage,
    })
}

/// Ghi dòng hóa đơn đã chuẩn bị vào bảng `invoice_item`.
async fn insert_invoice_items(
    tx: &mut SqliteTransaction<'_>,
    invoice_id: i64,
    lines: &[PreparedLine],
) -> Result<(), String> {
    for l in lines {
        sqlx::query!(
            "INSERT INTO invoice_item (invoice_id, product_id, quantity, unit_price, subtotal,
                                       industry_code, vat_rate, pit_rate, discount, warehouse_code,
                                       line_name)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            invoice_id,
            l.product_id,
            l.quantity,
            l.unit_price,
            l.subtotal,
            l.industry_code,
            l.vat_rate,
            l.pit_rate,
            l.discount,
            l.warehouse_code,
            l.line_name
        )
        .execute(&mut **tx)
        .await
        .map_err(|e| e.to_string())?;
        remember_sale_price(tx, l.product_id, l.unit_price).await?;
    }
    Ok(())
}

/// Ghi giá bán vừa lập lên mặt hàng làm GIÁ BÁN HIỆN TẠI — lần sau chọn mặt
/// hàng trên form bán là tự lấy đúng giá này. Giá 0 (khuyến mại / dòng thiếu
/// giá) không xoá giá đã lưu.
async fn remember_sale_price(
    tx: &mut SqliteTransaction<'_>,
    product_id: i64,
    unit_price: f64,
) -> Result<(), String> {
    if unit_price > 0.0 {
        sqlx::query!(
            "UPDATE product SET sale_price = ? WHERE id = ?",
            unit_price,
            product_id
        )
        .execute(&mut **tx)
        .await
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Nghiệp vụ lập hóa đơn nháp — tách riêng để test trực tiếp (không cần tauri::State).
/// Số hóa đơn đã tồn tại chưa (không phân biệt hoa/thường).
///
/// `except_id` = hóa đơn đang sửa thì bỏ qua chính nó (đổi số sang số khác vẫn
/// hợp lệ, giữ nguyên số cũ cũng hợp lệ).
async fn invoice_number_taken(
    tx: &mut SqliteTransaction<'_>,
    number: &str,
    except_id: Option<i64>,
) -> Result<bool, String> {
    let dup: Option<i64> = sqlx::query_scalar!(
        "SELECT id FROM invoice WHERE number = ? COLLATE NOCASE AND (? IS NULL OR id <> ?)",
        number,
        except_id,
        except_id
    )
    .fetch_optional(&mut **tx)
    .await
    .map_err(|e| e.to_string())?;
    Ok(dup.is_some())
}

async fn save_invoice_core(
    pool: &SqlitePool,
    number: &str,
    date: &str,
    customer: &str,
    customer_tax_code: &str,
    items: &[InvoiceItemInput],
) -> Result<serde_json::Value, String> {
    let number = number.trim();
    if number.is_empty() {
        return Err("Cần nhập số hóa đơn".into());
    }

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    // Số hóa đơn là định danh của chứng từ — trùng số là dữ liệu sai (bấm Lập
    // nhiều lần, hay nhập tay số đã có) nên chặn trước khi ghi.
    if invoice_number_taken(&mut tx, number, None).await? {
        return Err(format!("Số hóa đơn {number} đã tồn tại"));
    }
    let prepared = prepare_invoice(&mut tx, items, None).await?;

    // `vat_amount` luôn 0: hộ không tách thuế trên hóa đơn (tính cuối kỳ).
    let res = sqlx::query!(
        "INSERT INTO invoice (number, date, customer, customer_tax_code, total, vat_amount, status)
         VALUES (?, ?, ?, ?, ?, 0.0, 'draft')",
        number,
        date,
        customer,
        customer_tax_code,
        prepared.total
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    let invoice_id = res.last_insert_rowid();
    insert_invoice_items(&mut tx, invoice_id, &prepared.lines).await?;

    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "ok": true,
        "invoice_id": invoice_id,
        "total": round2(prepared.total),
        "vat_amount": 0.0,
        "tax_payable": round2(prepared.tax_payable),
        // Nháp vẫn ghi được khi thiếu tồn — app báo danh sách này cho người dùng.
        "shortage": prepared.shortage,
    }))
}

/// Sửa hóa đơn NHÁP: thay toàn bộ dòng hàng và cập nhật thông tin chung.
///
/// Chỉ nháp được sửa — đã chép sang bên kia (`exported`) hoặc đã phát hành thì
/// phải đi luồng hủy / điều chỉnh để giữ dấu vết đối chiếu.
async fn update_invoice_core(
    pool: &SqlitePool,
    id: i64,
    number: &str,
    date: &str,
    customer: &str,
    customer_tax_code: &str,
    items: &[InvoiceItemInput],
) -> Result<serde_json::Value, String> {
    let inv = sqlx::query!("SELECT number, status FROM invoice WHERE id = ?", id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Không tìm thấy hóa đơn".to_string())?;
    if inv.status != "draft" {
        return Err(format!(
            "Chỉ sửa được hóa đơn nháp — {} đang ở trạng thái đã xử lý",
            inv.number
        ));
    }
    let number = number.trim();
    if number.is_empty() {
        return Err("Cần nhập số hóa đơn".into());
    }

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    // Đổi sang số đã có hóa đơn khác thì phải báo, không cho ghi đè vô hình.
    if invoice_number_taken(&mut tx, number, Some(id)).await? {
        return Err(format!("Số hóa đơn {number} đã tồn tại"));
    }
    let prepared = prepare_invoice(&mut tx, items, Some(id)).await?;
    sqlx::query!(
        "UPDATE invoice SET number = ?, date = ?, customer = ?, customer_tax_code = ?, total = ?
          WHERE id = ?",
        number,
        date,
        customer,
        customer_tax_code,
        prepared.total,
        id
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    // Thay trọn danh sách dòng: bỏ dòng cũ rồi ghi lại (hóa đơn bán không trừ
    // kho/ghi bút toán nên không cần điều chỉnh gì khác).
    sqlx::query!("DELETE FROM invoice_item WHERE invoice_id = ?", id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    insert_invoice_items(&mut tx, id, &prepared.lines).await?;
    tx.commit().await.map_err(|e| e.to_string())?;

    Ok(serde_json::json!({
        "ok": true,
        "invoice_id": id,
        "total": round2(prepared.total),
        "vat_amount": 0.0,
        "tax_payable": round2(prepared.tax_payable),
        "shortage": prepared.shortage,
    }))
}

#[tauri::command]
pub(crate) async fn update_invoice(
    state: State<'_, AppState>,
    id: i64,
    number: String,
    date: String,
    customer: String,
    customer_tax_code: String,
    items: Vec<InvoiceItemInput>,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let res = update_invoice_core(
        &*state.pool.read().await,
        id,
        &number,
        &date,
        &customer,
        &customer_tax_code,
        &items,
    )
    .await?;
    audit(&state, "update", "invoice", &number).await;
    Ok(res.to_string())
}

/// "Đo trước" tồn kho cho form hóa đơn nháp — chạy đúng `prepare_invoice`
/// (kể cả giả lập trừ kho của các nháp khác) nhưng KHÔNG ghi gì: app hỏi người
/// dùng "thiếu kho, vẫn lưu nháp?" với số liệu chính xác trước khi chốt.
/// `except_id` = nháp đang sửa thì bỏ qua chính nó.
#[tauri::command]
pub(crate) async fn check_invoice_stock(
    state: State<'_, AppState>,
    items: Vec<InvoiceItemInput>,
    except_id: Option<i64>,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let pool = state.pool.read().await;
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let prepared = prepare_invoice(&mut tx, &items, except_id).await?;
    // Chỉ đọc — transaction bỏ đi, không cần commit.
    Ok(serde_json::json!({ "shortage": prepared.shortage }).to_string())
}

/// Cảnh báo khi hóa đơn nằm trong khoảng kỳ tờ khai đã được đánh dấu ĐÃ NỘP
/// (bấm trên màn Tờ khai). App không liên kết hệ thống thuế nên vẫn cho sửa —
/// chỉ nhắc để người dùng tự kiểm tra, kê khai bổ sung khi số liệu đã gửi bị
/// lệch so với bản ghi trong app.
pub(crate) async fn tax_period_closed_warning(
    pool: &SqlitePool,
    date: &str,
) -> Result<Option<String>, String> {
    let date = date.trim();
    if date.is_empty() {
        return Ok(None);
    }
    let row = sqlx::query!(
        r#"SELECT from_date, to_date, closed_at FROM tax_period_closed
            WHERE ? >= from_date AND ? <= to_date
            ORDER BY id DESC LIMIT 1"#,
        date,
        date
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(row.map(|r| {
        format!(
            "Kỳ tờ khai {} → {} đã đánh dấu ĐÃ NỘP ({}) — app không liên kết hệ thống thuế nên vẫn cho sửa, nhưng số liệu đã gửi có thể lệch; hãy tự kiểm tra và kê khai bổ sung nếu cần.",
            r.from_date, r.to_date, r.closed_at
        )
    }))
}

/// Sửa thông tin NGOÀI SỔ của hóa đơn đã xử lý (đã chép / đã phát hành / đã bị
/// bên kia điều chỉnh) cho khớp hóa đơn chính thức ở hệ thống ký bên kia.
///
/// App liên kết với hệ thống ký HĐĐT hoàn toàn thủ công, nên bản ghi local có
/// thể lệch hóa đơn thật (số hiệu, ngày, khách hàng, MST, số/ký hiệu HĐĐT) —
/// khi đó phải sửa được tại chỗ để sổ và tờ khai đối chiếu được. Không đụng
/// dòng hàng, bút toán hay phiếu xuất:
/// * có phiếu xuất mà đổi ngày sang KỲ KHÁC → chặn (doanh thu đang kê ở kỳ cũ,
///   muốn dời kỳ phải lập phiếu điều chỉnh);
/// * chưa có phiếu xuất → ngày đổi tự do (chưa có bút toán nào chịu ngày này,
///   cùng nguyên tắc với `link_hddt_core`);
/// * nháp → đi đường Sửa hóa đơn nháp; đã bị thay thế/hủy → không sửa được.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn update_invoice_header_core(
    pool: &SqlitePool,
    id: i64,
    number: &str,
    date: &str,
    customer: &str,
    customer_tax_code: &str,
    e_invoice_no: &str,
    e_invoice_symbol: &str,
    e_invoice_date: &str,
    actor: &str,
) -> Result<serde_json::Value, String> {
    let old = sqlx::query!(
        r#"SELECT number, date, customer, customer_tax_code, status, voucher_no,
                  e_invoice_no, e_invoice_symbol, e_invoice_date
             FROM invoice WHERE id = ?"#,
        id
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "Không tìm thấy hóa đơn".to_string())?;
    match old.status.as_str() {
        "exported" | "official" | "adjusted" => {}
        "draft" => {
            return Err(format!(
                "Hóa đơn {} còn là nháp — dùng Sửa hóa đơn nháp để sửa",
                old.number
            ));
        }
        other => {
            return Err(format!(
                "Hóa đơn {} đang ở trạng thái '{other}' — không sửa được",
                old.number
            ));
        }
    }
    let number = number.trim();
    if number.is_empty() {
        return Err("Cần nhập số hóa đơn".into());
    }
    let date = date.trim();
    if date.is_empty() {
        return Err("Cần nhập ngày hóa đơn".into());
    }
    let voucher_no = old.voucher_no.trim();
    let date_changed = date != old.date.as_str();
    if date_changed && !voucher_no.is_empty() && !same_period(&old.date, date) {
        return Err(format!(
            "Phiếu xuất {voucher_no} đang ghi ngày {} — doanh thu đã kê ở kỳ của ngày này. Muốn dời sang kỳ khác phải lập phiếu điều chỉnh, không đổi ngày tại chỗ.",
            old.date
        ));
    }

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    if invoice_number_taken(&mut tx, number, Some(id)).await? {
        return Err(format!("Số hóa đơn {number} đã tồn tại"));
    }
    sqlx::query!(
        r#"UPDATE invoice
              SET number = ?, date = ?, customer = ?, customer_tax_code = ?,
                  e_invoice_no = ?, e_invoice_symbol = ?, e_invoice_date = ?
            WHERE id = ?"#,
        number,
        date,
        customer,
        customer_tax_code,
        e_invoice_no,
        e_invoice_symbol,
        e_invoice_date,
        id
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;

    // Chỉ ghi vào sổ sự kiện những trường THẬT SỰ đổi — đủ đối soát mà không
    // phết nhật ký vô nghĩa khi bấm Lưu mà không đổi gì.
    let mut changes: Vec<String> = Vec::new();
    {
        let mut push = |label: &str, before: &str, after: &str| {
            if before != after {
                changes.push(format!("{label}: '{before}' → '{after}'"));
            }
        };
        push("Số HĐ", &old.number, number);
        push("Ngày", &old.date, date);
        push("Khách hàng", &old.customer, customer);
        push("MST", &old.customer_tax_code, customer_tax_code);
        push("Số HĐĐT", &old.e_invoice_no, e_invoice_no);
        push("Ký hiệu", &old.e_invoice_symbol, e_invoice_symbol);
        push("Ngày HĐĐT", &old.e_invoice_date, e_invoice_date);
    }
    if changes.is_empty() {
        return Err("Không có gì để đổi — dữ liệu không thay đổi".into());
    }
    let reason = format!("Sửa theo HĐ chính thức — {}", changes.join("; "));
    sqlx::query!(
        r#"INSERT INTO invoice_event (invoice_id, from_status, to_status, reason, actor, ref)
           VALUES (?, ?, ?, ?, ?, ?)"#,
        id,
        old.status,
        old.status,
        reason,
        actor,
        e_invoice_no,
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;

    // Cùng kỳ nhưng ngày phiếu giữ nguyên → doanh thu vẫn kê ở ngày phiếu, chỉ
    // ngày hóa đơn đổi. Báo để người dùng đối chiếu sổ.
    let mut warning = if date_changed && !voucher_no.is_empty() {
        format!(
            "Phiếu xuất {voucher_no} giữ ngày {} — doanh thu vẫn kê ở kỳ của {}; ngày hóa đơn sửa thành {date} vì theo HĐ chính thức.",
            old.date, old.date
        )
    } else {
        String::new()
    };
    if let Some(w) = tax_period_closed_warning(pool, date).await? {
        if warning.is_empty() {
            warning = w;
        } else {
            warning = format!("{warning} {w}");
        }
    }
    Ok(json!({
        "ok": true,
        "warning": warning,
        "changes": changes,
        "old_date": old.date,
        "date": date,
    }))
}

/// Lệnh giao diện: sửa thông tin hồ sơ của hóa đơn đã xử lý (khớp HĐ chính thức).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn update_invoice_header(
    state: State<'_, AppState>,
    id: i64,
    number: String,
    date: String,
    customer: String,
    customer_tax_code: String,
    e_invoice_no: String,
    e_invoice_symbol: String,
    e_invoice_date: String,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let actor = state
        .current_user
        .lock()
        .await
        .as_ref()
        .map(|u| u.username.clone())
        .unwrap_or_default();
    let res = update_invoice_header_core(
        &*state.pool.read().await,
        id,
        &number,
        &date,
        &customer,
        &customer_tax_code,
        &e_invoice_no,
        &e_invoice_symbol,
        &e_invoice_date,
        &actor,
    )
    .await?;
    audit(&state, "update_header", "invoice", &number).await;
    Ok(res.to_string())
}

/// Sửa DÒNG HÀNG của hóa đơn đã xử lý (đã chép / đã phát hành / đã bị bên kia
/// điều chỉnh) cho khớp hóa đơn chính thức. Đối lập với `update_invoice_header`
/// (chỉ sửa ngoài sổ), lệnh này ĐỤNG tới sổ và phải tự cân sách con:
///
/// * **Có phiếu xuất** → ghi thêm chứng từ `{số}DT`: đảo MỌI dòng doanh thu còn
///   hiệu lực trên phiếu gốc + các lần sửa trước (dòng GiamDT thì đảo ngược
///   lại) rồi ghi lại toàn bộ dòng mới. Tồn kho + giá vốn bù theo CHÊNH LỆCH số
///   lượng từng mã/kho: tăng → FIFO xuất thêm (thiếu kho thì CHẶN để đi nhập
///   hàng hoặc lập phiếu điều chỉnh — không tự bù ngược giá vốn), giảm → nhập
///   lại lô mới đúng theo đơn giá vốn đã ghi ở phiếu gốc.
/// * **Chưa có phiếu xuất** → chưa có bút toán nào, chỉ thay dòng + tổng.
///
/// Nhóm ngành / tỷ lệ thuế lấy lại từ nhóm ngành (như lúc lập phiếu); dòng mới
/// ghi Nợ 131 / Có 511 như bán thường → tờ khai vẫn tự tổng hợp đúng (tăng −
/// giảm = tổng mới).
pub(crate) async fn update_invoice_lines_core(
    pool: &SqlitePool,
    id: i64,
    items: &[InvoiceItemInput],
    actor: &str,
) -> Result<serde_json::Value, String> {
    let inv = sqlx::query!(
        r#"SELECT number, e_invoice_no, date, status, voucher_no, total, customer, customer_tax_code
             FROM invoice WHERE id = ?"#,
        id
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "Không tìm thấy hóa đơn".to_string())?;
    // Sổ phải chỉ tới số hóa đơn đã phát hành khi có; số nội bộ chỉ dùng cho
    // hóa đơn chưa có số HĐĐT.
    let display_number = if inv.e_invoice_no.trim().is_empty() {
        inv.number.as_str()
    } else {
        inv.e_invoice_no.trim()
    };
    match inv.status.as_str() {
        "exported" | "official" | "adjusted" => {}
        "draft" => {
            return Err(format!(
                "Hóa đơn {} còn là nháp — dùng Sửa hóa đơn nháp",
                inv.number
            ));
        }
        other => {
            return Err(format!(
                "Hóa đơn {} đang ở trạng thái '{other}' — không sửa được",
                inv.number
            ));
        }
    }

    // Kỳ tờ khai đã đánh dấu ĐÃ NỘP? Vẫn cho sửa (app không liên kết CQT) nhưng
    // phải nhắc để người dùng tự kê khai bổ sung nếu số liệu đã gửi bị lệch.
    let period_warning = tax_period_closed_warning(pool, &inv.date)
        .await?
        .unwrap_or_default();

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    // Chuẩn bị dòng mới: kiểm tra nhóm ngành, tính lại thành tiền / tổng.
    // (Thiếu tồn ở bước này KHÔNG dùng — sách đã trừ kho từ lúc lập phiếu,
    // soi tổng sẽ sai; delta được tính riêng bên dưới theo từng mã/kho.)
    let prepared = prepare_invoice(&mut tx, items, Some(id)).await?;

    let voucher_dt = if inv.voucher_no.trim().is_empty() {
        String::new()
    } else {
        format!("{}DT", inv.voucher_no.trim())
    };
    let mut booked = 0usize;
    let mut reason_extra = String::new();

    if !voucher_dt.is_empty() {
        // ── Sách con: có phiếu xuất ───────────────────────────────────────
        // Mã khách cho bút toán: ưu tiên MST, không có thì theo tên — như lúc
        // lập phiếu xuất từ hóa đơn.
        let mut customer_code = if inv.customer_tax_code.trim().is_empty() {
            String::new()
        } else {
            sqlx::query_scalar!(
                "SELECT code FROM customer WHERE tax_code = ?",
                inv.customer_tax_code
            )
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| e.to_string())?
            .unwrap_or_default()
        };
        if customer_code.is_empty() {
            customer_code =
                sqlx::query_scalar!("SELECT code FROM customer WHERE name = ?", inv.customer)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(|e| e.to_string())?
                    .unwrap_or_default();
        }

        // (mã hàng, cờ dịch vụ) — nạp một lần cho mọi thao tác phía sau.
        let product_rows =
            sqlx::query!("SELECT id, code, is_service as \"is_service: bool\" FROM product")
                .fetch_all(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
        let products: HashMap<i64, (String, bool)> = product_rows
            .into_iter()
            .map(|r| (r.id, (r.code, r.is_service)))
            .collect();

        // ── 1) Đảo doanh thu ── MỌI dòng còn hiệu lực trên phiếu gốc + phiếu
        // {v}DT (lần sửa trước ghi ở đó) → sổ về 0 trước khi ghi lại.
        let active = sqlx::query!(
            r#"SELECT quantity, unit_price, amount, product_code, industry_code,
                      vat_rate, pit_rate, discount, adjust_code
                 FROM journal_entry
                WHERE voucher_no IN (?, ?) AND entry_type = 'PX'
                  AND (credit_account = '511'
                       OR (debit_account = '511' AND adjust_code = 'GiamDT'))"#,
            inv.voucher_no,
            voucher_dt
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        let desc_rev = format!("Đảo doanh thu — sửa hóa đơn {display_number} theo HĐ chính thức");
        for row in &active {
            // Dòng tăng → đảo thành giảm; dòng GiamDT cũ → đảo ngược lại (tăng
            // lại) để hiệu lực đúng bằng 0, giữ nguyên sản phẩm / nhóm ngành /
            // tỷ lệ thuế của từng dòng (tờ khai phân nhóm không bị lệch).
            let (debit, credit, adjust) = if row.adjust_code == "GiamDT" {
                ("131", "511", "")
            } else {
                ("511", "131", "GiamDT")
            };
            insert_journal_entry(
                &mut tx,
                &inv.date,
                &voucher_dt,
                "PX",
                &desc_rev,
                &row.product_code,
                "",
                &customer_code,
                row.quantity,
                row.unit_price,
                row.amount,
                debit,
                credit,
                &row.industry_code,
                row.vat_rate,
                row.pit_rate,
                "HKD",
                adjust,
                "",
                row.discount,
            )
            .await?;
            booked += 1;
        }

        // ── 2) Ghi lại dòng doanh thu theo HĐ chính thức (giống bán thường).
        let desc_new = format!("Ghi lại doanh thu theo HĐ chính thức — hóa đơn {display_number}");
        let mut new_qty: HashMap<(i64, i64), f64> = HashMap::new();
        let mut new_industry: HashMap<(i64, i64), String> = HashMap::new();
        for line in &prepared.lines {
            let (code, _) = products
                .get(&line.product_id)
                .ok_or_else(|| "Sản phẩm trên dòng không tồn tại".to_string())?;
            let wh = resolve_warehouse(&mut tx, &line.warehouse_code, code).await?;
            let amount = round2(line.quantity * line.unit_price - line.discount);
            insert_journal_entry(
                &mut tx,
                &inv.date,
                &voucher_dt,
                "PX",
                &desc_new,
                code,
                "",
                &customer_code,
                line.quantity,
                line.unit_price,
                amount,
                "131",
                "511",
                &line.industry_code,
                line.vat_rate,
                line.pit_rate,
                "HKD",
                "",
                "",
                line.discount,
            )
            .await?;
            booked += 1;
            *new_qty.entry((line.product_id, wh)).or_default() += line.quantity;
            new_industry.insert((line.product_id, wh), line.industry_code.clone());
        }

        // ── 3) Tồn kho + giá vốn theo CHÊNH LỆCH từng mã/kho ──
        let old_rows = sqlx::query!(
            r#"SELECT ii.product_id, p.code, ii.quantity, ii.warehouse_code, ii.industry_code
                 FROM invoice_item ii JOIN product p ON p.id = ii.product_id
                WHERE ii.invoice_id = ? ORDER BY ii.id"#,
            id
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        let mut old_qty: HashMap<(i64, i64), f64> = HashMap::new();
        let mut old_industry: HashMap<(i64, i64), String> = HashMap::new();
        for row in &old_rows {
            let wh = resolve_warehouse(&mut tx, &row.warehouse_code, &row.code).await?;
            *old_qty.entry((row.product_id, wh)).or_default() += row.quantity;
            old_industry.insert((row.product_id, wh), row.industry_code.clone());
        }
        let mut deltas: HashMap<(i64, i64), f64> = HashMap::new();
        for (k, q) in &new_qty {
            *deltas.entry(*k).or_default() += q;
        }
        for (k, q) in &old_qty {
            *deltas.entry(*k).or_default() -= q;
        }

        let desc_stock = format!(
            "Bù tồn kho / giá vốn — sửa hóa đơn {} theo HĐ chính thức",
            inv.number
        );
        for ((pid, wh), dq) in deltas {
            if dq.abs() < 1e-9 {
                continue;
            }
            let (code, is_service) = products
                .get(&pid)
                .cloned()
                .ok_or_else(|| "Sản phẩm trên dòng không tồn tại".to_string())?;
            if is_service {
                continue; // dịch vụ không theo dõi tồn kho / giá vốn
            }
            let industry = new_industry
                .get(&(pid, wh))
                .or_else(|| old_industry.get(&(pid, wh)))
                .cloned()
                .unwrap_or_default();
            if dq > 0.0 {
                // Tăng SL → FIFO xuất thêm đúng kho đó; thiếu thì chặn (đi nhập
                // hàng rồi sửa lại, hoặc lập phiếu điều chỉnh trên màn Tồn kho).
                let lots: Vec<(i64, f64, f64)> = sqlx::query!(
                    r#"SELECT id as "id!", quantity, unit_cost FROM stock_lot
                        WHERE product_id = ? AND warehouse_id = ? AND depleted = 0
                        ORDER BY received_at ASC, id ASC"#,
                    pid,
                    wh
                )
                .fetch_all(&mut *tx)
                .await
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|r| (r.id, r.quantity, r.unit_cost))
                .collect();
                let (cogs, takes) =
                    crate::commands::stock::allocate_fifo(&lots, dq).map_err(|short| {
                        format!(
                            "Sửa làm thiếu tồn kho '{code}': thiếu {short:.2} (các lô trong kho đã xuất hết). Nhập thêm hàng rồi sửa lại, hoặc dùng phiếu điều chỉnh trên màn Tồn kho."
                        )
                    })?;
                for t in &takes {
                    sqlx::query!(
                        "UPDATE stock_lot SET quantity = ?, depleted = ? WHERE id = ?",
                        t.new_qty,
                        t.depleted,
                        t.lot_id
                    )
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| e.to_string())?;
                }
                if cogs > 0.0 {
                    insert_journal_entry(
                        &mut tx,
                        &inv.date,
                        &voucher_dt,
                        "CP",
                        &desc_stock,
                        &code,
                        "",
                        &customer_code,
                        dq,
                        cogs / dq,
                        round2(cogs),
                        "632",
                        "152",
                        &industry,
                        0.0,
                        0.0,
                        "HKD",
                        "",
                        "",
                        0.0,
                    )
                    .await?;
                    booked += 1;
                }
            } else {
                // Giảm SL → nhập lại lô đúng theo ĐƠN GIÁ VỐN đã ghi ở phiếu
                // gốc (giữ giá vốn không lệch — không tự suy diễn giá khác).
                let back = -dq;
                let (sum_amt, sum_qty): (f64, f64) = sqlx::query_as::<_, (f64, f64)>(
                    r#"SELECT COALESCE(SUM(amount), 0.0), COALESCE(SUM(quantity), 0.0)
                         FROM journal_entry
                        WHERE voucher_no = ? AND entry_type = 'CP' AND product_code = ?"#,
                )
                .bind(&inv.voucher_no)
                .bind(&code)
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
                let unit_cost = if sum_qty > 1e-9 {
                    sum_amt / sum_qty
                } else {
                    0.0
                };
                sqlx::query!(
                    r#"INSERT INTO stock_lot
                          (product_id, warehouse_id, quantity, unit_cost, received_at, depleted)
                       VALUES (?, ?, ?, ?, ?, 0)"#,
                    pid,
                    wh,
                    back,
                    unit_cost,
                    inv.date
                )
                .execute(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
                insert_journal_entry(
                    &mut tx,
                    &inv.date,
                    &voucher_dt,
                    "CP",
                    &desc_stock,
                    &code,
                    "",
                    &customer_code,
                    back,
                    unit_cost,
                    round2(back * unit_cost),
                    "152",
                    "632",
                    &industry,
                    0.0,
                    0.0,
                    "HKD",
                    "",
                    "",
                    0.0,
                )
                .await?;
                booked += 1;
            }
        }
        reason_extra = format!("; lập phiếu {voucher_dt}");
    }

    // ── Cập nhật hóa đơn + thay toàn bộ dòng (cả hai nhánh) ──
    sqlx::query!(
        "UPDATE invoice SET total = ?, vat_amount = 0.0 WHERE id = ?",
        prepared.total,
        id
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    sqlx::query!("DELETE FROM invoice_item WHERE invoice_id = ?", id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    insert_invoice_items(&mut tx, id, &prepared.lines).await?;

    let reason = format!(
        "Sửa dòng hàng theo HĐ chính thức — {} dòng; doanh thu {} → {}{}",
        prepared.lines.len(),
        round2(inv.total),
        round2(prepared.total),
        reason_extra
    );
    sqlx::query!(
        r#"INSERT INTO invoice_event (invoice_id, from_status, to_status, reason, actor, ref)
           VALUES (?, ?, ?, ?, ?, '')"#,
        id,
        inv.status,
        inv.status,
        reason,
        actor,
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;

    Ok(json!({
        "ok": true,
        "number": inv.number,
        "old_total": round2(inv.total),
        "new_total": round2(prepared.total),
        "voucher_dt": voucher_dt,
        "booked": booked,
        "warning": period_warning,
    }))
}

/// Lệnh giao diện: sửa dòng hàng hóa đơn đã xử lý (khớp HĐ chính thức).
#[tauri::command]
pub(crate) async fn update_invoice_lines(
    state: State<'_, AppState>,
    id: i64,
    items: Vec<InvoiceItemInput>,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let actor = state
        .current_user
        .lock()
        .await
        .as_ref()
        .map(|u| u.username.clone())
        .unwrap_or_default();
    let res = update_invoice_lines_core(&*state.pool.read().await, id, &items, &actor).await?;
    let number = res["number"].as_str().unwrap_or_default().to_string();
    audit(&state, "update_lines", "invoice", &number).await;
    Ok(res.to_string())
}

/// Trần số hổng liệt kê cho mỗi dãy: hộ nhỏ hổng vài chục số đã là nhiều, còn
/// một số dài bất thường (do nhập nhầm, ví dụ HD99999999) sẽ đòi liệt kê hàng
/// chục triệu số — tràn bộ nhớ ngay khi mở báo cáo thiếu số hiệu. Chạm trần thì
/// dừng; báo cáo vẫn có ích.
const MAX_MISSING_NUMBERS: usize = 1000;

/// Các số bị hổng GIỮA hai số đã có trong một dãy đã sắp xếp (tối đa
/// [`MAX_MISSING_NUMBERS`] số).
fn number_gaps(sorted: &[i64]) -> Vec<i64> {
    let mut out = Vec::new();
    'outer: for w in sorted.windows(2) {
        for n in w[0].saturating_add(1)..w[1] {
            out.push(n);
            if out.len() >= MAX_MISSING_NUMBERS {
                break 'outer;
            }
        }
    }
    out
}

/// Báo cáo số hiệu bị HỔNG giữa dãy: số nội bộ HD#### và số HĐĐT chính thức
/// (gom theo từng ký hiệu). Nhập trễ / quên một số giữa chừng → hiện ngay để
/// đối chiếu với hệ thống ký bên kia. Chỉ báo các số NẰM GIỮA hai số đã có
/// (biên min..max) — số ngoài biên chưa chắc đã tới lượt.
pub(crate) async fn invoice_missing_numbers_core(
    pool: &SqlitePool,
) -> Result<serde_json::Value, String> {
    let rows = sqlx::query!("SELECT number, e_invoice_no, e_invoice_symbol FROM invoice")
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

    // (a) Số nội bộ: HD + chữ số (app sinh theo dãy; số lạ không theo mẫu bỏ qua).
    let mut internal: Vec<i64> = Vec::new();
    for r in &rows {
        if let Some(rest) = r.number.strip_prefix("HD") {
            if let Ok(n) = rest.parse::<i64>() {
                internal.push(n);
            }
        }
    }
    internal.sort_unstable();
    internal.dedup();
    let internal_missing: Vec<String> = number_gaps(&internal)
        .iter()
        .map(|n| format!("HD{n:04}"))
        .collect();

    // (b) Số HĐĐT chính thức — theo từng ký hiệu (chưa liên kết/không ghi số thì
    // bỏ: chưa đủ thông tin để nói là "thiếu"). Giữ đúng độ dài số đã ghi
    // (00000089 → 00000090) để đối chiếu y hệt với hệ thống bên kia.
    let mut by_symbol: std::collections::BTreeMap<String, Vec<(i64, usize)>> =
        std::collections::BTreeMap::new();
    for r in &rows {
        let symbol = r.e_invoice_symbol.trim();
        let no = r.e_invoice_no.trim();
        if symbol.is_empty() || no.is_empty() {
            continue;
        }
        if let Ok(n) = no.parse::<i64>() {
            by_symbol
                .entry(symbol.to_string())
                .or_default()
                .push((n, no.len()));
        }
    }
    let mut official = Vec::new();
    for (symbol, mut list) in by_symbol {
        list.sort_unstable();
        let nums: Vec<i64> = list.iter().map(|(n, _)| *n).collect();
        let mut dedup = nums;
        dedup.dedup();
        let width = list.iter().map(|(_, w)| *w).max().unwrap_or(0);
        let missing: Vec<String> = number_gaps(&dedup)
            .iter()
            .map(|n| format!("{n:0width$}", width = width))
            .collect();
        if !missing.is_empty() {
            official.push(json!({
                "symbol": symbol,
                "present": dedup.len(),
                "missing": missing,
            }));
        }
    }

    Ok(json!({
        "internal_missing": internal_missing,
        "official": official,
    }))
}

/// Lệnh giao diện: báo cáo số hiệu hổng giữa dãy (nhập trễ / quên số).
#[tauri::command]
pub(crate) async fn invoice_missing_numbers(state: State<'_, AppState>) -> Result<String, String> {
    invoice_missing_numbers_core(&*state.pool.read().await)
        .await
        .map(|v| v.to_string())
}

/// Tạo hóa đơn nháp — thiếu tồn vẫn ghi được (trả danh sách `shortage` để app
/// hỏi người dùng); cánh cửa chặn là ghi số HĐĐT và lập phiếu xuất.
#[tauri::command]
pub(crate) async fn save_invoice(
    state: State<'_, AppState>,
    number: String,
    date: String,
    customer: String,
    customer_tax_code: String,
    items: Vec<InvoiceItemInput>,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let pool = state.pool.read().await;
    let res =
        save_invoice_core(&pool, &number, &date, &customer, &customer_tax_code, &items).await?;
    audit(&state, "save", "invoice", &number).await;
    Ok(res.to_string())
}

/// Xoá hóa đơn NHÁP. An toàn về sổ sách: hóa đơn bán chỉ ghi bảng `invoice` +
/// `invoice_item` (không trừ kho, không ghi bút toán — phiếu xuất là chứng tử riêng),
/// nên xoá nháp không đụng tồn kho hay kế toán.
///
/// Chỉ xoá được `draft`: đã chép sang bên kia (`exported`) hoặc đã phát hành
/// (`official`/`adjusted`) thì phải đi theo luồng hủy / điều chỉnh để còn dấu vết.
async fn delete_invoice_core(pool: &SqlitePool, id: i64) -> Result<String, String> {
    let inv = sqlx::query!(
        "SELECT number, status, e_invoice_no FROM invoice WHERE id = ?",
        id
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "Không tìm thấy hóa đơn".to_string())?;
    if inv.status != "draft" {
        return Err(format!(
            "Chỉ xoá được hóa đơn nháp — {} đang ở trạng thái đã xử lý. Hãy dùng 'ghi nhận hủy' hoặc 'bị sửa bên kia' để giữ dấu vết đối chiếu",
            inv.number
        ));
    }
    if !inv.e_invoice_no.trim().is_empty() {
        return Err(format!(
            "{} đã có số HĐĐT {} — không xoá được",
            inv.number, inv.e_invoice_no
        ));
    }

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    // Nháp chưa từng chép/hủy nên 2 bảng này thường rỗng; xoá phòng khi có để không
    // còn dòng mồ côi.
    sqlx::query!("DELETE FROM invoice_export WHERE invoice_id = ?", id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query!("DELETE FROM invoice_event WHERE invoice_id = ?", id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query!("DELETE FROM invoice_item WHERE invoice_id = ?", id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query!("DELETE FROM invoice WHERE id = ?", id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(inv.number)
}

/// Một số hóa đơn bị dùng cho nhiều bản ghi (sinh ra khi bấm Lập nhiều lần).
#[derive(sqlx::FromRow, serde::Serialize)]
pub(crate) struct DuplicateNumber {
    pub(crate) number: String,
    pub(crate) count: i64,
}

/// Báo cáo số hóa đơn trùng — app KHÔNG tự sửa dữ liệu, chỉ để người dùng tự
/// dọn: xoá bản nháp thừa, còn bản đã phát hành thì xử lý bằng thay thế.
///
/// Số hóa đơn là định danh chứng từ, trùng số làm doanh thu/tờ khai sai và
/// khó đối chiếu với bên cổng thuế.
pub(crate) async fn duplicate_numbers_core(
    pool: &SqlitePool,
) -> Result<Vec<DuplicateNumber>, String> {
    sqlx::query_as!(
        DuplicateNumber,
        r#"SELECT number, COUNT(*) as "count!: i64"
             FROM invoice GROUP BY number HAVING COUNT(*) > 1 ORDER BY number"#
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn invoice_duplicate_numbers(
    state: State<'_, AppState>,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let rows = duplicate_numbers_core(&*state.pool.read().await).await?;
    Ok(serde_json::to_string(&rows).unwrap_or_default())
}

/// Soi tồn kho cho MỖI hóa đơn trong danh sách — màn Hóa đơn / Chờ xuất HĐĐT
/// tô màu dòng. Trả về **cả hóa đơn đủ tồn** (level `ok`) để màn hình biết chắc
/// chắn dòng đó đã soi xong mà tô tím "đủ điều kiện xuất", không đoán qua việc
/// vắng mặt trong map.
///
/// `level`:
///   • `ok`       — đủ tồn thành phẩm, có thể phát hành/ghi số.
///   • `short`    — thiếu thành phẩm nhưng (nếu là hàng chế tạo) NVL trong định
///                  mức vẫn đủ để sản xuất bù → vẫn là đỏ thường.
///   • `material` — thiếu thành phẩm **và** NVL cấu thành cũng không đủ → đỏ đậm,
///                  không thể tự sản xuất bù.
///
/// JSON `{ "<id>": {"level": "...", "lines": ["SP001: cần 5, còn 2", …]} }`.
/// Không gắn phiếu xuất thì mới kiểm — phiếu đã trừ kho rồi (cùng cửa kiểm với
/// lúc ghi số). Lỗi riêng lẻ của 1 hóa đơn không làm hỏng cả bảng (bỏ id đó).
#[tauri::command]
pub(crate) async fn invoice_shortage_map(
    state: State<'_, AppState>,
    ids: Vec<i64>,
) -> Result<String, String> {
    if ids.is_empty() {
        return Ok("{}".into());
    }
    // Chốt chặn phòng hiểm: mỗi id vài truy vấn nhỏ — đừng kéo nguyên bảng vào.
    if ids.len() > 500 {
        return Err("Quá nhiều hóa đơn cần soi tồn trong một lần".into());
    }
    let pool = state.pool.read().await;
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let mut out = serde_json::Map::new();
    for id in ids {
        let scan = match invoice_stock_scan(&mut tx, id, true).await {
            Ok(scan) => scan,
            Err(_) => continue,
        };
        out.insert(
            id.to_string(),
            json!({ "level": scan.level(), "lines": scan.lines }),
        );
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(serde_json::to_string(&serde_json::Value::Object(out)).unwrap_or_default())
}

#[tauri::command]
pub(crate) async fn delete_invoice(state: State<'_, AppState>, id: i64) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let number = delete_invoice_core(&*state.pool.read().await, id).await?;
    audit(&state, "delete", "invoice", &number).await;
    Ok("ok".into())
}

/// Ngày trên HĐĐT là ngày văn bản đã phát hành, nên khi khác ngày lập nháp thì
/// hóa đơn phải theo ngày đó. Việc này chỉ an toàn khi hóa đơn **không** gắn phiếu
/// xuất: nháp lập ở màn Hóa đơn chỉ ghi bảng invoice, sổ sách chưa ghi gì nên đổi
/// ngày không hệ quả gì.
fn same_period(a: &str, b: &str) -> bool {
    a.len() >= 7 && b.len() >= 7 && a[..7] == b[..7] // cùng tháng: yyyy-mm
}

/// Kết quả soi tồn 1 hóa đơn cho màn danh sách.
///
/// `lines` giữ nguyên câu báo thiếu thành phẩm cũ (dùng cho cả lúc ghi số HĐĐT);
/// `material_short` bật khi có dòng hàng chế tạo mà NVL trong định mức cũng
/// không đủ bù phần thiếu — màn danh sách tô đỏ đậm hơn để phân biệt với thiếu
/// tồn thường (vẫn có thể sản xuất bù từ NVL sẵn có).
#[derive(Default)]
struct InvoiceStockScan {
    lines: Vec<String>,
    material_short: bool,
}

impl InvoiceStockScan {
    /// Mức cảnh báo cho màu dòng: `ok` | `short` | `material`.
    fn level(&self) -> &'static str {
        if self.lines.is_empty() {
            "ok"
        } else if self.material_short {
            "material"
        } else {
            "short"
        }
    }
}

/// NVL cần bù cho phần thành phẩm còn thiếu của 1 hóa đơn, đã gộp theo
/// (NVL, kho) để không cộng dồn sai khi nhiều dòng cùng dùng 1 NVL.
struct MaterialNeed {
    /// Mã NVL — chỉ để in câu giải thích.
    code: String,
    /// Nhãn kho dùng trong câu giải thích ("kho đã chọn" / "tổng các kho").
    where_str: &'static str,
    /// Tổng số lượng NVL cần để sản xuất bù.
    quantity: f64,
    /// Mã các thành phẩm cần NVL này — in kèm để biết nó phục vụ dòng nào.
    products: BTreeSet<String>,
}

/// Tồn khả dụng của 1 sản phẩm: `None` = tổng mọi kho (dòng hóa đơn không khai
/// kho xuất), `Some(wh)` = đúng kho đã chọn.
async fn stock_on_hand(
    tx: &mut SqliteTransaction<'_>,
    product_id: i64,
    warehouse_id: Option<i64>,
) -> Result<f64, String> {
    match warehouse_id {
        Some(wh_id) => sqlx::query_scalar!(
            r#"SELECT COALESCE(SUM(quantity), 0.0) as "qty!: f64" FROM stock_lot
               WHERE product_id = ? AND warehouse_id = ? AND depleted = 0"#,
            product_id,
            wh_id
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| e.to_string()),
        None => sqlx::query_scalar!(
            r#"SELECT COALESCE(SUM(quantity), 0.0) as "qty!: f64" FROM stock_lot
               WHERE product_id = ? AND depleted = 0"#,
            product_id
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| e.to_string()),
    }
}

/// Soi tồn kho của TỪNG dòng hóa đơn — thiếu ở ĐÚNG kho xuất trên dòng (rỗng →
/// tổng mọi kho). Hàng dịch vụ không theo dõi tồn kho nên bỏ qua.
///
/// `with_materials = true` (màn danh sách): với hàng chế tạo, tính thêm phần
/// **NVL cần để sản xuất bù** đúng số thành phẩm còn thiếu (định mức × phần
/// thiếu), gộp mọi dòng cùng NVL/kho rồi so tồn. Đủ NVL → vẫn chỉ là `short`
/// (đỏ thường); thiếu NVL → `material` (đỏ đậm).
async fn invoice_stock_scan(
    tx: &mut SqliteTransaction<'_>,
    invoice_id: i64,
    with_materials: bool,
) -> Result<InvoiceStockScan, String> {
    let rows = sqlx::query!(
        r#"SELECT ii.product_id, p.code, p.is_service as "is_service!: bool",
                  ii.quantity, ii.warehouse_code
             FROM invoice_item ii
             JOIN product p ON p.id = ii.product_id
            WHERE ii.invoice_id = ?"#,
        invoice_id
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| e.to_string())?;

    let mut scan = InvoiceStockScan::default();
    // NVL cần bù cho phần thành phẩm thiếu — gộp theo (NVL, kho); khóa kho 0 =
    // dòng hóa đơn không khai kho nên soi tổng mọi kho.
    let mut material_needs: BTreeMap<(i64, i64), MaterialNeed> = BTreeMap::new();

    for row in rows {
        if row.is_service {
            continue;
        }
        let (warehouse_id, where_str) = if row.warehouse_code.trim().is_empty() {
            (None, "tổng các kho")
        } else {
            let wh_id = resolve_warehouse(tx, &row.warehouse_code, &row.code).await?;
            (Some(wh_id), "kho đã chọn")
        };
        let available = stock_on_hand(tx, row.product_id, warehouse_id).await?;
        let deficit = row.quantity - available;
        if deficit <= 1e-9 {
            continue;
        }
        scan.lines.push(format!(
            "{}: cần {:.2}, tồn {} {:.2}",
            row.code, row.quantity, where_str, available
        ));
        if !with_materials {
            continue;
        }

        // Định mức chỉ tính NVL thật: dịch vụ (nhân công…) không có tồn kho.
        let bom = sqlx::query!(
            r#"SELECT b.material_product_id, m.code, b.quantity
                 FROM product_bom_item b
                 JOIN product m ON m.id = b.material_product_id
                WHERE b.product_id = ? AND m.is_service = 0"#,
            row.product_id
        )
        .fetch_all(&mut **tx)
        .await
        .map_err(|e| e.to_string())?;
        for b in bom {
            // Làm tròn 6 số như lúc tự xuất NVL — tránh nhiễu float cộng dồn.
            let add = (b.quantity * deficit * 1e6).round() / 1e6;
            if add <= 0.0 {
                continue;
            }
            let entry = material_needs
                .entry((b.material_product_id, warehouse_id.unwrap_or(0)))
                .or_insert_with(|| MaterialNeed {
                    code: b.code.clone(),
                    where_str,
                    quantity: 0.0,
                    products: BTreeSet::new(),
                });
            entry.quantity += add;
            entry.products.insert(row.code.clone());
        }
    }

    // Soi từng NVL đã gộp — một NVL thiếu là cả hóa đơn không tự sản xuất bù được.
    for ((material_id, wh_key), need) in material_needs {
        let warehouse_id = (wh_key != 0).then_some(wh_key);
        let available = stock_on_hand(tx, material_id, warehouse_id).await?;
        if need.quantity - available > 1e-9 {
            scan.material_short = true;
            scan.lines.push(format!(
                "NVL {} cho {}: cần {:.2}, tồn {} {:.2}",
                need.code,
                need.products.into_iter().collect::<Vec<_>>().join(", "),
                need.quantity,
                need.where_str,
                available
            ));
        }
    }
    Ok(scan)
}

/// Danh sách lỗi thiếu tồn thành phẩm — dùng cho cửa chặn ghi số HĐĐT (không
/// cần thông tin NVL: thiếu thành phẩm là đã chặn rồi).
async fn invoice_stock_shortage(
    tx: &mut SqliteTransaction<'_>,
    invoice_id: i64,
) -> Result<Vec<String>, String> {
    Ok(invoice_stock_scan(tx, invoice_id, false).await?.lines)
}

/// Ghi nhận số HĐĐT cho hóa đơn nháp → trạng thái 'official'.
///
/// Ký hiệu gõ ở đây cũng được lưu vào hồ sơ HKD (`business.hddt_symbol`) làm
/// mặc định cho lần sau — mỗi hộ có một ký hiệu riêng và đổi mẫu số theo năm
/// không cần sửa lại từng hóa đơn.
async fn link_hddt_core(
    pool: &SqlitePool,
    invoice_id: i64,
    hddt_no: &str,
    hddt_symbol: &str,
    hddt_date: &str,
) -> Result<serde_json::Value, String> {
    let symbol = hddt_symbol.trim();
    let old = sqlx::query!(
        "SELECT number, date, voucher_no FROM invoice WHERE id = ?",
        invoice_id
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "Không tìm thấy hóa đơn".to_string())?;

    // Hóa đơn gắn phiếu xuất (lập kèm ở màn Xuất kho) thì doanh thu đã ghi sổ theo
    // NGÀY PHIẾU — đổi ngày phiếu là dời doanh thu sang kỳ khác, không tự làm.
    let date_synced =
        !hddt_date.is_empty() && hddt_date != old.date && old.voucher_no.trim().is_empty();
    let date = if date_synced {
        hddt_date
    } else {
        old.date.as_str()
    };

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    // Ghi số HĐĐT = hóa đơn đã THẬT → bắt buộc đủ tồn kho NGAY LÚC NÀY (nháp thì
    // được thiếu, app đã hỏi người dùng trước khi lưu). Hóa đơn đã gắn phiếu xuất
    // thì kho đã trừ lúc lập phiếu → bỏ qua, không kiểm nữa.
    if old.voucher_no.trim().is_empty() {
        let shortage = invoice_stock_shortage(&mut tx, invoice_id).await?;
        if !shortage.is_empty() {
            return Err(format!("Không đủ tồn kho: {}", shortage.join("; ")));
        }
    }

    sqlx::query!(
        "UPDATE invoice SET status = 'official', e_invoice_no = ?, e_invoice_symbol = ?,
                           e_invoice_date = ?, date = ?
          WHERE id = ?",
        hddt_no,
        symbol,
        hddt_date,
        date,
        invoice_id
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    // Ký hiệu rỗng thì giữ nguyên ký hiệu đang lưu trong hồ sơ.
    if !symbol.is_empty() {
        sqlx::query!("UPDATE business SET hddt_symbol = ? WHERE id = 1", symbol)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
    }
    let reason = match (date_synced, old.voucher_no.trim().is_empty()) {
        (true, _) => format!(
            "Ghi nhận số HĐĐT {symbol}/{hddt_no} ngày {hddt_date}; ngày hóa đơn {} → {hddt_date}",
            old.date
        ),
        (false, false) if !hddt_date.is_empty() && hddt_date != old.date => format!(
            "Ghi nhận số HĐĐT {symbol}/{hddt_no} ngày {hddt_date}; giữ ngày hóa đơn {} vì đang gắn phiếu xuất {}",
            old.date,
            old.voucher_no
        ),
        _ => format!("Ghi nhận số HĐĐT {symbol}/{hddt_no} ngày {hddt_date}"),
    };
    sqlx::query!(
        r#"INSERT INTO invoice_event (invoice_id, from_status, to_status, reason, actor, ref)
           VALUES (?, 'draft', 'official', ?, '', ?)"#,
        invoice_id,
        reason,
        hddt_no
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;

    // Cảnh báo cho người dùng: ngày sổ sách chưa theo ngày HĐĐT.
    let warning = match (date_synced, old.voucher_no.trim().is_empty()) {
        (_, true) | (true, _) => String::new(),
        (false, false) if !hddt_date.is_empty() && hddt_date != old.date => {
            if same_period(&old.date, hddt_date) {
                format!(
                    "Phiếu xuất {} giữ ngày {} — doanh thu vẫn kê ở kỳ của {}, còn ngày trên HĐĐT là {}",
                    old.voucher_no, old.date, old.date, hddt_date
                )
            } else {
                format!(
                    "Lệch KỲ: phiếu xuất {} ghi ngày {} nhưng HĐĐT ngày {} — doanh thu đang kê ở kỳ của {}. Cần lập phiếu điều chỉnh nếu muốn đổi kỳ.",
                    old.voucher_no, old.date, hddt_date, old.date
                )
            }
        }
        _ => String::new(),
    };

    Ok(json!({
        "ok": true,
        "symbol": symbol,
        "date_synced": date_synced,
        "old_date": old.date,
        "hddt_date": hddt_date,
        "voucher_no": old.voucher_no,
        "warning": warning,
    }))
}

#[tauri::command]
pub(crate) async fn link_hddt(
    state: State<'_, AppState>,
    invoice_id: i64,
    hddt_no: String,
    hddt_symbol: String,
    hddt_date: String,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let res = link_hddt_core(
        &*state.pool.read().await,
        invoice_id,
        &hddt_no,
        &hddt_symbol,
        &hddt_date,
    )
    .await?;
    audit(
        &state,
        "link_hddt",
        "invoice",
        &format!(
            "invoice_id={invoice_id} → {}/{}",
            hddt_symbol.trim(),
            hddt_no
        ),
    )
    .await;
    Ok(res.to_string())
}

/// Sinh số phiếu xuất kế tiếp (PX + 3 chữ số) — cùng quy tắc với màn Xuất kho để
/// số do backend sinh không lệch với số màn hình gợi ý.
async fn next_px_no(pool: &SqlitePool) -> Result<String, String> {
    crate::commands::stock::next_voucher_no_core(pool, "PX").await
}

/// Sinh số hóa đơn nội bộ kế tiếp (HD + 4 chữ số) — cùng quy tắc với màn Hóa đơn.
async fn next_invoice_no(pool: &SqlitePool) -> Result<String, String> {
    let rows: Vec<String> = sqlx::query_scalar!("SELECT number FROM invoice")
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
    let max = rows
        .iter()
        .map(|v| v.chars().filter(char::is_ascii_digit).collect::<String>())
        .filter_map(|digits| digits.parse::<i64>().ok())
        .max()
        .unwrap_or(0);
    Ok(format!("HD{:04}", max + 1))
}

/// Thay thế hóa đơn điện tử đã phát hành (Thông tư 91/2026/TT-BTC Điều 10).
///
/// Từ 01/07/2026 không được hủy hóa đơn đã lập. Sai sót thì phát hành hóa đơn
/// **thay thế**: hóa đơn cũ giữ nguyên trong sổ (đánh dấu "đã bị thay thế"), số
/// tiền bị đảo bằng một bút toán Nợ 511 / Có 131 đúng bằng từng dòng hàng, và
/// app tạo sẵn một hóa đơn nháp mới (sao chép khách + dòng hàng) để người dùng
/// sửa sai sót rồi chép sang bên kia phát hành.
///
/// **Không đụng tồn kho**: hàng hoá chỉ sai trên giấy tờ, thực tế vẫn nằm trong
/// kho — phiếu xuất của hóa đơn thay thế sẽ trừ kho như bình thường. Hàng thật sự
/// trả lại thì người dùng lập phiếu nhập kho riêng.
pub(crate) async fn replace_invoice_core(
    pool: &SqlitePool,
    invoice_id: i64,
    reason: &str,
    actor: &str,
) -> Result<serde_json::Value, String> {
    let reason = reason.trim();
    if reason.is_empty() {
        return Err("Cần nhập lý do phải thay thế".into());
    }
    let inv = sqlx::query!(
        r#"SELECT number, e_invoice_no, date, customer, customer_tax_code, status, voucher_no, total
             FROM invoice WHERE id = ?"#,
        invoice_id
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "Không tìm thấy hóa đơn".to_string())?;
    match inv.status.as_str() {
        "official" | "adjusted" => {}
        "draft" | "exported" => {
            return Err(format!(
                "Hóa đơn {} chưa phát hành — sửa hoặc xoá luôn, không cần thay thế",
                inv.number
            ));
        }
        other => {
            return Err(format!(
                "Hóa đơn {} đang ở trạng thái '{other}'",
                inv.number
            ));
        }
    }

    // Đảo doanh thu từng dòng: Nợ 511 / Có 131, adjust_code = 'GiamDT' để tờ khai
    // thuế trừ đúng vào nhóm ngành đã kê. Số tiền khớp đúng dòng hàng gốc nên
    // hóa đơn thay thế kê lại là cân bằng.
    let lines = sqlx::query!(
        r#"SELECT ii.product_id, ii.quantity, ii.unit_price, ii.subtotal, ii.discount,
                  ii.industry_code, ii.vat_rate, ii.pit_rate, ii.line_name
             FROM invoice_item ii WHERE ii.invoice_id = ? ORDER BY ii.id"#,
        invoice_id
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    if lines.is_empty() {
        return Err(format!("Hóa đơn {} chưa có dòng hàng", inv.number));
    }
    let voucher_no = format!("{}DJ", inv.voucher_no.trim());
    if inv.voucher_no.trim().is_empty() {
        return Err(format!(
            "Hóa đơn {} chưa có phiếu xuất nên chưa có doanh thu trong sổ để đảo — hãy lập phiếu xuất trước",
            inv.number
        ));
    }
    let display_number = if inv.e_invoice_no.trim().is_empty() {
        inv.number.as_str()
    } else {
        inv.e_invoice_no.trim()
    };
    let description = format!("Đảo doanh thu — thay thế hóa đơn {display_number}");
    // Mã khách trong danh mục: ưu tiên theo MST, không có thì theo tên — cùng
    // quy tắc với `create_outbound_from_invoice` để bút toán đảo gắn đúng.
    let mst = inv.customer_tax_code.trim().to_string();
    let customer_code: String = if mst.is_empty() {
        String::new()
    } else {
        sqlx::query_scalar!("SELECT code FROM customer WHERE tax_code = ?", mst)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?
            .unwrap_or_default()
    };
    let customer_code = if customer_code.is_empty() {
        let name = inv.customer.clone();
        sqlx::query_scalar!("SELECT code FROM customer WHERE name = ?", name)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?
            .unwrap_or_default()
    } else {
        customer_code
    };

    // Số hóa đơn mới phải tính TRƯỚC khi mở transaction: pool test chỉ có 1
    // kết nối nên truy vấn ra pool khi đang giữ transaction sẽ kẹt.
    let new_no = next_invoice_no(pool).await?;
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    for l in &lines {
        // `invoice_item.subtotal` đã là giá trị dòng sau chiết khấu (đúng bằng
        // số tiền bút toán Nợ 511 / Có 131 của phiếu xuất gốc).
        let amount = round2(l.subtotal);
        if amount <= 0.0 {
            continue;
        }
        let code = sqlx::query_scalar!("SELECT code FROM product WHERE id = ?", l.product_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        insert_journal_entry(
            &mut tx,
            &inv.date,
            &voucher_no,
            "PX",
            &description,
            &code,
            "",
            &customer_code,
            l.quantity,
            l.unit_price,
            amount,
            "511",
            "131",
            &l.industry_code,
            l.vat_rate,
            l.pit_rate,
            "HKD",
            "GiamDT",
            reason,
            0.0,
        )
        .await?;
    }

    // Hóa đơn thay thế: nháp mới, sao chép khách + dòng hàng, trỏ về hóa đơn cũ.
    let res = sqlx::query!(
        r#"INSERT INTO invoice (number, date, customer, customer_tax_code, total, vat_amount,
                               status, replaces_invoice_id)
           VALUES (?, ?, ?, ?, ?, 0.0, 'draft', ?)"#,
        new_no,
        inv.date,
        inv.customer,
        inv.customer_tax_code,
        inv.total,
        invoice_id
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    let new_id = res.last_insert_rowid();
    for l in &lines {
        sqlx::query!(
            r#"INSERT INTO invoice_item (invoice_id, product_id, quantity, unit_price, subtotal,
                                        industry_code, vat_rate, pit_rate, discount, line_name)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            new_id,
            l.product_id,
            l.quantity,
            l.unit_price,
            l.subtotal,
            l.industry_code,
            l.vat_rate,
            l.pit_rate,
            l.discount,
            l.line_name
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        // Giá bán vừa chép sang hóa đơn thay thế cũng làm giá hiện tại của mặt hàng.
        remember_sale_price(&mut tx, l.product_id, l.unit_price).await?;
    }
    // Hóa đơn cũ: đánh dấu đã bị thay thế, ghi số hóa đơn thay thế + phiếu đảo doanh thu.
    sqlx::query!(
        r#"UPDATE invoice
              SET status = 'replaced', replace_reason = ?, ref_invoice = ?,
                  adjust_voucher_no = ?
            WHERE id = ?"#,
        reason,
        new_no,
        voucher_no,
        invoice_id
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    let note_old =
        format!("Lập hóa đơn thay thế {new_no} — đảo doanh thu bằng phiếu {voucher_no}. {reason}");
    sqlx::query!(
        r#"INSERT INTO invoice_event (invoice_id, from_status, to_status, reason, actor, ref)
           VALUES (?, 'official', 'replaced', ?, ?, ?)"#,
        invoice_id,
        note_old,
        actor,
        new_no
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    let inv_number = inv.number.clone();
    let note_new = format!("Hóa đơn thay thế cho hóa đơn {inv_number}. {reason}");
    sqlx::query!(
        r#"INSERT INTO invoice_event (invoice_id, from_status, to_status, reason, actor, ref)
           VALUES (?, '', 'draft', ?, ?, ?)"#,
        new_id,
        note_new,
        actor,
        inv_number
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;

    Ok(json!({
        "ok": true,
        "adjust_voucher_no": voucher_no,
        "replacement_invoice_id": new_id,
        "replacement_number": new_no,
    }))
}

#[tauri::command]
pub(crate) async fn replace_invoice(
    state: State<'_, AppState>,
    invoice_id: i64,
    reason: String,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let actor = state
        .current_user
        .lock()
        .await
        .as_ref()
        .map(|u| u.username.clone())
        .unwrap_or_default();
    let res = replace_invoice_core(&*state.pool.read().await, invoice_id, &reason, &actor).await?;
    audit(
        &state,
        "replace",
        "invoice",
        &format!("invoice_id={invoice_id}"),
    )
    .await;
    Ok(res.to_string())
}

/// Lập phiếu xuất cho một hóa đơn chưa có phiếu.
///
/// Hóa đơn lập ở màn Hóa đơn chỉ ghi bảng `invoice` — nếu không sinh phiếu xuất
/// thì tồn kho không giảm và doanh thu không vào sổ, tức là khoản bán đó không
/// được tính vào tờ khai thuế. Hàm này dựng phiếu xuất từ đúng dòng hàng của hóa
/// đơn (kể cả tiền chiết khấu) rồi liên kết 1-1 qua `voucher_no`.
///
/// Ngày phiếu = ngày hóa đơn (đã được đồng bộ theo ngày HĐĐT) để doanh thu rơi
/// đúng kỳ của chứng từ. Thiếu tồn thì cả phiếu không được tạo — sửa tồn kho rồi
/// bấm lại, không để lại trạng thái lệch sổ.
pub(crate) async fn create_outbound_from_invoice(
    pool: &SqlitePool,
    invoice_id: i64,
) -> Result<String, String> {
    let inv = sqlx::query!(
        "SELECT number, e_invoice_no, date, customer, customer_tax_code, voucher_no, status
           FROM invoice WHERE id = ?",
        invoice_id
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "Không tìm thấy hóa đơn".to_string())?;
    if !inv.voucher_no.trim().is_empty() {
        return Err(format!(
            "Hóa đơn {} đã có phiếu xuất {}",
            inv.number, inv.voucher_no
        ));
    }
    if matches!(inv.status.as_str(), "replaced" | "cancelled") {
        return Err("Hóa đơn đã bị thay thế — không lập được phiếu xuất".into());
    }

    // Mã khách trong danh mục: ưu tiên theo MST, không có thì theo tên. Không tìm
    // thấy thì để rỗng — phiếu vẫn ghi doanh thu, chỉ không gắn mã khách.
    let mst = inv.customer_tax_code.trim().to_string();
    let customer_code: String = if mst.is_empty() {
        String::new()
    } else {
        sqlx::query_scalar!("SELECT code FROM customer WHERE tax_code = ?", mst)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?
            .unwrap_or_default()
    };
    let customer_code = if customer_code.is_empty() {
        let name = inv.customer.clone();
        sqlx::query_scalar!("SELECT code FROM customer WHERE name = ?", name)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?
            .unwrap_or_default()
    } else {
        customer_code
    };

    let rows = sqlx::query!(
        r#"SELECT p.code, ii.quantity, ii.unit_price, ii.discount, ii.industry_code,
                  ii.warehouse_code
             FROM invoice_item ii JOIN product p ON p.id = ii.product_id
            WHERE ii.invoice_id = ? ORDER BY ii.id"#,
        invoice_id
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    if rows.is_empty() {
        return Err(format!("Hóa đơn {} chưa có dòng hàng", inv.number));
    }
    let items: Vec<OutboundItemInput> = rows
        .iter()
        .map(|r| OutboundItemInput {
            product_code: r.code.clone(),
            quantity: r.quantity,
            unit_price: r.unit_price,
            discount: r.discount,
            industry_code: r.industry_code.clone(),
            warehouse_code: r.warehouse_code.clone(),
            // Phiếu xuất không hiển thị tên dòng — tên(alias) chỉ cần khi LẬP HÓA
            // ĐƠN kèm theo, trường hợp này không tạo hóa đơn nên để rỗng.
            line_name: String::new(),
        })
        .collect();

    let voucher_no = next_px_no(pool).await?;
    // Sổ/phiếu phải chỉ tới số hóa đơn đã phát hành khi có; số nội bộ chỉ dùng
    // cho hóa đơn chưa có số HĐĐT.
    let display_number = if inv.e_invoice_no.trim().is_empty() {
        inv.number.as_str()
    } else {
        inv.e_invoice_no.trim()
    };
    let description = format!("Bán hàng theo hóa đơn {display_number}");
    let empty_invoice = OutboundInvoiceInput {
        number: String::new(),
        e_invoice_no: String::new(),
        e_invoice_symbol: String::new(),
        e_invoice_date: String::new(),
    };
    crate::commands::stock::save_outbound_core(
        pool,
        &inv.date,
        &voucher_no,
        &description,
        &customer_code,
        "HKD",
        &items,
        "",
        false,
        "sale",
        "up",
        &empty_invoice,
    )
    .await?;

    sqlx::query!(
        "UPDATE invoice SET voucher_no = ? WHERE id = ?",
        voucher_no,
        invoice_id
    )
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(voucher_no)
}

/// Một hóa đơn có thể chọn để liên kết với phiếu xuất (màn Phiếu xuất kho).
#[derive(sqlx::FromRow, serde::Serialize)]
pub(crate) struct InvoiceLinkCandidate {
    pub(crate) id: i64,
    pub(crate) number: String,
    pub(crate) e_invoice_no: String,
    pub(crate) date: String,
    pub(crate) customer: String,
    pub(crate) total: f64,
    pub(crate) status: String,
    /// Phiếu xuất đang gắn hóa đơn này (rỗng = chưa gắn) — UI cảnh báo trước
    /// khi người dùng chọn nhầm hóa đơn của phiếu khác.
    pub(crate) voucher_no: String,
}

/// Hóa đơn có thể liên kết với 1 phiếu xuất.
///
/// Chỉ lấy hóa đơn cùng kỳ thuế với phiếu (kỳ của hóa đơn trên sổ phải khớp kỳ
/// phiếu, nếu không doanh thu và tờ khai lệch nhau) — trừ hóa đơn đang gắn sẵn
/// với chính phiếu đó để người dùng vẫn thấy liên kết hiện tại.
#[tauri::command]
pub(crate) async fn invoice_link_candidates(
    state: State<'_, AppState>,
    voucher_no: String,
    search: String,
) -> Result<String, String> {
    let rows =
        invoice_link_candidates_core(&*state.pool.read().await, &voucher_no, &search).await?;
    Ok(serde_json::to_string(&rows).unwrap_or_default())
}

/// Thân truy vấn danh sách hóa đơn có thể liên kết — tách riêng để test trực tiếp.
pub(crate) async fn invoice_link_candidates_core(
    pool: &SqlitePool,
    voucher_no: &str,
    search: &str,
) -> Result<Vec<InvoiceLinkCandidate>, String> {
    let voucher_no = voucher_no.trim();
    let posting_date: Option<String> = sqlx::query_scalar!(
        r#"SELECT MIN(posting_date) as "posting_date?" FROM journal_entry
            WHERE entry_type = 'PX' AND voucher_no = ?"#,
        voucher_no
    )
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;
    let posting_date =
        posting_date.ok_or_else(|| format!("Không tìm thấy phiếu xuất {voucher_no}"))?;

    let mut qb = sqlx::QueryBuilder::<Sqlite>::new(
        "SELECT id, number, e_invoice_no, date, customer, total, status, voucher_no
           FROM invoice
          WHERE status NOT IN ('replaced', 'cancelled')
            AND (substr(date, 1, 7) = substr(",
    );
    qb.push_bind(posting_date.clone());
    qb.push(", 1, 7) OR voucher_no = ");
    qb.push_bind(voucher_no.to_string());
    qb.push(")");
    let kw = search.trim();
    if !kw.is_empty() {
        // Khoá ký tự đại diện để gõ `%` cũng chỉ tìm đúng chữ đó.
        let like = format!("%{}%", kw.replace('%', "\\%").replace('_', "\\_"));
        qb.push(" AND (number LIKE ")
            .push_bind(like.clone())
            .push(" ESCAPE '\\' OR e_invoice_no LIKE ")
            .push_bind(like.clone())
            .push(" ESCAPE '\\' OR customer LIKE ")
            .push_bind(like)
            .push(" ESCAPE '\\')");
    }
    // Hóa đơn đang gắn phiếu này lên đầu, rồi tới hóa đơn tự do, cuối cùng là
    // hóa đơn đã thuộc phiếu khác (UI vẫn hiện nhưng bấm liên kết sẽ bị chặn).
    qb.push(" ORDER BY CASE WHEN voucher_no = ")
        .push_bind(voucher_no.to_string())
        .push(" THEN 0 WHEN voucher_no = '' THEN 1 ELSE 2 END, e_invoice_date DESC, id DESC LIMIT 100");
    qb.build_query_as()
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())
}

/// Liên kết lại hóa đơn đại diện cho 1 phiếu xuất.
///
/// Phiếu xuất và hóa đơn liên kết 1-1 qua `invoice.voucher_no`. Khi phát hiện
/// phiếu trỏ nhầm hóa đơn (hoặc hóa đơn nhập tay cần gắn vào phiếu có sẵn), hàm
/// này gỡ liên kết cũ của phiếu rồi gắn hóa đơn được chọn — không đụng dòng
/// hàng/doanh thu của phiếu, chỉ sửa con trỏ liên kết.
pub(crate) async fn link_outbound_invoice_core(
    pool: &SqlitePool,
    voucher_no: &str,
    invoice_id: i64,
) -> Result<serde_json::Value, String> {
    let voucher_no = voucher_no.trim();
    if voucher_no.is_empty() {
        return Err("Thiếu số phiếu xuất".into());
    }
    // Kiểm tra phiếu đích trước: số phiếu sai là lỗi rõ ràng hơn mọi xung đột
    // của hóa đơn được chọn.
    let posting_date: Option<String> = sqlx::query_scalar!(
        r#"SELECT MIN(posting_date) as "posting_date?" FROM journal_entry
            WHERE entry_type = 'PX' AND voucher_no = ?"#,
        voucher_no
    )
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;
    let posting_date =
        posting_date.ok_or_else(|| format!("Không tìm thấy phiếu xuất {voucher_no}"))?;

    let inv = sqlx::query!(
        "SELECT id, number, e_invoice_no, date, status, voucher_no
           FROM invoice WHERE id = ?",
        invoice_id
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "Không tìm thấy hóa đơn".to_string())?;
    if matches!(inv.status.as_str(), "replaced" | "cancelled") {
        return Err(format!(
            "Hóa đơn {} đã bị thay thế/hủy — không liên kết được",
            inv.number
        ));
    }
    if !inv.voucher_no.trim().is_empty() && inv.voucher_no.trim() != voucher_no {
        return Err(format!(
            "Hóa đơn {} đang gắn phiếu xuất {} — gỡ liên kết ở phiếu đó trước",
            inv.number, inv.voucher_no
        ));
    }
    // Đổi liên kết sang hóa đơn khác kỳ sẽ làm doanh thu trên sổ rơi khác kỳ với
    // hóa đơn — trường hợp đó phải lập phiếu điều chỉnh, không sửa con trỏ.
    if !same_period(&posting_date, &inv.date) {
        return Err(format!(
            "Hóa đơn {} ngày {} thuộc kỳ khác phiếu {} ngày {} — không liên kết chéo kỳ",
            inv.number, inv.date, voucher_no, posting_date
        ));
    }

    let display_number = if inv.e_invoice_no.trim().is_empty() {
        inv.number.clone()
    } else {
        inv.e_invoice_no.trim().to_string()
    };

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    // Gỡ hóa đơn cũ đang đại diện cho phiếu (nếu có) — mỗi phiếu chỉ 1 hóa đơn.
    sqlx::query!(
        "UPDATE invoice SET voucher_no = '' WHERE voucher_no = ? AND id <> ?",
        voucher_no,
        invoice_id
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    sqlx::query!(
        "UPDATE invoice SET voucher_no = ? WHERE id = ?",
        voucher_no,
        invoice_id
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    // Diễn giải do app tự sinh khi lập phiếu từ hóa đơn thì cập nhật theo hóa đơn
    // mới; diễn giải người dùng tự nhập giữ nguyên.
    let description = format!("Bán hàng theo hóa đơn {display_number}");
    sqlx::query!(
        r#"UPDATE journal_entry SET description = ?
            WHERE entry_type = 'PX' AND voucher_no = ?
              AND description LIKE 'Bán hàng theo hóa đơn %'"#,
        description,
        voucher_no
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;

    Ok(json!({
        "voucher_no": voucher_no,
        "invoice_id": invoice_id,
        "display_number": display_number,
    }))
}

#[tauri::command]
pub(crate) async fn link_outbound_invoice(
    state: State<'_, AppState>,
    voucher_no: String,
    invoice_id: i64,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let res =
        link_outbound_invoice_core(&*state.pool.read().await, &voucher_no, invoice_id).await?;
    audit(
        &state,
        "link_outbound_invoice",
        "invoice",
        &format!(
            "{} → {} (id={})",
            voucher_no.trim(),
            res["display_number"].as_str().unwrap_or(""),
            invoice_id
        ),
    )
    .await;
    Ok(res.to_string())
}

#[tauri::command]
pub(crate) async fn create_invoice_outbound(
    state: State<'_, AppState>,
    invoice_id: i64,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let no = create_outbound_from_invoice(&*state.pool.read().await, invoice_id).await?;
    audit(
        &state,
        "create_outbound",
        "invoice",
        &format!("invoice_id={invoice_id} → {no}"),
    )
    .await;
    Ok(no)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;

    fn line(
        code: &str,
        qty: f64,
        price: f64,
        discount: f64,
        warehouse_code: &str,
    ) -> InvoiceItemInput {
        InvoiceItemInput {
            product_code: code.into(),
            quantity: qty,
            unit_price: price,
            industry_code: "PPHH".into(),
            discount,
            warehouse_code: warehouse_code.into(),
            line_name: String::new(),
        }
    }

    // Số "rác" dài (nhập nhầm HD99999999) không được kéo theo việc liệt kê cả
    // chục triệu số hổng — chạm trần là dừng; khoảng hổng nhỏ vẫn liệt kê đủ.
    #[test]
    fn number_gaps_dung_o_tran_voi_khoang_khong_lo() {
        assert_eq!(number_gaps(&[1, 4]), vec![2, 3]);
        let gaps = number_gaps(&[1, 1_000_000_000]);
        assert_eq!(gaps.len(), MAX_MISSING_NUMBERS);
        assert_eq!(gaps.first(), Some(&2));
    }

    #[tokio::test]
    async fn danh_sach_hoa_don_mac_dinh_xep_ngay_hddt_giam_dan() {
        let pool = test_pool().await;
        // Ngày hóa đơn và Ngày HĐĐT cố tình lệch nhau để chắc chắn danh sách
        // không còn xếp theo `date`; 2 dòng không có Ngày HĐĐT lấy ngày hóa đơn.
        for (number, date, e_invoice_date) in [
            ("HD-SORT-A", "2026-01-10", "2026-03-10"),
            ("HD-SORT-B", "2026-03-10", "2026-02-10"),
            ("HD-SORT-C", "2026-02-10", "2026-04-10"),
            ("HD-SORT-D", "2026-02-20", ""),
            ("HD-SORT-E", "2026-01-01", ""),
        ] {
            sqlx::query(
                "INSERT INTO invoice (number, date, customer, status, e_invoice_date)
                 VALUES (?, ?, 'Khách kiểm tra sắp xếp', 'official', ?)",
            )
            .bind(number)
            .bind(date)
            .bind(e_invoice_date)
            .execute(&pool)
            .await
            .unwrap();
        }

        async fn numbers(lazy_event: &str, pool: &SqlitePool) -> Vec<String> {
            let body = invoices_page_core(pool, lazy_event, false).await.unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
            parsed["rows"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|row| row["customer"] == "Khách kiểm tra sắp xếp")
                .map(|row| row["number"].as_str().unwrap().to_string())
                .collect()
        }

        let expected = [
            "HD-SORT-C",
            "HD-SORT-A",
            "HD-SORT-D",
            "HD-SORT-B",
            "HD-SORT-E",
        ];
        assert_eq!(numbers(r#"{"first":0,"rows":50}"#, &pool).await, expected);
        // Bấm cột "Ngày HĐĐT" lần đầu cũng phải là giảm dần.
        assert_eq!(
            numbers(
                r#"{"first":0,"rows":50,"sortField":"e_invoice_date","sortOrder":-1}"#,
                &pool,
            )
            .await,
            expected
        );
    }

    /// Báo thiếu số hiệu: gom số nội bộ HD#### và số HĐĐT theo TỪNG ký hiệu —
    /// chỉ các số hổng GIỮA hai số đã có, giữ đúng độ dài số đã ghi
    /// (00000089 → 00000090) để đối chiếu y hệt cổng, ký hiệu khác không đếm chéo.
    #[tokio::test]
    async fn bao_thieu_so_hieu_soi_noi_bo_va_so_hd_dau_theo_ky_hieu() {
        let pool = test_pool().await;
        for n in ["HD0001", "HD0002", "HD0004", "HD0005"] {
            sqlx::query!(
                "INSERT INTO invoice (number, date, customer, customer_tax_code, total, status)
                 VALUES (?, '2026-05-01', 'KH test', '', 0.0, 'official')",
                n
            )
            .execute(&pool)
            .await
            .unwrap();
        }
        // Số nội bộ: thiếu HD0003 giữa HD0002 và HD0004.
        // Ký hiệu K1: 00000089 + 00000091 → thiếu 00000090 (đúng 8 chữ số).
        // Ký hiệu K2: 12 + 13 → liền mạch, không đếm chéo với K1.
        for (number, no, symbol) in [
            ("HD0001", "00000089", "K1"),
            ("HD0002", "00000091", "K1"),
            ("HD0004", "12", "K2"),
            ("HD0005", "13", "K2"),
        ] {
            sqlx::query!(
                "UPDATE invoice SET e_invoice_no = ?, e_invoice_symbol = ? WHERE number = ?",
                no,
                symbol,
                number
            )
            .execute(&pool)
            .await
            .unwrap();
        }

        let v = invoice_missing_numbers_core(&pool).await.unwrap();
        assert_eq!(v["internal_missing"], json!(["HD0003"]));
        let official = v["official"].as_array().expect("danh sách ký hiệu");
        assert_eq!(official.len(), 1, "chỉ ký hiệu K1 có số hổng");
        assert_eq!(official[0]["symbol"], json!("K1"));
        assert_eq!(official[0]["present"], json!(2));
        assert_eq!(official[0]["missing"], json!(["00000090"]));
    }

    #[tokio::test]
    async fn hoa_don_tru_tien_ck_va_tinh_thue_theo_gia_tri_thuc() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "HD-A", "Hàng bán", 0.01).await;
        let w = seed_warehouse(&pool, "W1", "Kho 1").await;
        add_stock_lot(&pool, p, w, 100.0, 1000.0, "2026-01-01").await;

        // 10 x 2000 = 20000 − CK 5000 → giá trị dòng 15000.
        let res = save_invoice_core(
            &pool,
            "HD0001",
            "2026-03-10",
            "Khách X",
            "MST-X",
            &[line("HD-A", 10.0, 2000.0, 5000.0, "W1")],
        )
        .await
        .expect("lưu hóa đơn có chiết khấu");

        assert_eq!(res["total"].as_f64().unwrap(), 15_000.0);
        // Thuế theo nhóm ngành PPHH (vat_rate 1%): 15000 × 0.01 = 150
        assert_eq!(res["tax_payable"].as_f64().unwrap(), 150.0);

        // Dòng hóa đơn lưu đủ discount + kho xuất
        let row = sqlx::query!("SELECT subtotal, discount, warehouse_code FROM invoice_item")
            .fetch_one(&pool)
            .await
            .unwrap();
        let (subtotal, discount, wh) = (row.subtotal, row.discount, row.warehouse_code);
        assert_eq!((subtotal, discount), (15_000.0, 5_000.0));
        assert_eq!(wh, "W1");
    }

    // "Giá tiền tự nhớ": giá bán ghi trên hóa đơn trở thành GIÁ BÁN HIỆN TẠI
    // của mặt hàng để lần sau chọn mặt hàng là tự lấy đúng giá; dòng giá 0
    // (khuyến mại / thiếu giá) không xoá giá đã lưu.
    #[tokio::test]
    async fn lap_hoa_don_nho_lai_gia_ban_ra_hien_tai_cua_mat_hang() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "HD-GIA", "Hàng nhớ giá", 0.01).await;
        let p0 = seed_product(&pool, "HD-GIA0", "Hàng giá 0", 0.01).await;
        let w = seed_warehouse(&pool, "W1", "Kho 1").await;
        add_stock_lot(&pool, p, w, 10.0, 1000.0, "2026-01-01").await;
        add_stock_lot(&pool, p0, w, 10.0, 1000.0, "2026-01-01").await;
        sqlx::query!("UPDATE product SET sale_price = 5555.0 WHERE code = 'HD-GIA0'")
            .execute(&pool)
            .await
            .unwrap();

        save_invoice_core(
            &pool,
            "HD-GIA1",
            "2026-03-10",
            "Khách nhớ giá",
            "",
            &[
                line("HD-GIA", 1.0, 7777.0, 0.0, "W1"),
                line("HD-GIA0", 1.0, 0.0, 0.0, "W1"),
            ],
        )
        .await
        .expect("lưu hóa đơn");

        let sale = sqlx::query!("SELECT sale_price FROM product WHERE code = 'HD-GIA'")
            .fetch_one(&pool)
            .await
            .unwrap()
            .sale_price;
        assert_eq!(sale, 7777.0, "giá bán trên hóa đơn làm giá hiện tại");
        let sale0 = sqlx::query!("SELECT sale_price FROM product WHERE code = 'HD-GIA0'")
            .fetch_one(&pool)
            .await
            .unwrap()
            .sale_price;
        assert_eq!(sale0, 5555.0, "dòng giá 0 không xoá giá đã lưu");
    }

    #[tokio::test]
    async fn hoa_don_kiem_tra_ton_theo_dung_kho_xuat_tren_dong() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "HD-B", "Hàng B", 0.01).await;
        let w1 = seed_warehouse(&pool, "W1", "Kho 1").await;
        // W2 chỉ tồn tại trong DB — bài test khai kho bằng chuỗi "W2", không cần giữ biến.
        seed_warehouse(&pool, "W2", "Kho 2").await;
        // W1 có 5, W2 không có gì
        add_stock_lot(&pool, p, w1, 5.0, 1000.0, "2026-01-01").await;

        // Tồn nằm ở W1 nhưng khai kho W2 → thiếu dù tổng kho đủ. Nháp VẪN lưu được
        // (HĐĐT lên cổng có độ trễ) nhưng phải báo đúng dòng thiếu cho app hỏi.
        let res = save_invoice_core(
            &pool,
            "HD0002",
            "2026-03-10",
            "Khách Y",
            "MST-Y",
            &[line("HD-B", 4.0, 2000.0, 0.0, "W2")],
        )
        .await
        .expect("nháp được lưu dù thiếu tồn");
        let shortage = res["shortage"].as_array().cloned().unwrap_or_default();
        assert_eq!(shortage.len(), 1, "phải báo đúng 1 dòng thiếu: {res}");
        let detail = shortage[0].as_str().unwrap_or_default().to_string();
        assert!(detail.contains("HD-B"), "{detail}");
        assert!(detail.contains("kho đã chọn"), "{detail}");

        // Ghi số HĐĐT (chốt thành hóa đơn thật) khi vẫn thiếu kho → chặn.
        let id: i64 =
            sqlx::query_scalar!(r#"SELECT id as "id!" FROM invoice WHERE number = 'HD0002'"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        let err = link_hddt_core(&pool, id, "00000010", "1C26TT152", "2026-03-10")
            .await
            .unwrap_err();
        assert!(err.contains("Không đủ tồn kho"), "err: {err}");

        // Đủ tồn ở đúng kho W1 → không thiếu gì, ghi số HĐĐT được.
        let res = save_invoice_core(
            &pool,
            "HD0003",
            "2026-03-10",
            "Khách Y",
            "MST-Y",
            &[line("HD-B", 4.0, 2000.0, 0.0, "W1")],
        )
        .await
        .expect("đủ tồn kho đúng kho");
        assert!(
            res["shortage"]
                .as_array()
                .map(|v| v.is_empty())
                .unwrap_or(false),
            "đúng kho thì không thiếu gì: {res}"
        );
        let id: i64 =
            sqlx::query_scalar!(r#"SELECT id as "id!" FROM invoice WHERE number = 'HD0003'"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        link_hddt_core(&pool, id, "00000011", "1C26TT152", "2026-03-10")
            .await
            .expect("đủ tồn ở đúng kho thì ghi số được");
    }

    /// Màu dòng thông minh hơn (yêu cầu #8 mục 9): hàng chế tạo thiếu thành
    /// phẩm nhưng đủ NVL → `short` (đỏ thường, sản xuất bù được); NVL cũng
    /// thiếu → `material` (đỏ đậm); đủ thành phẩm → `ok` (tím, đủ điều kiện xuất).
    #[tokio::test]
    async fn soi_ton_hoa_don_che_tao_phan_biet_thieu_nvl_va_du_dieu_kien_xuat() {
        let pool = test_pool().await;
        let w = seed_warehouse(&pool, "W-BOM", "Kho BOM").await;
        let tp = seed_product(&pool, "TP-BOM", "Thành phẩm BOM", 0.01).await;
        let nvl = seed_product(&pool, "NVL-BOM", "NVL BOM", 0.01).await;
        // 1 thành phẩm cần 2 NVL.
        sqlx::query(
            "INSERT INTO product_bom_item (product_id, material_product_id, quantity)
             VALUES (?, ?, 2.0)",
        )
        .bind(tp)
        .bind(nvl)
        .execute(&pool)
        .await
        .unwrap();
        add_stock_lot(&pool, nvl, w, 10.0, 1000.0, "2026-01-01").await;

        // 2 dòng × 2 TP = 4 TP; NVL cần 8 ≤ 10 → chỉ thiếu thành phẩm.
        save_invoice_core(
            &pool,
            "HD-BOM1",
            "2026-03-10",
            "Khách BOM",
            "",
            &[
                line("TP-BOM", 2.0, 5000.0, 0.0, "W-BOM"),
                line("TP-BOM", 2.0, 5000.0, 0.0, "W-BOM"),
            ],
        )
        .await
        .expect("nháp hàng chế tạo vẫn lưu được");
        let id: i64 = sqlx::query_scalar("SELECT id FROM invoice WHERE number = ?")
            .bind("HD-BOM1")
            .fetch_one(&pool)
            .await
            .unwrap();

        let mut tx = pool.begin().await.unwrap();
        let scan = invoice_stock_scan(&mut tx, id, true).await.unwrap();
        tx.rollback().await.unwrap();
        assert_eq!(scan.level(), "short", "đủ NVL thì chỉ đỏ thường");
        assert!(scan.lines.iter().any(|l| l.contains("TP-BOM")));
        assert!(
            scan.lines.iter().all(|l| !l.contains("NVL-BOM")),
            "chưa được báo thiếu NVL: {:?}",
            scan.lines
        );

        // NVL chỉ còn 7 < 8 cần cho 4 TP → không thể tự sản xuất bù.
        sqlx::query("UPDATE stock_lot SET quantity = 7.0 WHERE product_id = ?")
            .bind(nvl)
            .execute(&pool)
            .await
            .unwrap();
        let mut tx = pool.begin().await.unwrap();
        let scan = invoice_stock_scan(&mut tx, id, true).await.unwrap();
        tx.rollback().await.unwrap();
        assert_eq!(scan.level(), "material", "thiếu NVL phải đỏ đậm");
        assert!(
            scan.lines.iter().any(|l| l.contains("NVL-BOM")),
            "phải nêu rõ NVL thiếu: {:?}",
            scan.lines
        );

        // Nhập đủ 5 thành phẩm → hóa đơn 5 TP đủ điều kiện xuất.
        add_stock_lot(&pool, tp, w, 5.0, 1000.0, "2026-01-02").await;
        save_invoice_core(
            &pool,
            "HD-BOM2",
            "2026-03-11",
            "Khách BOM",
            "",
            &[line("TP-BOM", 5.0, 5000.0, 0.0, "W-BOM")],
        )
        .await
        .expect("nháp đủ tồn");
        let id2: i64 = sqlx::query_scalar("SELECT id FROM invoice WHERE number = ?")
            .bind("HD-BOM2")
            .fetch_one(&pool)
            .await
            .unwrap();
        let mut tx = pool.begin().await.unwrap();
        let scan = invoice_stock_scan(&mut tx, id2, true).await.unwrap();
        tx.rollback().await.unwrap();
        assert_eq!(scan.level(), "ok", "đủ thành phẩm phải là ok (nền tím)");
        assert!(scan.lines.is_empty(), "{:?}", scan.lines);
    }

    #[tokio::test]
    async fn hoa_don_khong_khai_kho_kiem_theo_tong_cac_kho() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "HD-C", "Hàng C", 0.01).await;
        let w1 = seed_warehouse(&pool, "W1", "Kho 1").await;
        let w2 = seed_warehouse(&pool, "W2", "Kho 2").await;
        add_stock_lot(&pool, p, w1, 3.0, 1000.0, "2026-01-01").await;
        add_stock_lot(&pool, p, w2, 2.0, 1000.0, "2026-01-01").await;

        // Không chọn kho → cộng dồn 3 + 2 = 5, đủ xuất 5
        let res = save_invoice_core(
            &pool,
            "HD0004",
            "2026-03-10",
            "Khách Z",
            "MST-Z",
            &[line("HD-C", 5.0, 1000.0, 0.0, "")],
        )
        .await
        .expect("đủ tổng tồn các kho");
        assert_eq!(res["total"].as_f64().unwrap(), 5_000.0);
        // Kho xuất để trống → lưu chuỗi rỗng
        let wh: String = sqlx::query_scalar!("SELECT warehouse_code FROM invoice_item")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(wh, "");
    }

    /// Giả lập trừ kho cho hóa đơn nháp (issue #5): 10 đèn còn lưu, nháp 1 chiếm
    /// hết 10 → nháp 2 lập thêm phải BÁO HẾT HÀNG (dù kho thật chưa trừ lần nào).
    /// Sửa chính nháp 1 thì không tự trừ phần giữ chỗ của mình.
    #[tokio::test]
    async fn nhap_kho_nhap_giu_cho_nan_con_hang_nhap_sau_bao_het() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "HD-GIU", "Đèn GIU", 0.01).await;
        let w = seed_warehouse(&pool, "W-GIU", "Kho GIU").await;
        add_stock_lot(&pool, p, w, 10.0, 1000.0, "2026-01-01").await;

        // Nháp 1 chiếm hết 10 → lưu được, không thiếu gì (kho thật đủ).
        let r1 = save_invoice_core(
            &pool,
            "HD0001",
            "2026-03-10",
            "Khách G",
            "MST-G",
            &[line("HD-GIU", 10.0, 2000.0, 0.0, "W-GIU")],
        )
        .await
        .expect("nháp 1 chiếm hết tồn, kho thật đủ");
        assert!(
            r1["shortage"].as_array().unwrap().is_empty(),
            "kho đủ thì nháp 1 không thiếu: {r1}"
        );
        let id1: i64 =
            sqlx::query_scalar!(r#"SELECT id as "id!" FROM invoice WHERE number = 'HD0001'"#)
                .fetch_one(&pool)
                .await
                .unwrap();

        // Sửa lại nháp 1 (cùng số lượng) → chỉ soi phần giữ chỗ của NHÁP KHÁC,
        // chưa có nháp nào khác → không tự trừ chính mình, không báo thiếu oan.
        let r1b = update_invoice_core(
            &pool,
            id1,
            "HD0001",
            "2026-03-10",
            "Khách G",
            "MST-G",
            &[line("HD-GIU", 10.0, 2000.0, 0.0, "W-GIU")],
        )
        .await
        .expect("sửa nháp 1 không tự soi chính mình");
        assert!(
            r1b["shortage"].as_array().unwrap().is_empty(),
            "tự trừ chính mình là sai: {r1b}"
        );

        // Nháp 2 lập thêm cùng số lượng → HẾT HÀNG đã bị nháp khác giữ chỗ.
        let r2 = save_invoice_core(
            &pool,
            "HD0002",
            "2026-03-11",
            "Khách G",
            "MST-G",
            &[line("HD-GIU", 10.0, 2000.0, 0.0, "W-GIU")],
        )
        .await
        .expect("nháp vẫn lưu được nhưng phải báo hết");
        let shortage = r2["shortage"].as_array().cloned().unwrap_or_default();
        assert_eq!(shortage.len(), 1, "nháp 2 phải báo đúng 1 dòng hết: {r2}");
        let detail = shortage[0].as_str().unwrap_or_default();
        assert!(detail.contains("Đèn GIU"), "{detail}");
        assert!(detail.contains("chỉ còn 0.00"), "{detail}");
        assert!(detail.contains("đã chiếm 10.00"), "{detail}");
    }

    /// Lập 1 hóa đơn nháp có dòng hàng, trả về id. Mã hàng suy ra từ số hóa đơn
    /// để gọi nhiều lần trong 1 test không đụng ràng buộc UNIQUE.
    async fn seed_draft(pool: &SqlitePool, number: &str) -> i64 {
        let code = format!("HD-{}", number);
        let p = seed_product(pool, &code, "Hàng nháp", 0.01).await;
        let w = seed_warehouse(pool, &format!("W-{number}"), "Kho nháp").await;
        add_stock_lot(pool, p, w, 100.0, 1000.0, "2026-01-01").await;
        save_invoice_core(
            pool,
            number,
            "2026-03-10",
            "Khách X",
            "MST-X",
            &[line(&code, 1.0, 1000.0, 0.0, &format!("W-{number}"))],
        )
        .await
        .expect("lập được hóa đơn nháp");
        let id: i64 = sqlx::query_scalar!(
            r#"SELECT id as "id!" FROM invoice WHERE number = ?"#,
            number
        )
        .fetch_one(pool)
        .await
        .unwrap();
        id
    }

    /// Hóa đơn nội bộ → `detail_json` đủ khung để render PDF ở màn Chờ xuất HĐĐT.
    #[tokio::test]
    async fn xem_truoc_pdf_dung_detail_json_cua_hoa_don_noi_bo() {
        let pool = test_pool().await;
        sqlx::query!(
            "INSERT INTO business (id, name, tax_code, address, phone, hddt_symbol)
             VALUES (1, 'Hộ KD Test', '0123456789', 'Số 1 Đường A', '0241112223', 'C26TST')
             ON CONFLICT(id) DO UPDATE SET
                 name = excluded.name, tax_code = excluded.tax_code,
                 address = excluded.address, phone = excluded.phone,
                 hddt_symbol = excluded.hddt_symbol"
        )
        .execute(&pool)
        .await
        .expect("hồ sơ hộ kinh doanh");

        let id = seed_draft(&pool, "HDPDF").await;
        let raw = invoice_draft_detail_json(&pool, id)
            .await
            .expect("dựng được detail_json");
        let data: serde_json::Value = serde_json::from_str(&raw).expect("JSON hợp lệ");

        // Bên bán lấy từ hồ sơ, ký hiệu = ký hiệu HĐĐT của hộ.
        assert_eq!(data["nbten"], "Hộ KD Test");
        assert_eq!(data["nbmst"], "0123456789");
        assert_eq!(data["nbdchi"], "Số 1 Đường A");
        assert_eq!(data["khhdon"], "C26TST");
        assert_eq!(data["shdon"], "HDPDF");
        assert_eq!(
            data["nky"], "2026-03-10",
            "ngày hóa đơn → dòng \"Ngày …\" in PDF"
        );
        // Bên mua.
        assert_eq!(data["nmten"], "Khách X");
        assert_eq!(data["nmmst"], "MST-X");

        // 1 dòng 1 × 1.000đ không chiết khấu → cộng tiền hàng = tổng thanh toán.
        assert_eq!(data["tgtcthue"], 1000.0);
        assert_eq!(data["tgtttbso"], 1000.0);
        assert_eq!(data["tgtttbchu"], "Một nghìn đồng");
        assert_eq!(data["tgtthue"], 0.0, "hộ kinh doanh không tách thuế GTGT");
        assert!(data["thttltsuat"].as_array().expect("mảng").is_empty());

        let rows = data["hdhhdvu"].as_array().expect("dòng hàng");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["ten"], "Hàng nháp");
        assert_eq!(rows[0]["thtien"], 1000.0);
        assert_eq!(rows[0]["ltsuat"], "1%", "tỷ lệ thuế từ sản phẩm");
        assert_eq!(rows[0]["sluong"], 1.0);
    }

    /// Bản xem trước phân 2 nhánh: nháp / đã chép in rõ "HÓA ĐƠN NHÁP" với mẫu
    /// số + ký hiệu `000000`, đã phát hành giữ nguyên khung in (ký hiệu lấy từ
    /// hồ sơ hộ) — cả hai đều có QR. ZIP báo cáo kỳ thuế không đổi vì nó đọc
    /// [`invoice_draft_detail_json`].
    #[tokio::test]
    async fn xem_truoc_pdf_phan_biet_ban_nhap_va_ban_da_phat_hanh() {
        let pool = test_pool().await;
        sqlx::query!(
            "INSERT INTO business (id, name, tax_code, address, phone, hddt_symbol)
             VALUES (1, 'Hộ KD Test', '0123456789', 'Số 1 Đường A', '0241112223', 'C26TST')
             ON CONFLICT(id) DO UPDATE SET
                 name = excluded.name, tax_code = excluded.tax_code,
                 address = excluded.address, phone = excluded.phone,
                 hddt_symbol = excluded.hddt_symbol"
        )
        .execute(&pool)
        .await
        .expect("hồ sơ hộ kinh doanh");

        let id = seed_draft(&pool, "HDPRE").await;

        // ── 1) Nháp: chưa có số hiệu của cổng → nhãn nháp +000000 + QR ──
        let raw = invoice_preview_json(&pool, id)
            .await
            .expect("xem trước được");
        let draft: serde_json::Value = serde_json::from_str(&raw).expect("JSON hợp lệ");
        assert_eq!(draft["tlhdon"], "HÓA ĐƠN NHÁP");
        assert_eq!(draft["khmshdon"], "000000", "mẫu số tạm của bản nháp");
        assert_eq!(draft["khhdon"], "000000", "ký hiệu tạm của bản nháp");
        assert_eq!(draft["qrcode"], "https://github.com/tachibana-shin");

        // ── 2) Đã chép sang cổng → vẫn chưa phát hành nên vẫn là bản nháp;
        //       phát hành xong thì giữ nguyên khung in, chỉ còn thiếu QR ──
        for status in ["exported", "official"] {
            sqlx::query("UPDATE invoice SET status = ? WHERE id = ?")
                .bind(status)
                .bind(id)
                .execute(&pool)
                .await
                .unwrap_or_else(|e| panic!("đổi trạng thái {status}: {e}"));
            let raw = invoice_preview_json(&pool, id)
                .await
                .expect("xem trước được");
            let data: serde_json::Value = serde_json::from_str(&raw).expect("JSON hợp lệ");
            if status == "official" {
                assert_eq!(data["khhdon"], "C26TST", "ký hiệu lấy từ hồ sơ hộ");
                assert_eq!(data["khmshdon"], "", "mẫu số giữ nguyên từ hồ sơ");
                assert!(
                    data.get("tlhdon").is_none(),
                    "không được ghi đè tiêu đề của bản đã phát hành"
                );
            } else {
                assert_eq!(data["tlhdon"], "HÓA ĐƠN NHÁP");
                assert_eq!(data["khmshdon"], "000000");
                assert_eq!(data["khhdon"], "000000");
            }
            assert_eq!(
                data["qrcode"], "https://github.com/tachibana-shin",
                "QR phải có ở cả 2 nhánh ({status})"
            );
        }
    }

    /// HĐ đã phát hành: PDF xem trước dùng đúng số/ký hiệu/ngày HĐĐT đã ghi,
    /// không dùng số nội bộ (HD…) hay ký hiệu mặc định của hộ.
    #[tokio::test]
    async fn xem_truoc_pdf_da_phat_hanh_dung_thong_tin_hddt() {
        let pool = test_pool().await;
        let id = seed_draft(&pool, "HDHDDT").await;
        sqlx::query(
            "UPDATE invoice
                SET status = 'official', e_invoice_no = '10',
                    e_invoice_symbol = 'C26TST', e_invoice_date = '2026-03-11'
              WHERE id = ?",
        )
        .bind(id)
        .execute(&pool)
        .await
        .expect("ghi nhận HĐĐT");

        let raw = invoice_preview_json(&pool, id)
            .await
            .expect("xem trước được");
        let data: serde_json::Value = serde_json::from_str(&raw).expect("JSON hợp lệ");
        assert_eq!(data["shdon"], "10", "số HĐĐT thay số nội bộ");
        assert_eq!(data["khhdon"], "C26TST", "ký hiệu HĐĐT thay ký hiệu hộ");
        assert_eq!(data["nky"], "2026-03-11", "ngày HĐĐT");
        assert!(data.get("tlhdon").is_none(), "không phải bản nháp");
    }

    /// Không thấy hóa đơn → báo lỗi rõ ràng, không im lặng trả JSON rỗng.
    #[tokio::test]
    async fn xem_truoc_pdf_bao_loi_khi_khong_thay_hoa_don() {
        let pool = test_pool().await;
        let err = invoice_draft_detail_json(&pool, 999_999)
            .await
            .expect_err("hóa đơn không tồn tại phải lỗi");
        assert!(err.contains("999999"), "{err}");
    }

    #[tokio::test]
    async fn xoa_hoa_don_nhap_xoa_ca_dong_hang() {
        let pool = test_pool().await;
        let id = seed_draft(&pool, "HDDEL1").await;

        assert_eq!(delete_invoice_core(&pool, id).await.unwrap(), "HDDEL1");

        let left: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM invoice WHERE id = ?", id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(left, 0, "hóa đơn phải bị xoá");
        let lines: i64 =
            sqlx::query_scalar!("SELECT COUNT(*) FROM invoice_item WHERE invoice_id = ?", id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            lines, 0,
            "dòng hàng phải xoá theo, không để lại dòng mồ côi"
        );
    }

    #[tokio::test]
    async fn xoa_hoa_don_nhap_khong_dung_ton_kho() {
        // Hóa đơn bán không trừ kho: xoá nháp phải không đụng tồn (phiếu xuất mới
        // làm thay đổi tồn), nên số lô vẫn nguyên.
        let pool = test_pool().await;
        let id = seed_draft(&pool, "HDDEL2").await;
        let before: f64 = sqlx::query_scalar!("SELECT quantity FROM stock_lot")
            .fetch_one(&pool)
            .await
            .unwrap();

        delete_invoice_core(&pool, id).await.unwrap();

        let after: f64 = sqlx::query_scalar!("SELECT quantity FROM stock_lot")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(after, before);
    }

    #[tokio::test]
    async fn khong_xoa_duoc_hoa_don_da_xuat_hoac_da_phat_hanh() {
        let pool = test_pool().await;
        let id = seed_draft(&pool, "HDDEL3").await;
        sqlx::query!("UPDATE invoice SET status = 'exported' WHERE id = ?", id)
            .execute(&pool)
            .await
            .unwrap();

        let err = delete_invoice_core(&pool, id).await.unwrap_err();
        assert!(err.contains("Chỉ xoá được hóa đơn nháp"), "{err}");

        // Còn số HĐĐT thì chặn dù trạng thái vẫn là nháp.
        sqlx::query!(
            "UPDATE invoice SET status = 'draft', e_invoice_no = '00000001' WHERE id = ?",
            id
        )
        .execute(&pool)
        .await
        .unwrap();
        let err = delete_invoice_core(&pool, id).await.unwrap_err();
        assert!(err.contains("đã có số HĐĐT"), "{err}");

        let still: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM invoice WHERE id = ?", id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(still, 1, "hóa đơn bị chặn phải còn nguyên");
    }

    #[tokio::test]
    async fn sua_hoa_don_nhap_thay_dong_cu_du_dong_hang() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "HD-EDIT", "Hàng sửa", 0.01).await;
        let w = seed_warehouse(&pool, "W1", "Kho 1").await;
        add_stock_lot(&pool, p, w, 100.0, 1000.0, "2026-01-01").await;
        save_invoice_core(
            &pool,
            "HDE1",
            "2026-03-10",
            "Khách X",
            "MST-X",
            &[line("HD-EDIT", 2.0, 1000.0, 0.0, "W1")],
        )
        .await
        .unwrap();
        let id: i64 =
            sqlx::query_scalar!(r#"SELECT id as "id!" FROM invoice WHERE number = 'HDE1'"#)
                .fetch_one(&pool)
                .await
                .unwrap();

        // Sửa thành 2 dòng, đổi khách/SL/giá.
        let res = update_invoice_core(
            &pool,
            id,
            "HDE1",
            "2026-03-11",
            "Khách Y",
            "MST-Y",
            &[
                line("HD-EDIT", 3.0, 2000.0, 500.0, "W1"),
                line("HD-EDIT", 1.0, 1000.0, 0.0, "W1"),
            ],
        )
        .await
        .unwrap();
        // 3×2000 − 500 = 5500, cộng 1000 = 6500.
        assert_eq!(res["total"].as_f64().unwrap(), 6_500.0);

        let inv = sqlx::query!(
            "SELECT customer, customer_tax_code, date, total FROM invoice WHERE id = ?",
            id
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(inv.customer, "Khách Y");
        assert_eq!(inv.customer_tax_code, "MST-Y");
        assert_eq!(inv.date, "2026-03-11");
        assert_eq!(inv.total, 6_500.0);

        // 2 dòng mới, không dòng cũ sót lại.
        let rows: Vec<(f64, f64, f64, f64)> = sqlx::query!(
            "SELECT quantity, unit_price, discount, subtotal FROM invoice_item
              WHERE invoice_id = ? ORDER BY id",
            id
        )
        .fetch_all(&pool)
        .await
        .unwrap()
        .into_iter()
        .map(|r| (r.quantity, r.unit_price, r.discount, r.subtotal))
        .collect();
        assert_eq!(
            rows,
            vec![(3.0, 2000.0, 500.0, 5500.0), (1.0, 1000.0, 0.0, 1000.0)]
        );
    }

    #[tokio::test]
    async fn sua_hoa_don_nhap_giu_nguyen_trang_thai_nhap() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "HD-EDIT2", "Hàng giữ", 0.01).await;
        let w = seed_warehouse(&pool, "W1", "Kho 1").await;
        add_stock_lot(&pool, p, w, 100.0, 1000.0, "2026-01-01").await;
        save_invoice_core(
            &pool,
            "HDE2",
            "2026-03-10",
            "Khách X",
            "MST-X",
            &[line("HD-EDIT2", 1.0, 1000.0, 0.0, "W1")],
        )
        .await
        .unwrap();
        let id: i64 =
            sqlx::query_scalar!(r#"SELECT id as "id!" FROM invoice WHERE number = 'HDE2'"#)
                .fetch_one(&pool)
                .await
                .unwrap();

        update_invoice_core(
            &pool,
            id,
            "HDE2",
            "2026-03-10",
            "Khách X",
            "MST-X",
            &[line("HD-EDIT2", 5.0, 1000.0, 0.0, "W1")],
        )
        .await
        .unwrap();
        let status: String = sqlx::query_scalar!("SELECT status FROM invoice WHERE id = ?", id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status, "draft", "sửa nháp không được đổi trạng thái");
    }

    #[tokio::test]
    async fn khong_sua_duoc_hoa_don_da_xuat_hoac_da_phat_hanh() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "HD-EDIT3", "Hàng chặn", 0.01).await;
        let w = seed_warehouse(&pool, "W1", "Kho 1").await;
        add_stock_lot(&pool, p, w, 100.0, 1000.0, "2026-01-01").await;
        save_invoice_core(
            &pool,
            "HDE3",
            "2026-03-10",
            "Khách X",
            "MST-X",
            &[line("HD-EDIT3", 1.0, 1000.0, 0.0, "W1")],
        )
        .await
        .unwrap();
        let id: i64 =
            sqlx::query_scalar!(r#"SELECT id as "id!" FROM invoice WHERE number = 'HDE3'"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        sqlx::query!("UPDATE invoice SET status = 'exported' WHERE id = ?", id)
            .execute(&pool)
            .await
            .unwrap();

        let err = update_invoice_core(
            &pool,
            id,
            "HDE3",
            "2026-03-10",
            "Khách X",
            "MST-X",
            &[line("HD-EDIT3", 9.0, 1000.0, 0.0, "W1")],
        )
        .await
        .unwrap_err();
        assert!(err.contains("Chỉ sửa được hóa đơn nháp"), "{err}");

        // Dữ liệu phải còn nguyên sau khi bị chặn.
        let qty: f64 =
            sqlx::query_scalar!("SELECT quantity FROM invoice_item WHERE invoice_id = ?", id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(qty, 1.0);
    }

    /// Sửa thông tin NGOÀI SỔ của hóa đơn đã xử lý cho khớp hóa đơn chính thức
    /// ở hệ thống ký bên kia (app liên kết hoàn toàn thủ công). Sách con
    /// (bút toán + tồn kho) không được đụng tới.
    #[tokio::test]
    async fn sua_header_theo_hd_chinh_thuc_duoc_voi_exported_va_official() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "HD-HE", "Hàng HE", 0.01).await;
        let w = seed_warehouse(&pool, "W-HE", "Kho HE").await;
        add_stock_lot(&pool, p, w, 10.0, 500.0, "2026-01-01").await;
        save_invoice_core(
            &pool,
            "HD0021",
            "2026-05-10",
            "Khách A",
            "MST-A",
            &[line("HD-HE", 1.0, 1000.0, 0.0, "W-HE")],
        )
        .await
        .unwrap();
        let id: i64 =
            sqlx::query_scalar!(r#"SELECT id as "id!" FROM invoice WHERE number = 'HD0021'"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        create_outbound_from_invoice(&pool, id).await.unwrap();
        // Trạng thái "đã chép sang bên kia".
        sqlx::query!("UPDATE invoice SET status = 'exported' WHERE id = ?", id)
            .execute(&pool)
            .await
            .unwrap();

        let res = update_invoice_header_core(
            &pool,
            id,
            "HD0021",
            "2026-05-12",
            "Khách B",
            "MST-B",
            "00000089",
            "1C26TT152",
            "2026-05-12",
            "admin",
        )
        .await
        .unwrap();
        assert_eq!(res["ok"], true);
        let inv = sqlx::query!(
            r#"SELECT number, date, customer, customer_tax_code,
                      e_invoice_no, e_invoice_symbol, e_invoice_date
                 FROM invoice WHERE id = ?"#,
            id
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(inv.customer, "Khách B");
        assert_eq!(inv.customer_tax_code, "MST-B");
        assert_eq!(inv.date, "2026-05-12");
        assert_eq!(inv.e_invoice_no, "00000089");
        assert_eq!(inv.e_invoice_symbol, "1C26TT152");

        // SỔ KHÔNG ĐỤNG: doanh thu và tồn kho y nguyên như trước khi sửa header.
        let revenue: f64 = sqlx::query_scalar!(
            r#"SELECT COALESCE(SUM(amount), 0.0) as "q!: f64" FROM journal_entry WHERE entry_type = 'PX'"#
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(revenue, 1000.0);
        let stock: f64 = sqlx::query_scalar!(
            "SELECT COALESCE(SUM(quantity), 0.0) as \"q!: f64\" FROM stock_lot WHERE product_id = ?",
            p
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(stock, 9.0);

        // official (đã ghi số HĐĐT) vẫn sửa được — ghi nhầm số thì sửa tại chỗ.
        link_hddt_core(&pool, id, "00000090", "1C26TT152", "2026-05-12")
            .await
            .unwrap();
        let res = update_invoice_header_core(
            &pool,
            id,
            "HD0021",
            "2026-05-12",
            "Khách B",
            "MST-B",
            "00000091",
            "1C26TT152",
            "2026-05-12",
            "admin",
        )
        .await
        .unwrap();
        assert_eq!(res["ok"], true);
        let e_no: String = sqlx::query_scalar!("SELECT e_invoice_no FROM invoice WHERE id = ?", id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(e_no, "00000091");
    }

    /// Hóa đơn đã có phiếu xuất: ngày chỉ đổi trong cùng kỳ (doanh thu đang kê
    /// ở kỳ của ngày phiếu) — khác kỳ phải đi phiếu điều chỉnh. Chưa có phiếu
    /// thì ngày đổi tự do vì chưa có bút toán nào chịu ngày này.
    #[tokio::test]
    async fn sua_header_ngay_gioi_han_theo_phoieu_xuat() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "HD-HE2", "Hàng HE2", 0.01).await;
        let w = seed_warehouse(&pool, "W-HE2", "Kho HE2").await;
        add_stock_lot(&pool, p, w, 10.0, 500.0, "2026-01-01").await;
        save_invoice_core(
            &pool,
            "HD0031",
            "2026-05-10",
            "Khách A",
            "",
            &[line("HD-HE2", 1.0, 1000.0, 0.0, "W-HE2")],
        )
        .await
        .unwrap();
        let with_px: i64 =
            sqlx::query_scalar!(r#"SELECT id as "id!" FROM invoice WHERE number = 'HD0031'"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        create_outbound_from_invoice(&pool, with_px).await.unwrap();
        sqlx::query!(
            "UPDATE invoice SET status = 'exported' WHERE id = ?",
            with_px
        )
        .execute(&pool)
        .await
        .unwrap();
        save_invoice_core(
            &pool,
            "HD0032",
            "2026-05-11",
            "Khách B",
            "",
            &[line("HD-HE2", 1.0, 1000.0, 0.0, "W-HE2")],
        )
        .await
        .unwrap();
        let no_px: i64 =
            sqlx::query_scalar!(r#"SELECT id as "id!" FROM invoice WHERE number = 'HD0032'"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        sqlx::query!("UPDATE invoice SET status = 'exported' WHERE id = ?", no_px)
            .execute(&pool)
            .await
            .unwrap();

        // Có phiếu mà đổi ngày sang tháng khác → chặn, phải đi phiếu điều chỉnh.
        let err = update_invoice_header_core(
            &pool,
            with_px,
            "HD0031",
            "2026-06-10",
            "Khách A",
            "",
            "",
            "",
            "",
            "admin",
        )
        .await
        .unwrap_err();
        assert!(err.contains("phiếu điều chỉnh"), "{err}");

        // Cùng kỳ → qua và có cảnh báo ngày phiếu giữ nguyên.
        let res = update_invoice_header_core(
            &pool,
            with_px,
            "HD0031",
            "2026-05-20",
            "Khách A",
            "",
            "",
            "",
            "",
            "admin",
        )
        .await
        .unwrap();
        assert_eq!(res["ok"], true);
        assert!(!res["warning"].as_str().unwrap_or_default().is_empty());

        // Chưa có phiếu xuất → ngày khác kỳ vẫn cho (chưa có bút toán nào liên quan).
        let res = update_invoice_header_core(
            &pool,
            no_px,
            "HD0032",
            "2026-08-01",
            "Khách B",
            "",
            "",
            "",
            "",
            "admin",
        )
        .await
        .unwrap();
        assert_eq!(res["ok"], true);
        assert_eq!(res["warning"].as_str().unwrap_or_default(), "");
    }

    /// Chặn các cửa sai: số đã bị hóa đơn khác chiếm; nháp thì chỉ qua Sửa nháp;
    /// đã bị thay thế thì không sửa được; lưu mà không đổi gì thì báo.
    #[tokio::test]
    async fn sua_header_chan_so_trung_va_trang_thai_khong_hop_le() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "HD-HE3", "Hàng HE3", 0.01).await;
        let w = seed_warehouse(&pool, "W-HE3", "Kho HE3").await;
        add_stock_lot(&pool, p, w, 5.0, 500.0, "2026-01-01").await;
        for (no, date) in [("HD0041", "2026-05-10"), ("HD0042", "2026-05-11")] {
            save_invoice_core(
                &pool,
                no,
                date,
                "Khách",
                "",
                &[line("HD-HE3", 1.0, 1000.0, 0.0, "W-HE3")],
            )
            .await
            .unwrap();
        }
        let id: i64 =
            sqlx::query_scalar!(r#"SELECT id as "id!" FROM invoice WHERE number = 'HD0041'"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        sqlx::query!("UPDATE invoice SET status = 'exported' WHERE id = ?", id)
            .execute(&pool)
            .await
            .unwrap();

        // Số mới đã bị hóa đơn khác chiếm.
        let err = update_invoice_header_core(
            &pool,
            id,
            "HD0042",
            "2026-05-10",
            "Khách",
            "",
            "",
            "",
            "",
            "admin",
        )
        .await
        .unwrap_err();
        assert!(err.contains("đã tồn tại"), "{err}");

        // Nháp → chỉ cho qua đường Sửa hóa đơn nháp.
        let draft: i64 =
            sqlx::query_scalar!(r#"SELECT id as "id!" FROM invoice WHERE number = 'HD0042'"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        let err = update_invoice_header_core(
            &pool,
            draft,
            "HD0042",
            "2026-05-11",
            "Khách",
            "",
            "",
            "",
            "",
            "admin",
        )
        .await
        .unwrap_err();
        assert!(err.contains("nháp"), "{err}");

        // Đã bị thay thế → đóng cửa.
        sqlx::query!("UPDATE invoice SET status = 'replaced' WHERE id = ?", id)
            .execute(&pool)
            .await
            .unwrap();
        let err = update_invoice_header_core(
            &pool,
            id,
            "HD0041",
            "2026-05-10",
            "Khách",
            "",
            "",
            "",
            "",
            "admin",
        )
        .await
        .unwrap_err();
        assert!(err.contains("không sửa được"), "{err}");

        // Lưu mà không đổi gì → báo rõ, không phết nhật ký vô nghĩa.
        sqlx::query!("UPDATE invoice SET status = 'official' WHERE id = ?", id)
            .execute(&pool)
            .await
            .unwrap();
        let cur = sqlx::query!(
            r#"SELECT number, date, customer FROM invoice WHERE id = ?"#,
            id
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let err = update_invoice_header_core(
            &pool,
            id,
            &cur.number,
            &cur.date,
            &cur.customer,
            "",
            "",
            "",
            "",
            "admin",
        )
        .await
        .unwrap_err();
        assert!(err.contains("Không có gì để đổi"), "{err}");
    }

    /// Sửa header để lại dấu vết trong invoice_event: từ trạng thái → chính trạng
    /// thái đó, ghi rõ từng trường đổi — đối chiếu với hóa đơn chính thức sau này.
    #[tokio::test]
    async fn sua_header_ghi_su_kien_de_doi_soat() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "HD-HE4", "Hàng HE4", 0.01).await;
        let w = seed_warehouse(&pool, "W-HE4", "Kho HE4").await;
        add_stock_lot(&pool, p, w, 5.0, 500.0, "2026-01-01").await;
        save_invoice_core(
            &pool,
            "HD0051",
            "2026-05-10",
            "Khách A",
            "MST-A",
            &[line("HD-HE4", 1.0, 1000.0, 0.0, "W-HE4")],
        )
        .await
        .unwrap();
        let id: i64 =
            sqlx::query_scalar!(r#"SELECT id as "id!" FROM invoice WHERE number = 'HD0051'"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        sqlx::query!("UPDATE invoice SET status = 'official' WHERE id = ?", id)
            .execute(&pool)
            .await
            .unwrap();
        update_invoice_header_core(
            &pool,
            id,
            "HD0051",
            "2026-05-10",
            "Khách Mới",
            "MST-M",
            "00000100",
            "1C26TT152",
            "2026-05-10",
            "ketoan1",
        )
        .await
        .unwrap();

        let ev = sqlx::query!(
            r#"SELECT from_status, to_status, reason, actor, ref
                 FROM invoice_event WHERE invoice_id = ? ORDER BY id DESC LIMIT 1"#,
            id
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(ev.from_status, "official");
        assert_eq!(ev.to_status, "official");
        assert_eq!(ev.actor, "ketoan1");
        assert_eq!(ev.r#ref, "00000100");
        assert!(
            ev.reason.contains("Sửa theo HĐ chính thức"),
            "{}",
            ev.reason
        );
        assert!(ev.reason.contains("Khách hàng"), "{}", ev.reason);
        assert!(ev.reason.contains("MST"), "{}", ev.reason);
        assert!(ev.reason.contains("Số HĐĐT"), "{}", ev.reason);
        assert!(ev.reason.contains("00000100"), "{}", ev.reason);
    }

    /// Doanh thu NET trên cặp phiếu gốc + {v}DT (tăng − giảm) — đúng cách tờ
    /// khai tổng hợp từ journal.
    async fn revenue_net(pool: &SqlitePool, v: &str, vdt: &str) -> f64 {
        sqlx::query_scalar!(
            r#"SELECT COALESCE(SUM(CASE
                     WHEN entry_type = 'PX' AND credit_account = '511' THEN amount
                     WHEN entry_type = 'PX' AND debit_account = '511' THEN -amount
                     ELSE 0.0 END), 0.0) as "net!: f64"
                 FROM journal_entry WHERE voucher_no IN (?, ?)"#,
            v,
            vdt
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    async fn stock_qty(pool: &SqlitePool, product_id: i64) -> f64 {
        sqlx::query_scalar!(
            "SELECT COALESCE(SUM(quantity), 0.0) as \"q!: f64\" FROM stock_lot WHERE product_id = ?",
            product_id
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    /// Có phiếu xuất, sửa ĐƠN GIÁ (SL giữ nguyên) → ghi phiếu {v}DT: đảo toàn
    /// bộ doanh thu cũ + ghi lại theo HĐ chính thức. Tồn kho và giá vốn KHÔNG
    /// đổi vì số lượng không đổi.
    #[tokio::test]
    async fn sua_dong_hang_dao_doanh_thu_va_ghi_lai_tren_phoieu() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "HD-LN1", "Hàng LN1", 0.01).await;
        let w = seed_warehouse(&pool, "W-LN1", "Kho LN1").await;
        add_stock_lot(&pool, p, w, 10.0, 500.0, "2026-01-01").await;
        save_invoice_core(
            &pool,
            "HD0101",
            "2026-05-10",
            "Khách LN",
            "MST-LN",
            &[line("HD-LN1", 2.0, 1000.0, 0.0, "W-LN1")],
        )
        .await
        .unwrap();
        let id: i64 =
            sqlx::query_scalar!(r#"SELECT id as "id!" FROM invoice WHERE number = 'HD0101'"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        create_outbound_from_invoice(&pool, id).await.unwrap();
        sqlx::query!(
            "UPDATE invoice SET status = 'exported', e_invoice_no = '00000009' WHERE id = ?",
            id
        )
        .execute(&pool)
        .await
        .unwrap();
        let v: String = sqlx::query_scalar!("SELECT voucher_no FROM invoice WHERE id = ?", id)
            .fetch_one(&pool)
            .await
            .unwrap();
        let vdt = format!("{v}DT");

        // Đơn giá 1000 → 1200, SL giữ 2 → tổng 2400.
        let res = update_invoice_lines_core(
            &pool,
            id,
            &[line("HD-LN1", 2.0, 1200.0, 0.0, "W-LN1")],
            "admin",
        )
        .await
        .unwrap();
        assert_eq!(res["ok"], true);
        assert_eq!(res["voucher_dt"].as_str().unwrap(), vdt);
        // Sổ ghi chứng từ sửa theo số HĐĐT, không phải số hóa đơn nội bộ.
        let descs: Vec<String> = sqlx::query_scalar(
            r#"SELECT description FROM journal_entry
                WHERE voucher_no = ? AND entry_type = 'PX' ORDER BY id"#,
        )
        .bind(&vdt)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert!(!descs.is_empty());
        assert!(
            descs
                .iter()
                .all(|d| d.contains("00000009") && !d.contains("HD0101")),
            "diễn giải phải dùng số HĐĐT: {descs:?}"
        );

        let total: f64 = sqlx::query_scalar!("SELECT total FROM invoice WHERE id = ?", id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(total, 2400.0);
        // Đảo + ghi lại trên {v}DT → net đúng tổng mới, bản in phiếu gốc giữ nguyên.
        assert_eq!(revenue_net(&pool, &v, &vdt).await, 2400.0);
        // SL không đổi → tồn kho y nguyên (8 sau khi xuất 2) và không có dòng
        // giá vốn mới (chỉ còn dòng của phiếu gốc).
        assert_eq!(stock_qty(&pool, p).await, 8.0);
        let cp_rows: i64 = sqlx::query_scalar!(
            r#"SELECT COUNT(*) as "n!"
                 FROM journal_entry WHERE voucher_no IN (?, ?) AND entry_type = 'CP'"#,
            v,
            vdt
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(cp_rows, 1, "giá vốn giữ nguyên — không sinh dòng CP mới");
    }

    /// Sửa SỐ LƯỢNG có phiếu: tăng → FIFO xuất thêm + bù giá vốn đúng lượng;
    /// giảm → nhập lại lô theo đúng đơn giá vốn đã ghi ở phiếu gốc.
    #[tokio::test]
    async fn sua_dong_hang_tang_sl_fifo_va_giam_sl_hoi_kho_theo_gia_von() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "HD-LN2", "Hàng LN2", 0.01).await;
        let w = seed_warehouse(&pool, "W-LN2", "Kho LN2").await;
        add_stock_lot(&pool, p, w, 10.0, 500.0, "2026-01-01").await;
        save_invoice_core(
            &pool,
            "HD0102",
            "2026-05-10",
            "Khách LN",
            "",
            &[line("HD-LN2", 2.0, 1000.0, 0.0, "W-LN2")],
        )
        .await
        .unwrap();
        let id: i64 =
            sqlx::query_scalar!(r#"SELECT id as "id!" FROM invoice WHERE number = 'HD0102'"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        create_outbound_from_invoice(&pool, id).await.unwrap();
        sqlx::query!("UPDATE invoice SET status = 'official' WHERE id = ?", id)
            .execute(&pool)
            .await
            .unwrap();
        let v: String = sqlx::query_scalar!("SELECT voucher_no FROM invoice WHERE id = ?", id)
            .fetch_one(&pool)
            .await
            .unwrap();
        let vdt = format!("{v}DT");

        // Tăng 2 → 5: FIFO rút thêm 3 (giá vốn 3 × 500 = 1500).
        let res = update_invoice_lines_core(
            &pool,
            id,
            &[line("HD-LN2", 5.0, 1000.0, 0.0, "W-LN2")],
            "admin",
        )
        .await
        .unwrap();
        assert_eq!(res["ok"], true);
        assert_eq!(stock_qty(&pool, p).await, 5.0);
        assert_eq!(revenue_net(&pool, &v, &vdt).await, 5000.0);
        let cp_dt: (f64, f64) = sqlx::query_as::<_, (f64, f64)>(
            r#"SELECT COALESCE(SUM(quantity), 0.0), COALESCE(SUM(amount), 0.0)
                 FROM journal_entry WHERE voucher_no = ? AND entry_type = 'CP'"#,
        )
        .bind(&vdt)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(cp_dt, (3.0, 1500.0), "bù giá vốn đúng 3 × 500");

        // Giảm 5 → 1: nhập lại 4 về kho đúng giá vốn gốc 500 (từ CP phiếu gốc).
        let res = update_invoice_lines_core(
            &pool,
            id,
            &[line("HD-LN2", 1.0, 1000.0, 0.0, "W-LN2")],
            "admin",
        )
        .await
        .unwrap();
        assert_eq!(res["ok"], true);
        // 10 − 2 (lần đầu) − 3 (tăng) + 4 (hồi kho) = 9 = 10 − SL cuối (1).
        assert_eq!(stock_qty(&pool, p).await, 9.0);
        assert_eq!(revenue_net(&pool, &v, &vdt).await, 1000.0);
        let back = sqlx::query!(
            r#"SELECT quantity, unit_cost FROM stock_lot
                WHERE product_id = ? ORDER BY id DESC LIMIT 1"#,
            p
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(back.quantity, 4.0);
        assert_eq!(back.unit_cost, 500.0, "giá vốn hồi kho = giá vốn phiếu gốc");
    }

    /// Tăng SL nhưng kho không đủ → CHẶN toàn bộ (không nửa vời ghi sách),
    /// chỉ đường nhập hàng / phiếu điều chỉnh.
    #[tokio::test]
    async fn sua_dong_hang_tang_sl_thieu_ton_bi_chan() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "HD-LN3", "Hàng LN3", 0.01).await;
        let w = seed_warehouse(&pool, "W-LN3", "Kho LN3").await;
        add_stock_lot(&pool, p, w, 10.0, 500.0, "2026-01-01").await;
        save_invoice_core(
            &pool,
            "HD0103",
            "2026-05-10",
            "Khách",
            "",
            &[line("HD-LN3", 2.0, 1000.0, 0.0, "W-LN3")],
        )
        .await
        .unwrap();
        let id: i64 =
            sqlx::query_scalar!(r#"SELECT id as "id!" FROM invoice WHERE number = 'HD0103'"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        create_outbound_from_invoice(&pool, id).await.unwrap();
        sqlx::query!("UPDATE invoice SET status = 'exported' WHERE id = ?", id)
            .execute(&pool)
            .await
            .unwrap();
        let v: String = sqlx::query_scalar!("SELECT voucher_no FROM invoice WHERE id = ?", id)
            .fetch_one(&pool)
            .await
            .unwrap();

        // Kho còn 8; sửa lên 15 → cần thêm 13 → thiếu 5.
        let err = update_invoice_lines_core(
            &pool,
            id,
            &[line("HD-LN3", 15.0, 1000.0, 0.0, "W-LN3")],
            "admin",
        )
        .await
        .unwrap_err();
        assert!(err.contains("thiếu"), "{err}");

        // Bị chặn thì mọi thứ phải còn nguyên.
        let total: f64 = sqlx::query_scalar!("SELECT total FROM invoice WHERE id = ?", id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(total, 2000.0);
        assert_eq!(stock_qty(&pool, p).await, 8.0);
        let vdt = format!("{v}DT");
        let dt_rows: i64 = sqlx::query_scalar!(
            r#"SELECT COUNT(*) as "n!" FROM journal_entry WHERE voucher_no = ?"#,
            vdt
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(dt_rows, 0, "không được sinh chứng từ nào khi bị chặn");
    }

    /// Chưa lập phiếu xuất → chưa có bút toán nào: chỉ thay dòng + tổng,
    /// không sinh chứng từ.
    #[tokio::test]
    async fn sua_dong_hang_khong_phoieu_chi_thay_dong_va_tong() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "HD-LN4", "Hàng LN4", 0.01).await;
        let w = seed_warehouse(&pool, "W-LN4", "Kho LN4").await;
        add_stock_lot(&pool, p, w, 10.0, 500.0, "2026-01-01").await;
        save_invoice_core(
            &pool,
            "HD0104",
            "2026-05-10",
            "Khách",
            "",
            &[line("HD-LN4", 2.0, 1000.0, 0.0, "W-LN4")],
        )
        .await
        .unwrap();
        let id: i64 =
            sqlx::query_scalar!(r#"SELECT id as "id!" FROM invoice WHERE number = 'HD0104'"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        sqlx::query!("UPDATE invoice SET status = 'exported' WHERE id = ?", id)
            .execute(&pool)
            .await
            .unwrap();
        let journal_before: i64 =
            sqlx::query_scalar!(r#"SELECT COUNT(*) as "n!" FROM journal_entry"#)
                .fetch_one(&pool)
                .await
                .unwrap();

        let res = update_invoice_lines_core(
            &pool,
            id,
            &[line("HD-LN4", 3.0, 500.0, 0.0, "W-LN4")],
            "admin",
        )
        .await
        .unwrap();
        assert_eq!(res["ok"], true);
        assert_eq!(res["voucher_dt"].as_str().unwrap(), "");
        assert_eq!(res["booked"], 0);
        let total: f64 = sqlx::query_scalar!("SELECT total FROM invoice WHERE id = ?", id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(total, 1500.0);
        let journal_after: i64 =
            sqlx::query_scalar!(r#"SELECT COUNT(*) as "n!" FROM journal_entry"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            journal_before, journal_after,
            "không có bút toán khi chưa phiếu"
        );
    }

    /// Nháp chỉ qua Sửa nháp; hóa đơn đã bị thay thế thì đóng hẳn.
    #[tokio::test]
    async fn sua_dong_hang_chan_hoa_don_nhap_va_da_thay_the() {
        let pool = test_pool().await;
        let _p = seed_product(&pool, "HD-LN5", "Hàng LN5", 0.01).await;
        let _w = seed_warehouse(&pool, "W-LN5", "Kho LN5").await;
        save_invoice_core(
            &pool,
            "HD0105",
            "2026-05-10",
            "Khách",
            "",
            &[line("HD-LN5", 1.0, 1000.0, 0.0, "W-LN5")],
        )
        .await
        .unwrap();
        let id: i64 =
            sqlx::query_scalar!(r#"SELECT id as "id!" FROM invoice WHERE number = 'HD0105'"#)
                .fetch_one(&pool)
                .await
                .unwrap();

        let err = update_invoice_lines_core(
            &pool,
            id,
            &[line("HD-LN5", 2.0, 1000.0, 0.0, "W-LN5")],
            "admin",
        )
        .await
        .unwrap_err();
        assert!(err.contains("nháp"), "{err}");

        sqlx::query!("UPDATE invoice SET status = 'replaced' WHERE id = ?", id)
            .execute(&pool)
            .await
            .unwrap();
        let err = update_invoice_lines_core(
            &pool,
            id,
            &[line("HD-LN5", 2.0, 1000.0, 0.0, "W-LN5")],
            "admin",
        )
        .await
        .unwrap_err();
        assert!(err.contains("không sửa được"), "{err}");
    }

    /// Khoảng kỳ tờ khai đã đánh dấu ĐÃ NỘP (bấm trên màn Tờ khai): hóa đơn
    /// trong kỳ VẪN sửa được (app không liên kết CQT) nhưng mọi thao tác phải
    /// kèm cảnh báo để người dùng tự kê khai bổ sung khi số liệu đã gửi lệch.
    #[tokio::test]
    async fn sua_hoa_don_trong_ky_da_nop_to_khai_van_duoc_canh_bao() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "HD-LN6", "Hàng LN6", 0.01).await;
        let w = seed_warehouse(&pool, "W-LN6", "Kho LN6").await;
        add_stock_lot(&pool, p, w, 10.0, 500.0, "2026-01-01").await;
        save_invoice_core(
            &pool,
            "HD0106",
            "2026-05-10",
            "Khách A",
            "MST-A",
            &[line("HD-LN6", 2.0, 1000.0, 0.0, "W-LN6")],
        )
        .await
        .unwrap();
        let id: i64 =
            sqlx::query_scalar!(r#"SELECT id as "id!" FROM invoice WHERE number = 'HD0106'"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        sqlx::query!("UPDATE invoice SET status = 'exported' WHERE id = ?", id)
            .execute(&pool)
            .await
            .unwrap();

        // Chưa đóng kỳ → không có cảnh báo "ĐÃ NỘP".
        let res = update_invoice_header_core(
            &pool,
            id,
            "HD0106",
            "2026-05-10",
            "Khách B",
            "",
            "",
            "",
            "",
            "admin",
        )
        .await
        .unwrap();
        assert!(!res["warning"]
            .as_str()
            .unwrap_or_default()
            .contains("ĐÃ NỘP"));

        // Đánh dấu kỳ 2026-05 đã nộp (mô phỏng bấm trên màn Tờ khai).
        sqlx::query!(
            r#"INSERT INTO tax_period_closed (from_date, to_date, note, actor)
               VALUES ('2026-05-01', '2026-05-31', 'đã gửi CQT', 'admin')"#
        )
        .execute(&pool)
        .await
        .unwrap();

        // Sửa header trong kỳ → VẪN sửa được + kèm cảnh báo đóng kỳ.
        let res = update_invoice_header_core(
            &pool,
            id,
            "HD0106",
            "2026-05-12",
            "Khách C",
            "",
            "",
            "",
            "",
            "admin",
        )
        .await
        .unwrap();
        assert_eq!(res["ok"], true);
        assert!(
            res["warning"]
                .as_str()
                .unwrap_or_default()
                .contains("ĐÃ NỘP"),
            "{:?}",
            res
        );

        // Sửa dòng hàng trong kỳ → cũng được + cảnh báo.
        let res = update_invoice_lines_core(
            &pool,
            id,
            &[line("HD-LN6", 2.0, 1200.0, 0.0, "W-LN6")],
            "admin",
        )
        .await
        .unwrap();
        assert_eq!(res["ok"], true);
        assert!(
            res["warning"]
                .as_str()
                .unwrap_or_default()
                .contains("ĐÃ NỘP"),
            "{:?}",
            res
        );

        // Đưa ngày sang NGOÀI kỳ (chưa phiếu → ngày tự do) → hết cảnh báo.
        let res = update_invoice_header_core(
            &pool,
            id,
            "HD0106",
            "2026-07-10",
            "Khách C",
            "",
            "",
            "",
            "",
            "admin",
        )
        .await
        .unwrap();
        assert_eq!(res["ok"], true);
        assert!(
            !res["warning"]
                .as_str()
                .unwrap_or_default()
                .contains("ĐÃ NỘP"),
            "{:?}",
            res
        );
    }

    #[tokio::test]
    async fn sua_hoa_don_nhap_van_bao_danh_sach_thieu_ton() {
        // Sửa số lượng vượt tồn: nháp vẫn sửa được nhưng phải báo thiếu.
        let pool = test_pool().await;
        let p = seed_product(&pool, "HD-EDIT4", "Hàng ít tồn", 0.01).await;
        let w = seed_warehouse(&pool, "W1", "Kho 1").await;
        add_stock_lot(&pool, p, w, 5.0, 1000.0, "2026-01-01").await;
        save_invoice_core(
            &pool,
            "HDE4",
            "2026-03-10",
            "Khách X",
            "MST-X",
            &[line("HD-EDIT4", 2.0, 1000.0, 0.0, "W1")],
        )
        .await
        .unwrap();
        let id: i64 =
            sqlx::query_scalar!(r#"SELECT id as "id!" FROM invoice WHERE number = 'HDE4'"#)
                .fetch_one(&pool)
                .await
                .unwrap();

        let res = update_invoice_core(
            &pool,
            id,
            "HDE4",
            "2026-03-10",
            "Khách X",
            "MST-X",
            &[line("HD-EDIT4", 99.0, 1000.0, 0.0, "W1")],
        )
        .await
        .expect("sửa nháp được lưu dù thiếu tồn");
        assert!(
            !res["shortage"]
                .as_array()
                .map(|v| v.is_empty())
                .unwrap_or(true),
            "phải báo thiếu: {res}"
        );

        // Còn ghi số HĐĐT khi vẫn thiếu thì bị chặn.
        let err = link_hddt_core(&pool, id, "00000020", "1C26TT152", "2026-03-10")
            .await
            .unwrap_err();
        assert!(err.contains("Không đủ tồn kho"), "{err}");
    }

    #[tokio::test]
    async fn lien_ket_hddt_luu_ky_hieu_vao_ho_so_hoi_kinh_doanh() {
        let pool = test_pool().await;
        let id = seed_draft(&pool, "HDSYM1").await;

        link_hddt_core(&pool, id, "00000123", "1C26TT152", "2026-09-26")
            .await
            .unwrap();

        let inv = sqlx::query!(
            "SELECT status, e_invoice_no, e_invoice_symbol FROM invoice WHERE id = ?",
            id
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(inv.status, "official");
        assert_eq!(inv.e_invoice_no, "00000123");
        assert_eq!(inv.e_invoice_symbol, "1C26TT152");

        // Ký hiệu được lưu vào hồ sơ làm mặc định cho lần sau.
        let symbol: String = sqlx::query_scalar!("SELECT hddt_symbol FROM business WHERE id = 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(symbol, "1C26TT152");

        // Đổi mẫu số theo năm: ký hiệu mới ghi đè ký hiệu cũ (đã cắt khoảng trắng).
        let id2 = seed_draft(&pool, "HDSYM2").await;
        link_hddt_core(&pool, id2, "00000001", " 1C27TT152 ", "2027-01-05")
            .await
            .unwrap();
        let symbol: String = sqlx::query_scalar!("SELECT hddt_symbol FROM business WHERE id = 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(symbol, "1C27TT152");

        // Gõ ký hiệu rỗng thì không đụng ký hiệu đang lưu trong hồ sơ.
        let id3 = seed_draft(&pool, "HDSYM3").await;
        link_hddt_core(&pool, id3, "00000002", "", "2027-01-06")
            .await
            .unwrap();
        let symbol: String = sqlx::query_scalar!("SELECT hddt_symbol FROM business WHERE id = 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            symbol, "1C27TT152",
            "ký hiệu rỗng không được xoá ký hiệu đã lưu"
        );
    }

    #[tokio::test]
    async fn lien_ket_hddt_dong_bo_ngay_hoa_don_khi_khong_co_phieu_xuat() {
        // Nháp lập ở màn Hóa đơn: không có phiếu xuất, sổ chưa ghi gì → ngày hóa
        // đơn phải theo ngày trên HĐĐT.
        let pool = test_pool().await;
        let id = seed_draft(&pool, "HDSY1").await;

        let r = link_hddt_core(&pool, id, "00000123", "1C26TT152", "2026-03-30")
            .await
            .unwrap();
        assert_eq!(r["date_synced"], json!(true));
        assert_eq!(r["old_date"], json!("2026-03-10"));
        assert_eq!(r["warning"], json!(""));

        let inv = sqlx::query!(
            "SELECT date, e_invoice_date, status FROM invoice WHERE id = ?",
            id
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(inv.date, "2026-03-30", "ngày hóa đơn phải theo ngày HĐĐT");
        assert_eq!(inv.e_invoice_date, "2026-03-30");
        assert_eq!(inv.status, "official");

        // Ghi vào lịch sử để đối chiếu.
        let reason: String = sqlx::query_scalar!(
            "SELECT reason FROM invoice_event WHERE invoice_id = ? ORDER BY id DESC LIMIT 1",
            id
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(reason.contains("2026-03-30"), "{reason}");
    }

    #[tokio::test]
    async fn lien_ket_hddt_khong_doi_ngay_khi_hoa_don_da_gan_phieu_xuat() {
        // Lập kèm ở màn Xuất kho: doanh thu đã ghi sổ theo ngày phiếu → không tự
        // đổi ngày hóa đơn, phải cảnh báo lệch (nhấn mạnh khi lệch kỳ).
        let pool = test_pool().await;
        let id = seed_draft(&pool, "HDSY2").await;
        sqlx::query!(
            "UPDATE invoice SET voucher_no = 'PX000123' WHERE id = ?",
            id
        )
        .execute(&pool)
        .await
        .unwrap();

        // Lệch trong cùng tháng → nhắc nhẹ.
        let r = link_hddt_core(&pool, id, "00000124", "1C26TT152", "2026-03-30")
            .await
            .unwrap();
        assert_eq!(r["date_synced"], json!(false));
        assert!(r["warning"].as_str().unwrap().contains("PX000123"));

        let date: String = sqlx::query_scalar!("SELECT date FROM invoice WHERE id = ?", id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(date, "2026-03-10", "không được tự đổi ngày phiếu xuất");

        // Lệch qua ranh giới tháng → cảnh báo nói rõ lệch kỳ.
        let id2 = seed_draft(&pool, "HDSY3").await;
        sqlx::query!(
            "UPDATE invoice SET voucher_no = 'PX000124' WHERE id = ?",
            id2
        )
        .execute(&pool)
        .await
        .unwrap();
        let r = link_hddt_core(&pool, id2, "00000125", "1C26TT152", "2026-04-01")
            .await
            .unwrap();
        let w = r["warning"].as_str().unwrap();
        assert!(w.contains("Lệch KỲ"), "{w}");
        assert!(w.contains("điều chỉnh"), "{w}");
    }

    #[tokio::test]
    async fn lien_ket_hddt_ngay_giong_nhau_thi_khong_doi_gi() {
        let pool = test_pool().await;
        let id = seed_draft(&pool, "HDSY4").await;
        let r = link_hddt_core(&pool, id, "00000126", "1C26TT152", "2026-03-10")
            .await
            .unwrap();
        assert_eq!(r["date_synced"], json!(false));
        assert_eq!(r["warning"], json!(""));
        let date: String = sqlx::query_scalar!("SELECT date FROM invoice WHERE id = ?", id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(date, "2026-03-10");
    }

    /// Hóa đơn nháp + tồn kho → tạo phiếu xuất phải trừ tồn, ghi Nợ 131/Có 511
    /// và liên kết 1-1 với hóa đơn.
    #[tokio::test]
    async fn tao_phieu_xuat_tu_hoa_don_ghi_so_troi_ton() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "HD-PX1", "Hàng xuất", 0.01).await;
        let w = seed_warehouse(&pool, "W-PX1", "Kho PX").await;
        add_stock_lot(&pool, p, w, 10.0, 1000.0, "2026-01-01").await;
        save_invoice_core(
            &pool,
            "HDPX1",
            "2026-03-10",
            "Khách X",
            "MST-X",
            &[
                line("HD-PX1", 2.0, 1000.0, 0.0, "W-PX1"),
                line("HD-PX1", 1.0, 2000.0, 500.0, "W-PX1"),
            ],
        )
        .await
        .unwrap();
        let id: i64 =
            sqlx::query_scalar!(r#"SELECT id as "id!" FROM invoice WHERE number = 'HDPX1'"#)
                .fetch_one(&pool)
                .await
                .unwrap();

        let px = create_outbound_from_invoice(&pool, id).await.unwrap();
        assert_eq!(px, "PX001");

        // Tồn còn 10 − 3 = 7.
        let qty: f64 = sqlx::query_scalar!(
            "SELECT quantity FROM stock_lot WHERE product_id = ? AND depleted = 0",
            p
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(qty, 7.0);

        // Doanh thu ghi sổ = 2×1000 + (1×2000 − 500) = 3500 (có chiết khấu trên dòng).
        let rev: f64 = sqlx::query_scalar!(
            "SELECT COALESCE(SUM(amount), 0.0) as \"q!: f64\" FROM journal_entry
              WHERE entry_type = 'PX' AND debit_account = '131'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(rev, 3_500.0);
        let credit: String = sqlx::query_scalar!(
            "SELECT credit_account FROM journal_entry WHERE entry_type = 'PX' LIMIT 1"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(credit, "511");

        // Liên kết 1-1: hóa đơn đã trỏ phiếu, tạo lần 2 bị chặn.
        let voucher: String =
            sqlx::query_scalar!("SELECT voucher_no FROM invoice WHERE id = ?", id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(voucher, "PX001");
        let err = create_outbound_from_invoice(&pool, id).await.unwrap_err();
        assert!(err.contains("đã có phiếu xuất"), "{err}");
    }

    #[tokio::test]
    async fn tao_phieu_xuat_thieu_ton_thi_khong_duoc_tao() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "HD-PX2", "Hàng thiếu tồn", 0.01).await;
        let w = seed_warehouse(&pool, "W-PX2", "Kho PX2").await;
        add_stock_lot(&pool, p, w, 5.0, 1000.0, "2026-01-01").await;
        save_invoice_core(
            &pool,
            "HDPX2",
            "2026-03-10",
            "Khách X",
            "MST-X",
            &[line("HD-PX2", 5.0, 1000.0, 0.0, "W-PX2")],
        )
        .await
        .unwrap();
        // Tồn bị xuất mất ở nơi khác sau khi lập nháp (giả lập bán ngoài app) →
        // lúc tạo phiếu xuất sẽ thiếu tồn.
        sqlx::query!(
            "UPDATE stock_lot SET quantity = 1.0 WHERE product_id = ?",
            p
        )
        .execute(&pool)
        .await
        .unwrap();
        let id: i64 =
            sqlx::query_scalar!(r#"SELECT id as "id!" FROM invoice WHERE number = 'HDPX2'"#)
                .fetch_one(&pool)
                .await
                .unwrap();

        let err = create_outbound_from_invoice(&pool, id).await.unwrap_err();
        assert!(err.contains("Không đủ tồn kho"), "{err}");

        // Không để lại trạng thái lệch sổ: hóa đơn chưa gắn phiếu, sổ chưa có PX.
        let voucher: String =
            sqlx::query_scalar!("SELECT voucher_no FROM invoice WHERE id = ?", id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(voucher, "");
        let px_count: i64 =
            sqlx::query_scalar!("SELECT COUNT(*) FROM journal_entry WHERE entry_type = 'PX'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(px_count, 0, "phiếu lỗi phải rollback trọn vẹn");
    }

    /// Báo cáo số hóa đơn trùng để người dùng tự dọn (app không tự sửa).
    #[tokio::test]
    async fn bao_cao_so_hoa_don_trung() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "HD-DUP", "Hàng", 0.01).await;
        let w = seed_warehouse(&pool, "W-DUP", "Kho trùng").await;
        add_stock_lot(&pool, p, w, 20.0, 1000.0, "2026-01-01").await;
        let items = [line("HD-DUP", 1.0, 1000.0, 0.0, "W-DUP")];

        // Bỏ qua chặn ở tầng app để dựng dữ liệu trùng như thực tế đã lỡ xảy ra.
        sqlx::query!("INSERT INTO invoice (number, date, customer, total, status) VALUES ('HD0001', '2026-03-10', 'Khách A', 1000.0, 'draft')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query!("INSERT INTO invoice (number, date, customer, total, status) VALUES ('HD0001', '2026-03-10', 'Khách A', 1000.0, 'draft')")
            .execute(&pool)
            .await
            .unwrap();
        save_invoice_core(&pool, "HD0002", "2026-03-10", "Khách B", "", &items)
            .await
            .unwrap();

        let dups = duplicate_numbers_core(&pool).await.unwrap();
        assert_eq!(dups.len(), 1);
        assert_eq!(dups[0].number, "HD0001");
        assert_eq!(dups[0].count, 2);

        // Xoá bản nháp trùng thì hết cảnh báo.
        let dup_id: i64 = sqlx::query_scalar!(
            r#"SELECT id as "id!" FROM invoice WHERE number = 'HD0001' ORDER BY id LIMIT 1"#
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        delete_invoice_core(&pool, dup_id).await.unwrap();
        assert!(duplicate_numbers_core(&pool).await.unwrap().is_empty());
    }

    /// Bấm nhiều lần nút Lập hóa đơn / nhập tay số đã có → không được tạo trùng.
    #[tokio::test]
    async fn so_hoa_don_trung_thi_bao_loi_khong_ghi_trung() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "HD-SO", "Hàng", 0.01).await;
        let w = seed_warehouse(&pool, "W-SO", "Kho số hóa đơn").await;
        add_stock_lot(&pool, p, w, 20.0, 1000.0, "2026-01-01").await;
        let items = [line("HD-SO", 1.0, 1000.0, 0.0, "W-SO")];

        save_invoice_core(&pool, "HD9001", "2026-03-10", "Khách X", "MST-X", &items)
            .await
            .unwrap();

        // Lần thứ hai (bấm Lập nhiều lần) → chặn, không sinh bản ghi thứ hai.
        let err = save_invoice_core(&pool, "HD9001", "2026-03-10", "Khách X", "MST-X", &items)
            .await
            .unwrap_err();
        assert!(err.contains("đã tồn tại"), "{err}");
        // Khác hoa/thường cũng tính là trùng số hóa đơn.
        let err = save_invoice_core(&pool, "hd9001", "2026-03-10", "Khách X", "MST-X", &items)
            .await
            .unwrap_err();
        assert!(err.contains("đã tồn tại"), "{err}");

        let count: i64 = sqlx::query_scalar!(
            r#"SELECT COUNT(*) as "c!: i64" FROM invoice WHERE number = 'HD9001'"#
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 1);

        // Sửa hóa đơn nháp sang số của hóa đơn khác → cũng phải chặn.
        let id: i64 =
            sqlx::query_scalar!(r#"SELECT id as "id!" FROM invoice WHERE number = 'HD9001'"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        save_invoice_core(&pool, "HD9002", "2026-03-10", "Khách X", "MST-X", &items)
            .await
            .unwrap();
        let err = update_invoice_core(
            &pool,
            id,
            "HD9002",
            "2026-03-11",
            "Khách X",
            "MST-X",
            &items,
        )
        .await
        .unwrap_err();
        assert!(err.contains("đã tồn tại"), "{err}");

        // Sửa nháp nhưng giữ nguyên số của chính nó thì vẫn được.
        update_invoice_core(
            &pool,
            id,
            "HD9001",
            "2026-03-11",
            "Khách X",
            "MST-X",
            &items,
        )
        .await
        .unwrap();
    }

    /// Thay thế hóa đơn đã phát hành: đảo doanh thu, tạo hóa đơn nháp mới,
    /// giữ nguyên tồn kho (hàng vẫn nằm trong kho — chỉ sai trên giấy tờ).
    #[tokio::test]
    async fn thay_the_hoa_don_da_phat_hanh_dao_doanh_thu_va_tao_hoa_don_nhap_moi() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "HD-TH1", "Bình NN Rossi", 0.01).await;
        let w = seed_warehouse(&pool, "W-TH1", "Kho TH").await;
        add_stock_lot(&pool, p, w, 10.0, 1000.0, "2026-01-01").await;
        save_invoice_core(
            &pool,
            "HD0007",
            "2026-03-10",
            "Khách X",
            "MST-X",
            &[
                line("HD-TH1", 2.0, 1000.0, 0.0, "W-TH1"),
                line("HD-TH1", 1.0, 2000.0, 500.0, "W-TH1"),
            ],
        )
        .await
        .unwrap();
        let id: i64 =
            sqlx::query_scalar!(r#"SELECT id as "id!" FROM invoice WHERE number = 'HD0007'"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        create_outbound_from_invoice(&pool, id).await.unwrap();
        link_hddt_core(&pool, id, "00000123", "1C26TT152", "2026-03-10")
            .await
            .unwrap();

        let res = replace_invoice_core(&pool, id, "Sai số lượng bán", "admin")
            .await
            .unwrap();
        let new_id = res["replacement_invoice_id"].as_i64().unwrap();
        let new_no = res["replacement_number"].as_str().unwrap();
        assert_eq!(new_no, "HD0008");
        assert_eq!(res["adjust_voucher_no"].as_str().unwrap(), "PX001DJ");

        // Bút toán đảo: Nợ 511 / Có 131, đúng bằng từng dòng (3.500), gắn
        // adjust_code = 'GiamDT' để tờ khai thuế trừ vào nhóm ngành.
        let down: f64 = sqlx::query_scalar!(
            "SELECT COALESCE(SUM(amount), 0.0) as \"q!: f64\" FROM journal_entry
              WHERE adjust_code = 'GiamDT'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(down, 3_500.0);
        let acc = sqlx::query!(
            "SELECT debit_account, credit_account FROM journal_entry WHERE adjust_code = 'GiamDT' LIMIT 1"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            (acc.debit_account.as_str(), acc.credit_account.as_str()),
            ("511", "131")
        );
        // Doanh thu kê trong tờ khai = ghi tăng − ghi giảm = 0.
        let net: f64 = sqlx::query_scalar!(
            "SELECT COALESCE(SUM(CASE WHEN adjust_code <> 'GiamDT' THEN amount ELSE 0.0 END), 0.0)
                  - COALESCE(SUM(CASE WHEN adjust_code = 'GiamDT' THEN amount ELSE 0.0 END), 0.0)
                    as \"q!: f64\"
               FROM journal_entry WHERE entry_type = 'PX'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(net, 0.0);

        // Tồn kho KHÔNG đụng: hàng vẫn trong kho cho hóa đơn thay thế xuất sau.
        let qty: f64 = sqlx::query_scalar!(
            "SELECT quantity FROM stock_lot WHERE product_id = ? AND depleted = 0",
            p
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(qty, 7.0);

        // Hóa đơn cũ: đã bị thay thế, trỏ sang hóa đơn mới, ghi phiếu đảo doanh thu.
        let old = sqlx::query!(
            r#"SELECT status, replace_reason, ref_invoice, adjust_voucher_no
                 FROM invoice WHERE id = ?"#,
            id
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(old.status, "replaced");
        assert_eq!(old.replace_reason, "Sai số lượng bán");
        assert_eq!(old.ref_invoice, "HD0008");
        assert_eq!(old.adjust_voucher_no, "PX001DJ");

        // Hóa đơn thay thế: nháp, sao chép đủ khách + dòng hàng.
        let fresh = sqlx::query!(
            r#"SELECT status, customer, customer_tax_code, total,
                      (SELECT COUNT(*) FROM invoice_item WHERE invoice_id = invoice.id) as "n!: i64"
                 FROM invoice WHERE id = ?"#,
            new_id
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(fresh.status, "draft");
        assert_eq!(fresh.customer, "Khách X");
        assert_eq!(fresh.customer_tax_code, "MST-X");
        assert_eq!(fresh.total, 3_500.0);
        assert_eq!(fresh.n, 2);
        let repl_id: Option<i64> = sqlx::query_scalar!(
            "SELECT replaces_invoice_id FROM invoice WHERE id = ?",
            new_id
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(repl_id, Some(id));
    }

    #[tokio::test]
    async fn thay_the_hoa_don_nhap_thi_bao_chi_can_sua_va_xoa() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "HD-TH2", "Hàng nháp", 0.01).await;
        let w = seed_warehouse(&pool, "W-TH2", "Kho TH2").await;
        add_stock_lot(&pool, p, w, 5.0, 1000.0, "2026-01-01").await;
        save_invoice_core(
            &pool,
            "HD0011",
            "2026-03-10",
            "Khách Y",
            "MST-Y",
            &[line("HD-TH2", 1.0, 1000.0, 0.0, "W-TH2")],
        )
        .await
        .unwrap();
        let id: i64 =
            sqlx::query_scalar!(r#"SELECT id as "id!" FROM invoice WHERE number = 'HD0011'"#)
                .fetch_one(&pool)
                .await
                .unwrap();

        let err = replace_invoice_core(&pool, id, "Nháp thôi", "admin")
            .await
            .unwrap_err();
        assert!(err.contains("chưa phát hành"), "{err}");
        let err = replace_invoice_core(&pool, id, "  ", "admin")
            .await
            .unwrap_err();
        assert!(err.contains("lý do"), "{err}");
    }

    #[tokio::test]
    async fn thay_the_hoa_don_chua_co_phieu_xuat_thi_bao_tao_phieu_truoc() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "HD-TH3", "Hàng chưa xuất", 0.01).await;
        let w = seed_warehouse(&pool, "W-TH3", "Kho TH3").await;
        add_stock_lot(&pool, p, w, 5.0, 1000.0, "2026-01-01").await;
        save_invoice_core(
            &pool,
            "HD0012",
            "2026-03-10",
            "Khách Z",
            "MST-Z",
            &[line("HD-TH3", 1.0, 1000.0, 0.0, "W-TH3")],
        )
        .await
        .unwrap();
        let id: i64 =
            sqlx::query_scalar!(r#"SELECT id as "id!" FROM invoice WHERE number = 'HD0012'"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        link_hddt_core(&pool, id, "00000124", "1C26TT152", "2026-03-10")
            .await
            .unwrap();

        let err = replace_invoice_core(&pool, id, "Sai tiền", "admin")
            .await
            .unwrap_err();
        assert!(err.contains("lập phiếu xuất trước"), "{err}");
    }

    // ─── F5 — DÒNG HÓA ĐƠN GIỮ NGUYÊN TÊN NGƯỜI DÙNG CHỌN (TÊN KHÁC) ───

    #[tokio::test]
    async fn hoa_don_giu_ten_duoc_chon_khong_fallback_ve_ten_chinh() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "HD-AL", "Bột mì", 0.01).await;
        let w = seed_warehouse(&pool, "W1", "Kho 1").await;
        add_stock_lot(&pool, p, w, 100.0, 1_000.0, "2026-01-01").await;

        // Dòng 1: người dùng chọn tên khác "Mì flour".
        let mut alias_line = line("HD-AL", 2.0, 10_000.0, 0.0, "W1");
        alias_line.line_name = "Mì flour".into();
        // Dòng 2: không chọn gì → dùng tên chính.
        let plain_line = line("HD-AL", 1.0, 10_000.0, 0.0, "W1");
        let res = save_invoice_core(
            &pool,
            "HD-AL1",
            "2026-03-10",
            "Khách Y",
            "",
            &[alias_line, plain_line],
        )
        .await
        .expect("lưu hóa đơn có dòng đặt tên khác");
        let id = res["invoice_id"].as_i64().unwrap();

        // Lưu đúng tên đã chọn (không ghi đè bằng tên sản phẩm).
        let stored: Vec<String> = sqlx::query_scalar!(
            r#"SELECT line_name FROM invoice_item WHERE invoice_id = ?"#,
            id
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(stored, vec!["Mì flour".to_string(), String::new()]);

        // Chi tiết in (khối HĐĐT "hdhhdvu"): dòng alias vẫn hiện "Mì flour",
        // dòng không chọn tên khác hiện tên sản phẩm.
        let detail: serde_json::Value =
            serde_json::from_str(&invoice_draft_detail_json(&pool, id).await.unwrap()).unwrap();
        let names: Vec<&str> = detail["hdhhdvu"]
            .as_array()
            .unwrap()
            .iter()
            .map(|it| it["ten"].as_str().unwrap())
            .collect();
        assert_eq!(names, vec!["Mì flour", "Bột mì"]);
    }

    // ─── Liên kết lại HĐĐT đại diện cho phiếu xuất (#8 mục 12) ───

    /// Lập 1 phiếu xuất kèm hóa đơn nháp — trả về id hóa đơn vừa tạo.
    async fn seed_outbound_with_invoice(
        pool: &SqlitePool,
        voucher_no: &str,
        date: &str,
        invoice_no: &str,
    ) -> i64 {
        let p = seed_product(pool, "HD-LINK", "Hàng liên kết", 0.01).await;
        let w = seed_warehouse(pool, "W-LINK", "Kho liên kết").await;
        add_stock_lot(pool, p, w, 10.0, 1_000.0, "2026-01-01").await;
        crate::commands::stock::save_outbound_core(
            pool,
            date,
            voucher_no,
            &format!("Bán hàng theo hóa đơn {invoice_no}"),
            "",
            "HKD",
            &[OutboundItemInput {
                product_code: "HD-LINK".into(),
                quantity: 1.0,
                unit_price: 5_000.0,
                discount: 0.0,
                industry_code: "PPHH".into(),
                warehouse_code: "W-LINK".into(),
                line_name: String::new(),
            }],
            "",
            false,
            "sale",
            "up",
            &OutboundInvoiceInput {
                number: invoice_no.into(),
                ..Default::default()
            },
        )
        .await
        .expect("lập phiếu xuất kèm hóa đơn");
        sqlx::query_scalar!(
            r#"SELECT id as "id!" FROM invoice WHERE number = ?"#,
            invoice_no
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    /// Gắn hóa đơn mới vào phiếu xuất: hóa đơn cũ của phiếu được gỡ, diễn giải
    /// tự sinh đổi theo số HĐĐT mới, các phiếu khác không bị đụng.
    #[tokio::test]
    async fn lien_ket_lai_hoa_don_go_lien_ket_cu_va_cap_nhat_dien_giai() {
        let pool = test_pool().await;
        let old_id = seed_outbound_with_invoice(&pool, "PX-LK1", "2026-03-10", "HD-LK-CU").await;
        let new_id: i64 = sqlx::query_scalar(
            "INSERT INTO invoice (number, date, customer, status, e_invoice_no)
             VALUES ('HD-LK-MOI', '2026-03-12', 'Khách mới', 'official', '00000999')
             RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        let res = link_outbound_invoice_core(&pool, "PX-LK1", new_id)
            .await
            .expect("liên kết lại được");
        assert_eq!(res["display_number"], "00000999");

        let old_voucher: String =
            sqlx::query_scalar!(r#"SELECT voucher_no FROM invoice WHERE id = ?"#, old_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(old_voucher, "", "hóa đơn cũ phải được gỡ khỏi phiếu");
        let new_voucher: String =
            sqlx::query_scalar!(r#"SELECT voucher_no FROM invoice WHERE id = ?"#, new_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(new_voucher, "PX-LK1");

        let description: String = sqlx::query_scalar(
            "SELECT MIN(description) FROM journal_entry
              WHERE entry_type = 'PX' AND voucher_no = 'PX-LK1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(description, "Bán hàng theo hóa đơn 00000999");
    }

    /// Không cướp hóa đơn của phiếu khác, không liên kết chéo kỳ thuế, và
    /// không liên kết vào phiếu không tồn tại.
    #[tokio::test]
    async fn lien_ket_lai_hoa_don_chan_xung_dot_va_lech_ky() {
        let pool = test_pool().await;
        seed_outbound_with_invoice(&pool, "PX-LK2", "2026-03-10", "HD-LK2").await;
        let other: i64 = sqlx::query_scalar(
            "INSERT INTO invoice (number, date, customer, status, voucher_no)
             VALUES ('HD-LK-OTHER', '2026-03-11', 'Khách khác', 'draft', 'PX-KHAC')
             RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let err = link_outbound_invoice_core(&pool, "PX-LK2", other)
            .await
            .unwrap_err();
        assert!(err.contains("đang gắn phiếu xuất PX-KHAC"), "{err}");

        let far: i64 = sqlx::query_scalar(
            "INSERT INTO invoice (number, date, customer, status)
             VALUES ('HD-LK-APR', '2026-04-02', 'Khách tháng 4', 'draft')
             RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let err = link_outbound_invoice_core(&pool, "PX-LK2", far)
            .await
            .unwrap_err();
        assert!(err.contains("kỳ khác"), "{err}");

        let err = link_outbound_invoice_core(&pool, "PX-KHONG-CO", other)
            .await
            .unwrap_err();
        assert!(err.contains("Không tìm thấy phiếu xuất"), "{err}");
    }

    /// Danh sách ứng viên: đúng kỳ thuế, bỏ hóa đơn thay thế/hủy, hóa đơn đang
    /// gắn chính phiếu lên đầu rồi tới hóa đơn tự do, cuối cùng là phiếu khác.
    #[tokio::test]
    async fn danh_sach_hoa_don_de_lien_ket_loc_ky_va_xep_hang() {
        let pool = test_pool().await;
        seed_outbound_with_invoice(&pool, "PX-LK3", "2026-03-10", "HD-LK3").await;
        sqlx::query(
            "INSERT INTO invoice (number, date, customer, status, voucher_no)
             VALUES ('HD-FREE', '2026-03-15', 'Khách tự do', 'draft', ''),
                    ('HD-OTHER', '2026-03-11', 'Khách phiếu khác', 'draft', 'PX-KHAC'),
                    ('HD-APR', '2026-04-01', 'Khách tháng 4', 'draft', ''),
                    ('HD-REPLACED', '2026-03-20', 'Khách thay thế', 'replaced', '')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let rows = invoice_link_candidates_core(&pool, "PX-LK3", "")
            .await
            .unwrap();
        let numbers: Vec<&str> = rows.iter().map(|r| r.number.as_str()).collect();
        assert_eq!(numbers, vec!["HD-LK3", "HD-FREE", "HD-OTHER"]);

        let found = invoice_link_candidates_core(&pool, "PX-LK3", "HD-FREE")
            .await
            .unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].number, "HD-FREE");

        // Tìm được cả theo tên khách hàng.
        let by_customer = invoice_link_candidates_core(&pool, "PX-LK3", "tự do")
            .await
            .unwrap();
        assert_eq!(by_customer.len(), 1);
        assert_eq!(by_customer[0].number, "HD-FREE");
    }
}
