#![allow(unused_imports)]

use crate::helpers::*;
use crate::models::*;
use serde_json::json;
use sqlx::SqlitePool;
use tauri::{AppHandle, Manager, State};

/// Tạo user admin mặc định (admin/admin123) khi khởi động nếu chưa có.
/// Chạy SAU migration, từ code — vì băm SHA-256 dùng salt ngẫu nhiên không seed được trong SQL.
pub(crate) async fn seed_default_users(pool: &SqlitePool) {
    // Nếu chưa có user nào → tạo admin/admin123
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM app_user")
        .fetch_one(pool)
        .await
        .unwrap_or(0);
    if count == 0 {
        let hash = hash_password("admin123");
        let _ = sqlx::query(
            "INSERT INTO app_user (username, display_name, password_hash, role) VALUES ('admin', 'Quản trị', ?, 'admin')",
        )
        .bind(hash)
        .execute(pool)
        .await;
        return;
    }
    // Đảm bảo luôn tồn tại ít nhất 1 tài khoản admin (phòng trường hợp bị xóa hết)
    let admins: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM app_user WHERE role = 'admin' AND active = 1")
            .fetch_one(pool)
            .await
            .unwrap_or(0);
    if admins == 0 {
        let hash = hash_password("admin123");
        let _ = sqlx::query(
            "INSERT INTO app_user (username, display_name, password_hash, role) VALUES ('admin', 'Quản trị', ?, 'admin')",
        )
        .bind(hash)
        .execute(pool)
        .await;
    }
}

