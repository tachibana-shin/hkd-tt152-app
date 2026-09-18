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
    let items: Vec<InvoiceItemRow> = sqlx::query_as!(
        InvoiceItemRow,
        "SELECT ii.id, ii.invoice_id, p.code AS product_code, p.name AS product_name,
                p.unit, ii.quantity, ii.unit_price, ii.subtotal
         FROM invoice_item ii
         JOIN product p ON p.id = ii.product_id
         WHERE ii.invoice_id = ?
         ORDER BY ii.id",
        id
    )
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

    let mut shortage: Vec<String> = Vec::new();
    for item in &items {
        let product = resolve_product(&mut tx, &item.product_code).await?;
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

    // tổng tiền + thuế GTGT (theo mức thuế suất của từng sản phẩm)
    let mut total = 0.0;
    let mut vat_amount = 0.0;
    for item in &items {
        let product = resolve_product(&mut tx, &item.product_code).await?;
        let subtotal = item.quantity * item.unit_price;
        total += subtotal;
        vat_amount += subtotal * product.vat_rate;
    }

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

    for item in &items {
        let product = resolve_product(&mut tx, &item.product_code).await?;
        let subtotal = item.quantity * item.unit_price;
        sqlx::query!(
            "INSERT INTO invoice_item (invoice_id, product_id, quantity, unit_price, subtotal)
             VALUES (?, ?, ?, ?, ?)",
            invoice_id,
            product.id,
            item.quantity,
            item.unit_price,
            subtotal
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    }

    tx.commit().await.map_err(|e| e.to_string())?;
    audit(&state, "save", "invoice", &number).await;
    Ok(serde_json::json!({ "ok": true, "invoice_id": invoice_id, "total": total, "vat_amount": vat_amount }).to_string())
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
