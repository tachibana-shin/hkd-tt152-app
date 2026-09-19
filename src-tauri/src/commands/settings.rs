#![allow(unused_imports)]

use crate::helpers::*;
use crate::models::*;
use serde_json::json;
use sqlx::sqlite::{SqlitePoolOptions, SqliteTransaction};
use sqlx::SqlitePool;
use std::collections::HashMap;
use tauri::{AppHandle, Manager, State};

#[derive(sqlx::FromRow, serde::Serialize)]
pub(crate) struct SettingRow {
    pub(crate) key: Option<String>,
    pub(crate) value: String,
}

/// Trả về toàn bộ bảng cấu hình app_setting dưới dạng map {key: value}.
#[tauri::command]
pub(crate) async fn get_app_settings(state: State<'_, AppState>) -> Result<String, String> {
    let rows: Vec<SettingRow> = sqlx::query_as!(SettingRow, "SELECT key, value FROM app_setting")
        .fetch_all(&*state.pool.read().await)
        .await
        .map_err(|e| e.to_string())?;
    let map: HashMap<String, String> = rows
        .into_iter()
        .map(|r| (r.key.unwrap_or_default(), r.value))
        .collect();
    Ok(serde_json::to_string(&map).unwrap_or_default())
}

/// Ghi đè (upsert) một nhóm giá trị vào app_setting.
#[tauri::command]
pub(crate) async fn save_app_settings(
    state: State<'_, AppState>,
    settings: HashMap<String, String>,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let pool = state.pool.read().await;
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    for (key, value) in &settings {
        sqlx::query!(
            "INSERT INTO app_setting (key, value) VALUES (?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            key,
            value
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok("ok".into())
}