/// Đăng nhập → lưu người dùng hiện tại vào state phiên làm việc.
#[tauri::command]
pub(crate) async fn login(
    state: State<'_, AppState>,
    username: String,
    password: String,
) -> Result<String, String> {
    let row: Option<(i64, String, String, String, bool)> = sqlx::query_as(
        "SELECT id, display_name, password_hash, role, active FROM app_user WHERE username = ?",
    )
    .bind(&username)
    .fetch_optional(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    let Some((id, display_name, password_hash, role, active)) = row else {
        return Err("Sai tên đăng nhập hoặc mật khẩu".into());
    };
    if !active {
        return Err("Tài khoản đã bị khóa. Liên hệ quản trị viên.".into());
    }
    if !verify_password(&password, &password_hash) {
        return Err("Sai tên đăng nhập hoặc mật khẩu".into());
    }
    let user = CurrentUser {
        id,
        username: username.clone(),
        display_name,
        role,
    };
    *state.current_user.lock().await = Some(user.clone());
    audit(&state, "login", "auth", &username).await;
    Ok(json!(user).to_string())
}

/// Đăng xuất — xóa người dùng hiện tại khỏi phiên.
#[tauri::command]
pub(crate) async fn logout(state: State<'_, AppState>) -> Result<String, String> {
    let username = state
        .current_user
        .lock()
        .await
        .as_ref()
        .map(|u| u.username.clone())
        .unwrap_or_default();
    *state.current_user.lock().await = None;
    *state.portal.lock().await = None; // đăng xuất app → xóa luôn phiên cổng HĐĐT
    if !username.is_empty() {
        audit(&state, "logout", "auth", &username).await;
    }
    Ok("ok".into())
}

/// Trả về người dùng hiện tại (để frontend khôi phục phiên khi mở app) — "null" nếu chưa đăng nhập.
#[tauri::command]
pub(crate) async fn get_current_user(state: State<'_, AppState>) -> Result<String, String> {
    let current = state.current_user.lock().await.clone();
    match current {
        Some(u) => Ok(json!(u).to_string()),
        None => Ok("null".into()),
    }
}

/// Danh sách người dùng (không gồm password_hash). Chỉ admin.
#[tauri::command]
pub(crate) async fn list_users(state: State<'_, AppState>) -> Result<String, String> {
    require_role(&state, &["admin"]).await?;
    let rows: Vec<UserRow> =
        sqlx::query_as("SELECT id, username, display_name, role, active FROM app_user ORDER BY id")
            .fetch_all(&*state.pool.read().await)
            .await
            .map_err(|e| e.to_string())?;
    Ok(json!(rows).to_string())
}

/// Thêm / sửa người dùng. Password không rỗng → đặt lại mật khẩu. Chỉ admin.
#[tauri::command]
pub(crate) async fn save_user(
    state: State<'_, AppState>,
    input: UserInput,
) -> Result<String, String> {
    require_role(&state, &["admin"]).await?;
    let roles = ["admin", "ketoan", "kho", "xem"];
    if !roles.contains(&input.role.as_str()) {
        return Err(format!(
            "Vai trò không hợp lệ '{}' (chấp nhận: {})",
            input.role,
            roles.join(", ")
        ));
    }
    let username = input.username.trim();
    if username.is_empty() {
        return Err("Thiếu tên đăng nhập".into());
    }
    let is_create = {
        let exists: Option<i64> = sqlx::query_scalar("SELECT id FROM app_user WHERE username = ?")
            .bind(username)
            .fetch_optional(&*state.pool.read().await)
            .await
            .map_err(|e| e.to_string())?;
        exists.is_none()
    };
    if is_create {
        if input.password.is_empty() {
            return Err("Người dùng mới phải có mật khẩu".into());
        }
        if input.role == "admin" {
            // không cho tạo thêm admin qua UI để tránh mất kiểm soát
            return Err("Không thể tạo tài khoản admin mới qua màn hình này. Hãy tạo tài khoản vai trò khác trước, hoặc liên hệ quản trị hệ thống.".into());
        }
        let hash = hash_password(&input.password);
        sqlx::query(
            "INSERT INTO app_user (username, display_name, password_hash, role, active)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(username)
        .bind(&input.display_name)
        .bind(hash)
        .bind(&input.role)
        .bind(input.active)
        .execute(&*state.pool.read().await)
        .await
        .map_err(|e| e.to_string())?;
        audit(&state, "create_user", "auth", username).await;
        Ok(json!({ "ok": true, "created": true }).to_string())
    } else {
        // Sửa: cập nhật thông tin + (tùy chọn) đổi mật khẩu
        if input.password.is_empty() {
            sqlx::query(
                "UPDATE app_user SET display_name = ?, role = ?, active = ? WHERE username = ?",
            )
            .bind(&input.display_name)
            .bind(&input.role)
            .bind(input.active)
            .bind(username)
            .execute(&*state.pool.read().await)
            .await
            .map_err(|e| e.to_string())?;
        } else {
            let hash = hash_password(&input.password);
            sqlx::query(
                "UPDATE app_user SET display_name = ?, password_hash = ?, role = ?, active = ? WHERE username = ?",
            )
            .bind(&input.display_name)
            .bind(hash)
            .bind(&input.role)
            .bind(input.active)
            .bind(username)
            .execute(&*state.pool.read().await)
            .await
            .map_err(|e| e.to_string())?;
        }
        audit(&state, "update_user", "auth", username).await;
        Ok(json!({ "ok": true, "created": false }).to_string())
    }
}

/// Xóa người dùng. Không tự xóa được chính mình; không xóa admin cuối cùng. Chỉ admin.
#[tauri::command]
pub(crate) async fn delete_user(state: State<'_, AppState>, id: i64) -> Result<String, String> {
    require_role(&state, &["admin"]).await?;
    let current = state.current_user.lock().await.clone();
    if let Some(u) = current {
        if u.id == id {
            return Err("Không thể xóa tài khoản đang đăng nhập".into());
        }
    }
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT username, role FROM app_user WHERE id = ?")
            .bind(id)
            .fetch_optional(&*state.pool.read().await)
            .await
            .map_err(|e| e.to_string())?;
    let Some((username, role)) = row else {
        return Err("Không tìm thấy người dùng".into());
    };
    if role == "admin" {
        let admins: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM app_user WHERE role = 'admin' AND active = 1 AND id != ?",
        )
        .bind(id)
        .fetch_one(&*state.pool.read().await)
        .await
        .map_err(|e| e.to_string())?;
        if admins == 0 {
            return Err("Không thể xóa tài khoản admin cuối cùng".into());
        }
    }
    sqlx::query("DELETE FROM app_user WHERE id = ?")
        .bind(id)
        .execute(&*state.pool.read().await)
        .await
        .map_err(|e| e.to_string())?;
    audit(&state, "delete_user", "auth", &username).await;
    Ok(json!({ "ok": true }).to_string())
}

/// Người dùng tự đổi mật khẩu của chính mình (cần mật khẩu cũ).
#[tauri::command]
pub(crate) async fn change_password(
    state: State<'_, AppState>,
    old_password: String,
    new_password: String,
) -> Result<String, String> {
    let current = state.current_user.lock().await.clone();
    let Some(user) = current else {
        return Err("Chưa đăng nhập".into());
    };
    if new_password.len() < 4 {
        return Err("Mật khẩu mới phải có ít nhất 4 ký tự".into());
    }
    let stored: String = sqlx::query_scalar("SELECT password_hash FROM app_user WHERE id = ?")
        .bind(user.id)
        .fetch_one(&*state.pool.read().await)
        .await
        .map_err(|e| e.to_string())?;
    if !verify_password(&old_password, &stored) {
        return Err("Mật khẩu cũ không đúng".into());
    }
    let hash = hash_password(&new_password);
    sqlx::query("UPDATE app_user SET password_hash = ? WHERE id = ?")
        .bind(hash)
        .bind(user.id)
        .execute(&*state.pool.read().await)
        .await
        .map_err(|e| e.to_string())?;
    audit(&state, "change_password", "auth", &user.username).await;
    Ok(json!({ "ok": true }).to_string())
}
