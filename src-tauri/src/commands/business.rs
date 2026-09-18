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

/// Thông tin hộ kinh doanh. Năm tài chính / kỳ báo cáo KHÔNG còn được lưu:
/// các báo cáo và tờ khai thuế dùng năm hiện tại từ hệ thống (ngày thực làm việc).
#[tauri::command]
pub(crate) async fn get_business_config(state: State<'_, AppState>) -> Result<String, String> {
    let row = sqlx::query_as::<_, BusinessConfigRow>(
        "SELECT name, tax_code, address, short_name, ownership,
                province, tax_code_issued_on, phone, email
         FROM business WHERE id = 1",
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
) -> Result<String, String> {
    require_role(&state, &["admin"]).await?;
    let pool = state.pool.read().await;
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    sqlx::query(
        "INSERT INTO business (id, name, tax_code, address, short_name, ownership,
                               province, tax_code_issued_on, phone, email)
         VALUES (1, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
            name = excluded.name, tax_code = excluded.tax_code,
            address = excluded.address, short_name = excluded.short_name,
            ownership = excluded.ownership, province = excluded.province,
            tax_code_issued_on = excluded.tax_code_issued_on,
            phone = excluded.phone, email = excluded.email",
    )
    .bind(&name)
    .bind(&tax_code)
    .bind(&address)
    .bind(&short_name)
    .bind(&ownership)
    .bind(&province)
    .bind(&tax_code_issued_on)
    .bind(&phone)
    .bind(&email)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok("ok".into())
}