#![allow(unused_imports)]

use crate::helpers::*;
use crate::models::*;
use serde_json::json;
use sqlx::sqlite::{SqlitePoolOptions, SqliteTransaction};
use sqlx::SqlitePool;
use tauri::{State, Manager, AppHandle};

/// Nhập khối NHAP LIEU (sheet NHAP LIEU của file mẫu HKD) vào journal_entry.
/// Không tạo stock_lot như PNK/PXK thủ công — dữ liệu lịch sử chỉ phục vụ sổ sách
/// (tồn kho tổng hợp theo bút toán PN/PX; giá vốn FIFO cho các phiếu xuất SAU đó
/// vẫn dựa trên stock_lot được tạo từ màn hình Nhập kho).
#[tauri::command]
pub(crate) async fn import_nhap_lieu(
    state: State<'_, AppState>,
    items: Vec<ImportEntryInput>,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    if items.is_empty() {
        return Err("Không có dòng dữ liệu nào để nhập".into());
    }
    let pool = state.pool.read().await;
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let mut imported = 0i64;
    let mut skipped = 0i64;
    for it in &items {
        if it.voucher_no.is_empty() || it.posting_date.is_empty() {
            skipped += 1;
            continue;
        }
        // insert trực tiếp để lưu được cả Ngày chứng từ (doc_date) và Kỳ khai thuế (tax_period)
        sqlx::query(
            "INSERT INTO journal_entry
             (posting_date, voucher_no, doc_date, description, product_code,
              supplier_code, customer_code, quantity, unit_price, amount, entry_type,
              debit_account, credit_account, industry_code, vat_rate, pit_rate,
              tax_period, unit_code, adjust_code, note)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&it.posting_date)
        .bind(&it.voucher_no)
        .bind(if it.doc_date.is_empty() { &it.posting_date } else { &it.doc_date })
        .bind(&it.description)
        .bind(&it.product_code)
        .bind(&it.supplier_code)
        .bind(&it.customer_code)
        .bind(it.quantity)
        .bind(it.unit_price)
        .bind(it.amount)
        .bind(&it.entry_type)
        .bind(&it.debit_account)
        .bind(&it.credit_account)
        .bind(&it.industry_code)
        .bind(it.vat_rate)
        .bind(it.pit_rate)
        .bind(it.tax_period)
        .bind(if it.unit_code.is_empty() { "HaNoi-01" } else { &it.unit_code })
        .bind(&it.adjust_code)
        .bind(&it.note)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        imported += 1;
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    audit(
        &state,
        "import",
        "nhap_lieu",
        &format!("{} dòng (bỏ qua {})", imported, skipped),
    )
    .await;
    Ok(json!({ "ok": true, "imported": imported, "skipped": skipped }).to_string())
}