#![allow(unused_imports)]

use crate::helpers::*;
use crate::models::*;
use serde_json::json;
use sqlx::sqlite::{SqlitePoolOptions, SqliteTransaction};
use sqlx::SqlitePool;
use tauri::{State, Manager, AppHandle};
#[tauri::command]
pub(crate) async fn get_accounts(state: State<'_, AppState>) -> Result<String, String> {
    let rows: Vec<AccountRow> = sqlx::query_as!(
        AccountRow,
        "SELECT id, code, name, opening_debit, opening_credit FROM account ORDER BY code"
    )
    .fetch_all(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    Ok(serde_json::to_string(&rows).unwrap_or_default())
}

#[tauri::command]
pub(crate) async fn get_products(state: State<'_, AppState>) -> Result<String, String> {
    let rows: Vec<ProductRow> = sqlx::query_as!(
        ProductRow,
        "SELECT id, code, name, unit, sale_price, cost_price, min_stock, vat_rate, vat_reduced
         FROM product ORDER BY code"
    )
    .fetch_all(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    Ok(serde_json::to_string(&rows).unwrap_or_default())
}

#[tauri::command]
pub(crate) async fn get_industry_groups(state: State<'_, AppState>) -> Result<String, String> {
    let rows: Vec<IndustryGroupRow> = sqlx::query_as!(
        IndustryGroupRow,
        "SELECT code, name, vat_rate, pit_rate FROM industry_group ORDER BY code"
    )
    .fetch_all(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    Ok(serde_json::to_string(&rows).unwrap_or_default())
}

#[tauri::command]
pub(crate) async fn get_warehouses(state: State<'_, AppState>) -> Result<String, String> {
    let rows: Vec<WarehouseRow> = sqlx::query_as!(
        WarehouseRow,
        "SELECT id, code, name FROM warehouse ORDER BY code"
    )
    .fetch_all(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    Ok(serde_json::to_string(&rows).unwrap_or_default())
}

#[tauri::command]
pub(crate) async fn save_warehouse(
    state: State<'_, AppState>,
    code: String,
    name: String,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan", "kho"]).await?;
    sqlx::query!(
        "INSERT INTO warehouse (code, name) VALUES (?, ?)
         ON CONFLICT(code) DO UPDATE SET name = excluded.name",
        code,
        name
    )
    .execute(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    audit(&state, "save", "warehouse", &code).await;
    Ok("ok".into())
}

#[tauri::command]
pub(crate) async fn get_suppliers(state: State<'_, AppState>) -> Result<String, String> {
    let rows: Vec<SupplierRow> = sqlx::query_as!(
        SupplierRow,
        "SELECT id, code, name, address, tax_code, phone FROM supplier ORDER BY code"
    )
    .fetch_all(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    Ok(serde_json::to_string(&rows).unwrap_or_default())
}

#[tauri::command]
pub(crate) async fn save_supplier(
    state: State<'_, AppState>,
    code: String,
    name: String,
    address: String,
    tax_code: String,
    phone: String,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan", "kho"]).await?;
    sqlx::query!(
        "INSERT INTO supplier (code, name, address, tax_code, phone) VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(code) DO UPDATE SET
            name = excluded.name, address = excluded.address,
            tax_code = excluded.tax_code, phone = excluded.phone",
        code,
        name,
        address,
        tax_code,
        phone
    )
    .execute(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    audit(&state, "save", "supplier", &code).await;
    Ok("ok".into())
}

#[tauri::command]
pub(crate) async fn get_customers(state: State<'_, AppState>) -> Result<String, String> {
    let rows: Vec<CustomerRow> = sqlx::query_as!(
        CustomerRow,
        "SELECT id, code, name, address, tax_code, phone FROM customer ORDER BY code"
    )
    .fetch_all(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    Ok(serde_json::to_string(&rows).unwrap_or_default())
}

#[tauri::command]
pub(crate) async fn save_customer(
    state: State<'_, AppState>,
    code: String,
    name: String,
    address: String,
    tax_code: String,
    phone: String,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan", "kho"]).await?;
    sqlx::query!(
        "INSERT INTO customer (code, name, address, tax_code, phone) VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(code) DO UPDATE SET
            name = excluded.name, address = excluded.address,
            tax_code = excluded.tax_code, phone = excluded.phone",
        code,
        name,
        address,
        tax_code,
        phone
    )
    .execute(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    audit(&state, "save", "customer", &code).await;
    Ok("ok".into())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn save_product(
    state: State<'_, AppState>,
    code: String,
    name: String,
    unit: String,
    sale_price: f64,
    cost_price: f64,
    min_stock: f64,
    vat_rate: f64,
    vat_reduced: bool,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan", "kho"]).await?;
    sqlx::query!(
        "INSERT INTO product (code, name, unit, sale_price, cost_price, min_stock, vat_rate, vat_reduced)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(code) DO UPDATE SET
            name = excluded.name, unit = excluded.unit,
            sale_price = excluded.sale_price, cost_price = excluded.cost_price,
            min_stock = excluded.min_stock, vat_rate = excluded.vat_rate,
            vat_reduced = excluded.vat_reduced",
        code,
        name,
        unit,
        sale_price,
        cost_price,
        min_stock,
        vat_rate,
        vat_reduced
    )
    .execute(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    audit(&state, "save", "product", &code).await;
    Ok("ok".into())
}

#[tauri::command]
pub(crate) async fn delete_product(state: State<'_, AppState>, id: i64) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan", "kho"]).await?;
    sqlx::query!("DELETE FROM product WHERE id = ?", id)
        .execute(&*state.pool.read().await)
        .await
        .map_err(|e| {
            format!("Không thể xóa sản phẩm (đã phát sinh nghiệp vụ): {}", e)
        })?;
    Ok("ok".into())
}
