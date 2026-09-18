#![allow(unused_imports)]

use crate::helpers::*;
use crate::models::*;
use serde_json::json;
use sqlx::sqlite::{SqlitePoolOptions, SqliteTransaction};
use sqlx::SqlitePool;
use tauri::{State, Manager, AppHandle};
#[tauri::command]
pub(crate) async fn get_invoices(state: State<'_, AppState>) -> Result<String, String> {
    let rows: Vec<InvoiceRow> = sqlx::query_as!(
        InvoiceRow,
        "SELECT id, number, date, customer, customer_tax_code, total, vat_amount,
                status, e_invoice_no, e_invoice_symbol, e_invoice_date
         FROM invoice ORDER BY date DESC, id DESC"
    )
    .fetch_all(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    Ok(serde_json::to_string(&rows).unwrap_or_default())
}

#[tauri::command]
pub(crate) async fn get_invoice_detail(
    state: State<'_, AppState>,
    id: i64,
) -> Result<String, String> {
    let inv: InvoiceRow = sqlx::query_as!(
        InvoiceRow,
        "SELECT id, number, date, customer, customer_tax_code, total, vat_amount,
                status, e_invoice_no, e_invoice_symbol, e_invoice_date
         FROM invoice WHERE id = ?",
        id
    )
    .fetch_one(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    let items: Vec<InvoiceItemRow> = sqlx::query_as::<_, InvoiceItemRow>(
        "SELECT ii.id, ii.invoice_id, p.code AS product_code, p.name AS product_name,
                p.unit, ii.quantity, ii.unit_price, ii.subtotal,
                ii.industry_code, ii.vat_rate, ii.pit_rate
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
    if items.is_empty() {
        return Err("Chưa có mặt hàng nào trong hóa đơn".into());
    }
    let pool = state.pool.read().await;
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    let mut basis: Vec<(String, f64, f64)> = Vec::with_capacity(items.len());
    let mut shortage: Vec<String> = Vec::new();
    for item in &items {
        let product = resolve_product(&mut tx, &item.product_code).await?;

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
        let (vat_rate, pit_rate): (f64, f64) = sqlx::query_as(
            "SELECT vat_rate, pit_rate FROM industry_group WHERE code = ?",
        )
        .bind(&industry)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| format!("Nhóm ngành '{}' không hợp lệ", industry))?;
        basis.push((industry, vat_rate, pit_rate));

        // Sản phẩm dịch vụ (nhân công...) không theo dõi tồn kho → bỏ qua kiểm tra.
        if product.is_service {
            continue;
        }
        let available: (f64,) = sqlx::query_as(
            "SELECT COALESCE(SUM(quantity), 0.0) FROM stock_lot WHERE product_id = ? AND depleted = 0",
        )
        .bind(product.id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        if item.quantity - available.0 > 1e-9 {
            shortage.push(format!(
                "{}: cần {:.2}, tồn {:.2}",
                item.product_code, item.quantity, available.0
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
    for (i, item) in items.iter().enumerate() {
        let subtotal = item.quantity * item.unit_price;
        total += subtotal;
        tax_payable += subtotal * basis[i].1;
    }
    let vat_amount = 0.0;

    let res = sqlx::query!(
        "INSERT INTO invoice (number, date, customer, customer_tax_code, total, vat_amount, status)
         VALUES (?, ?, ?, ?, ?, ?, 'draft')",
        number,
        date,
        customer,
        customer_tax_code,
        total,
        vat_amount
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    let invoice_id = res.last_insert_rowid();

    for (i, item) in items.iter().enumerate() {
        let product = resolve_product(&mut tx, &item.product_code).await?;
        let subtotal = item.quantity * item.unit_price;
        sqlx::query(
            "INSERT INTO invoice_item (invoice_id, product_id, quantity, unit_price, subtotal, industry_code, vat_rate, pit_rate)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(invoice_id)
        .bind(product.id)
        .bind(item.quantity)
        .bind(item.unit_price)
        .bind(subtotal)
        .bind(&basis[i].0)
        .bind(basis[i].1)
        .bind(basis[i].2)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    }

    tx.commit().await.map_err(|e| e.to_string())?;
    audit(&state, "save", "invoice", &number).await;
    Ok(serde_json::json!({
        "ok": true,
        "invoice_id": invoice_id,
        "total": total,
        "vat_amount": vat_amount,
        "tax_payable": round2(tax_payable),
    })
    .to_string())
}

/// Ghi nhận số HĐĐT cho hóa đơn nháp → trạng thái 'official'
#[tauri::command]
pub(crate) async fn link_hddt(
    state: State<'_, AppState>,
    invoice_id: i64,
    hddt_no: String,
    hddt_symbol: String,
    hddt_date: String,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    sqlx::query!(
        "UPDATE invoice SET status = 'official', e_invoice_no = ?, e_invoice_symbol = ?, e_invoice_date = ? WHERE id = ?",
        hddt_no,
        hddt_symbol,
        hddt_date,
        invoice_id
    )
    .execute(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    audit(
        &state,
        "link_hddt",
        "invoice",
        &format!("invoice_id={} → {}/{}", invoice_id, hddt_symbol, hddt_no),
    )
    .await;
    Ok("ok".into())
}
