#![allow(unused_imports)]

use crate::helpers::*;
use crate::models::*;
use serde_json::json;
use sqlx::sqlite::{SqlitePoolOptions, SqliteTransaction};
use sqlx::SqlitePool;
use tauri::{AppHandle, Manager, State};

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
        // Biểu thức tạm (`if ... { } else { .. }`) bind trước: macro giữ
        // tham chiếu tới hết `.await`.
        let doc_date = if it.doc_date.is_empty() {
            it.posting_date.clone()
        } else {
            it.doc_date.clone()
        };
        let unit_code = if it.unit_code.is_empty() {
            "HKD".to_string()
        } else {
            it.unit_code.clone()
        };
        sqlx::query!(
            "INSERT INTO journal_entry
             (posting_date, voucher_no, doc_date, description, product_code,
              supplier_code, customer_code, quantity, unit_price, amount, entry_type,
              debit_account, credit_account, industry_code, vat_rate, pit_rate,
              tax_period, unit_code, adjust_code, note)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            it.posting_date,
            it.voucher_no,
            doc_date,
            it.description,
            it.product_code,
            it.supplier_code,
            it.customer_code,
            it.quantity,
            it.unit_price,
            it.amount,
            it.entry_type,
            it.debit_account,
            it.credit_account,
            it.industry_code,
            it.vat_rate,
            it.pit_rate,
            it.tax_period,
            unit_code,
            it.adjust_code,
            it.note
        )
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
