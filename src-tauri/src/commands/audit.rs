#![allow(unused_imports)]

use crate::helpers::*;
use crate::models::*;
use serde_json::json;
use sqlx::sqlite::{SqlitePoolOptions, SqliteTransaction};
use sqlx::SqlitePool;
use tauri::{AppHandle, Manager, State};

/// Kết quả phân trang: dòng của trang hiện tại + tổng số dòng (để bảng tính số trang).
#[derive(serde::Serialize)]
pub(crate) struct PageResult<T> {
    pub rows: Vec<T>,
    pub total: i64,
}

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

/// Nhật ký hoạt động — phân trang server-side, mới nhất trước.
///
/// Bảng từng kéo tới 1000 dòng (kèm cả `detail` dài) về client rồi cắt trang ở
/// trình duyệt nên mở màn là đơ. Giờ mỗi lần đổi trang/lọc chỉ lấy đúng trang
/// đó; `total` để bảng hiện số trang thật.
#[tauri::command]
pub(crate) async fn get_audit_log_page(
    state: State<'_, AppState>,
    lazy_event: String,
) -> Result<String, String> {
    let ev: crate::commands::page::PageEvent =
        serde_json::from_str(&lazy_event).map_err(|e| format!("lazy_event lỗi: {}", e))?;
    let page = ev.page();
    let (where_sql, params) = crate::commands::page::global_where(
        ev.global_keyword(),
        &["action", "entity", "detail", "username"],
    );
    let order = crate::commands::page::order_by(
        &[
            ("id", "id"),
            ("ts", "ts"),
            ("action", "action"),
            ("entity", "entity"),
            ("username", "username"),
        ],
        &ev,
        "id DESC",
    );
    let sql = format!(
        "SELECT id, ts, action, entity, detail, username FROM audit_log{} ORDER BY {} LIMIT ? OFFSET ?",
        where_sql, order
    );
    let count_sql = format!("SELECT COUNT(*) FROM audit_log{where_sql}");
    let (rows, total): (Vec<AuditRow>, i64) = crate::commands::page::fetch_page(
        &*state.pool.read().await,
        &sql,
        &count_sql,
        &params,
        &page,
    )
    .await?;
    Ok(serde_json::to_string(&PageResult { rows, total }).unwrap_or_default())
}
