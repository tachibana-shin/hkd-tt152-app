#![allow(unused_imports)]

use crate::helpers::*;
use crate::models::*;
use serde_json::json;
use sqlx::sqlite::{SqlitePoolOptions, SqliteTransaction};
use sqlx::SqlitePool;
use tauri::{AppHandle, Manager, State};

/// helper cho admin: thư mục backups (tạo nếu chưa có)
pub(crate) fn backups_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let dir = dir.join("backups");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}
/// Sao lưu cơ sở dữ liệu vào thư mục backups (kèm WAL checkpoint)
#[tauri::command]
pub(crate) async fn create_backup(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    require_role(&state, &["admin"]).await?;
    // PRAGMA không khai báo kiểu cột (NULL) nên macro sqlx không map được →
    // giữ runtime query, đây là câu lệnh bảo trì DB chứ không đụng schema.
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&*state.pool.read().await)
        .await
        .map_err(|e| e.to_string())?;
    let dir = backups_dir(&app)?;
    let filename = format!(
        "hkd-backup-{}.db",
        chrono::Local::now().format("%Y%m%d-%H%M%S")
    );
    // DB của hồ sơ đang mở (multi-profile): profiles/<key>/hkd.db
    let src = crate::commands::profile::active_db_path(&app)?;
    std::fs::copy(&src, dir.join(&filename)).map_err(|e| e.to_string())?;
    audit(&state, "backup", "database", &filename).await;
    Ok(filename)
}

/// Liệt kê các bản sao lưu (mới nhất trước)
#[tauri::command]
pub(crate) async fn list_backups(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    require_role(&state, &["admin"]).await?;
    let dir = backups_dir(&app)?;
    let mut files: Vec<String> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.ends_with(".db") {
                files.push(name);
            }
        }
    }
    files.sort();
    files.reverse();
    Ok(serde_json::to_string(&files).unwrap_or_default())
}

/// Khôi phục từ bản sao lưu (cần khởi động lại ứng dụng)
#[tauri::command]
pub(crate) async fn restore_backup(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    filename: String,
) -> Result<String, String> {
    require_role(&state, &["admin"]).await?;
    if filename.contains('/') || filename.contains('\\') || filename.contains("..") {
        return Err("Tên file không hợp lệ".into());
    }
    let dir = backups_dir(&app)?;
    let src = dir.join(&filename);
    if !src.is_file() {
        return Err("Bản sao lưu không tồn tại".into());
    }
    // Ghi đè file DB của hồ sơ đang mở (multi-profile): profiles/<key>/hkd.db
    let dst = crate::commands::profile::active_db_path(&app)?;
    std::fs::copy(&src, &dst).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(format!("{}-wal", dst.display()));
    let _ = std::fs::remove_file(format!("{}-shm", dst.display()));
    audit(&state, "restore", "database", &filename).await;
    Ok("ok".into())
}
