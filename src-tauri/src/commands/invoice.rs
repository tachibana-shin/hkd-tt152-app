#![allow(unused_imports)]

use crate::helpers::*;
use crate::models::*;
use serde_json::json;
use sqlx::sqlite::{SqlitePoolOptions, SqliteTransaction};
use sqlx::SqlitePool;
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
    let ev: crate::commands::page::PageEvent =
        serde_json::from_str(&lazy_event).map_err(|e| format!("lazy_event lỗi: {}", e))?;
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
    let order = crate::commands::page::order_by(
        &[
            ("id", "id"),
            ("date", "date"),
            ("number", "number"),
            ("customer", "customer"),
        ],
        &ev,
        "date DESC, id DESC",
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
    let (rows, total): (Vec<InvoiceRow>, i64) = crate::commands::page::fetch_page(
        &*state.pool.read().await,
        &sql,
        &count_sql,
        &params,
        &page,
    )
    .await?;
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

/// Xem trước PDF hóa đơn nội bộ (nháp hoặc đã chép) ở màn Chờ xuất HĐĐT.
#[tauri::command]
pub(crate) async fn invoice_draft_detail(
    state: State<'_, AppState>,
    id: i64,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let pool = state.pool.read().await;
    invoice_draft_detail_json(&pool, id).await
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
}

/// Kiểm tra + dựng dòng hóa đơn từ input người dùng: nhóm ngành phải hợp lệ, tồn
/// kho phải đủ theo ĐÚNG kho xuất trên từng dòng (rỗng → tổng mọi kho), và tính
/// tổng tiền. Dùng chung cho lập mới và sửa hóa đơn nháp.
async fn prepare_invoice(
    tx: &mut SqliteTransaction<'_>,
    items: &[InvoiceItemInput],
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
        let where_str = if item.warehouse_code.trim().is_empty() {
            "tổng các kho"
        } else {
            "kho đã chọn"
        };
        if item.quantity - available > 1e-9 {
            shortage.push(format!(
                "{}: cần {:.2}, tồn {} {:.2}",
                item.product_code, item.quantity, where_str, available
            ));
        }
    }
    if !shortage.is_empty() {
        return Err(format!("Không đủ tồn kho: {}", shortage.join("; ")));
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

    Ok(PreparedInvoice {
        total,
        tax_payable,
        lines,
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
    let prepared = prepare_invoice(&mut tx, items).await?;

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
    let prepared = prepare_invoice(&mut tx, items).await?;
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

/// Tạo hóa đơn nháp — kiểm tra tồn kho trước khi lưu
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
        r#"SELECT number, date, customer, customer_tax_code, status, voucher_no, total
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
    let description = format!("Đảo doanh thu — thay thế hóa đơn {}", inv.number);
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
        "SELECT number, date, customer, customer_tax_code, voucher_no, status
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
    let description = format!("Bán hàng theo hóa đơn {}", inv.number);
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

    #[tokio::test]
    async fn hoa_don_kiem_tra_ton_theo_dung_kho_xuat_tren_dong() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "HD-B", "Hàng B", 0.01).await;
        let w1 = seed_warehouse(&pool, "W1", "Kho 1").await;
        // W2 chỉ tồn tại trong DB — bài test khai kho bằng chuỗi "W2", không cần giữ biến.
        seed_warehouse(&pool, "W2", "Kho 2").await;
        // W1 có 5, W2 không có gì
        add_stock_lot(&pool, p, w1, 5.0, 1000.0, "2026-01-01").await;

        // Khoai trừ W1 nhưng khai kho W2 → thiếu dù tổng kho đủ
        let err = save_invoice_core(
            &pool,
            "HD0002",
            "2026-03-10",
            "Khách Y",
            "MST-Y",
            &[line("HD-B", 4.0, 2000.0, 0.0, "W2")],
        )
        .await
        .unwrap_err();
        assert!(err.contains("Không đủ tồn kho"), "err: {err}");

        // Đủ tồn ở đúng kho W1 → lưu được
        save_invoice_core(
            &pool,
            "HD0003",
            "2026-03-10",
            "Khách Y",
            "MST-Y",
            &[line("HD-B", 4.0, 2000.0, 0.0, "W1")],
        )
        .await
        .expect("đủ tồn kho đúng kho");
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

    #[tokio::test]
    async fn sua_hoa_don_nhap_van_kiem_tra_ton_kho() {
        // Sửa số lượng vượt tồn phải bị chặn như lúc lập mới.
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

        let err = update_invoice_core(
            &pool,
            id,
            "HDE4",
            "2026-03-10",
            "Khách X",
            "MST-X",
            &[line("HD-EDIT4", 99.0, 1000.0, 0.0, "W1")],
        )
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
}
