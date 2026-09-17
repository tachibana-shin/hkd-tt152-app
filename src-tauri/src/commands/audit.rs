#![allow(unused_imports)]

use crate::helpers::*;
use crate::models::*;
use serde_json::json;
use sqlx::sqlite::{SqlitePoolOptions, SqliteTransaction};
use sqlx::SqlitePool;
use tauri::{State, Manager, AppHandle};

/// Ghi 1 dòng audit_log thủ công (dùng khi cần đánh dấu hành động từ frontend)
#[tauri::command]
pub(crate) async fn log_audit(
    state: State<'_, AppState>,
    action: String,
    entity: String,
    detail: String,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    audit(&state, &action, &entity, &detail).await;
    Ok("ok".into())
}

/// Nhật ký hoạt động — mới nhất trước (mặc định 200 dòng)
#[tauri::command]
pub(crate) async fn get_audit_log(
    state: State<'_, AppState>,
    limit: i64,
) -> Result<String, String> {
    let lim = if limit <= 0 { 200 } else { limit };
    let rows: Vec<AuditRow> = sqlx::query_as::<_, AuditRow>(
        "SELECT id, ts, action, entity, detail, username FROM audit_log ORDER BY id DESC LIMIT ?",
    )
    .bind(lim)
    .fetch_all(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    Ok(serde_json::to_string(&rows).unwrap_or_default())
}