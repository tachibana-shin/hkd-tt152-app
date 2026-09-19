#![allow(unused_imports)]

use crate::helpers::*;
use crate::models::*;
use serde_json::json;
use sqlx::sqlite::{SqlitePoolOptions, SqliteTransaction};
use sqlx::SqlitePool;
use tauri::{AppHandle, Manager, State};
#[tauri::command]
pub(crate) async fn get_inventory_counts(state: State<'_, AppState>) -> Result<String, String> {
    let rows: Vec<InventoryCountRow> = sqlx::query_as!(
        InventoryCountRow,
        "SELECT id, date, note FROM inventory_count ORDER BY date DESC, id DESC"
    )
    .fetch_all(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    Ok(serde_json::to_string(&rows).unwrap_or_default())
}

#[tauri::command]
pub(crate) async fn get_inventory_count_detail(
    state: State<'_, AppState>,
    id: i64,
) -> Result<String, String> {
    let rows: Vec<CountItemRow> = sqlx::query_as!(
        CountItemRow,
        "SELECT cik.id, cik.count_id, p.code AS product_code, p.name AS product_name,
                w.code AS warehouse_code,
                COALESCE((SELECT SUM(quantity) FROM stock_lot sl
                          WHERE sl.product_id = cik.product_id
                            AND sl.warehouse_id = cik.warehouse_id
                            AND sl.depleted = 0), 0.0) AS book_qty,
                cik.counted_qty, cik.variance
         FROM inventory_count_item cik
         JOIN product p ON p.id = cik.product_id
         JOIN warehouse w ON w.id = cik.warehouse_id
         WHERE cik.count_id = ?
         ORDER BY p.code",
        id
    )
    .fetch_all(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    Ok(serde_json::to_string(&rows).unwrap_or_default())
}

/// Kiểm kê: ghi số kiểm đếm + chênh lệch so với sổ sách
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn save_inventory_count(
    state: State<'_, AppState>,
    date: String,
    note: String,
    items: Vec<CountItemInput>,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan", "kho"]).await?;
    if items.is_empty() {
        return Err("Chưa có mặt hàng nào trong phiếu kiểm kê".into());
    }
    let pool = state.pool.read().await;
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    let res = sqlx::query!(
        "INSERT INTO inventory_count (date, note) VALUES (?, ?)",
        date,
        note
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    let count_id = res.last_insert_rowid();

    for item in &items {
        let product = resolve_product(&mut tx, &item.product_code).await?;
        let wh_id = resolve_warehouse(&mut tx, &item.warehouse_code, &item.product_code).await?;
        let book: (f64,) = sqlx::query_as(
            "SELECT COALESCE(SUM(quantity), 0.0) FROM stock_lot
             WHERE product_id = ? AND warehouse_id = ? AND depleted = 0",
        )
        .bind(product.id)
        .bind(wh_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        let variance = item.counted_qty - book.0;
        sqlx::query!(
            "INSERT INTO inventory_count_item (count_id, product_id, warehouse_id, counted_qty, variance)
             VALUES (?, ?, ?, ?, ?)",
            count_id,
            product.id,
            wh_id,
            item.counted_qty,
            variance
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    }

    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "ok": true, "count_id": count_id }).to_string())
}
