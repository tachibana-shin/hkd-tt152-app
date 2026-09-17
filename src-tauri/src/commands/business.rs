#![allow(unused_imports)]

use crate::helpers::*;
use crate::models::*;
use serde_json::json;
use sqlx::sqlite::{SqlitePoolOptions, SqliteTransaction};
use sqlx::SqlitePool;
use tauri::{State, Manager, AppHandle};
#[tauri::command]
pub(crate) async fn get_business_info(state: State<'_, AppState>) -> Result<String, String> {
    let row: BusinessInfo = sqlx::query_as!(
        BusinessInfo,
        "SELECT name FROM business WHERE id = 1"
    )
    .fetch_one(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    Ok(row.name)
}

#[tauri::command]
pub(crate) async fn get_business_config(state: State<'_, AppState>) -> Result<String, String> {
    let row: BusinessConfigRow = sqlx::query_as!(
        BusinessConfigRow,
        "SELECT b.name, b.tax_code, b.address, b.short_name, b.ownership,
                b.province, b.tax_code_issued_on, b.phone, b.email, b.fiscal_year,
                rp.from_date AS report_from, rp.to_date AS report_to
         FROM business b CROSS JOIN report_period rp
         WHERE b.id = 1 AND rp.id = 1"
    )
    .fetch_one(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    Ok(serde_json::to_string(&row).unwrap_or_default())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn save_business_config(
    state: State<'_, AppState>,
    name: String,
    tax_code: String,
    address: String,
    short_name: String,
    ownership: String,
    province: String,
    tax_code_issued_on: String,
    phone: String,
    email: String,
    fiscal_year: i64,
    report_from: String,
    report_to: String,
) -> Result<String, String> {
    require_role(&state, &["admin"]).await?;
    let pool = state.pool.read().await;
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    sqlx::query!(
        "INSERT INTO business (id, name, tax_code, address, short_name, ownership,
                               province, tax_code_issued_on, phone, email, fiscal_year)
         VALUES (1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
            name = excluded.name, tax_code = excluded.tax_code,
            address = excluded.address, short_name = excluded.short_name,
            ownership = excluded.ownership, province = excluded.province,
            tax_code_issued_on = excluded.tax_code_issued_on,
            phone = excluded.phone, email = excluded.email,
            fiscal_year = excluded.fiscal_year",
        name,
        tax_code,
        address,
        short_name,
        ownership,
        province,
        tax_code_issued_on,
        phone,
        email,
        fiscal_year
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    sqlx::query!(
        "INSERT INTO report_period (id, from_date, to_date) VALUES (1, ?, ?)
         ON CONFLICT(id) DO UPDATE SET from_date = excluded.from_date, to_date = excluded.to_date",
        report_from,
        report_to
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok("ok".into())
}
