#![allow(unused_imports)]

use crate::helpers::*;
use crate::models::*;
use serde_json::json;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tauri::{AppHandle, Manager, State};

// ─── HỒ SƠ HKD (multi-profile): mỗi hộ kinh doanh = 1 thư mục riêng chứa 1 file DB riêng ───
// Cấu trúc:
//   app_data_dir/
//   ├── active_profile.json            ← hồ sơ đang mở { key, name }
//   └── profiles/
//       └── <key>/
//           ├── hkd.db                 ← toàn bộ dữ liệu của hồ sơ đó (migration + seed admin riêng)
//           └── profile.json           ← { name, created_at }

#[derive(serde::Serialize)]
pub(crate) struct ProfileRow {
    pub(crate) key: String,
    pub(crate) name: String,
    pub(crate) created_at: String,
    pub(crate) active: bool,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ProfileMeta {
    name: String,
    #[serde(default)]
    created_at: String,
}

fn profiles_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .map(|d| d.join("profiles"))
        .unwrap_or_else(|_| PathBuf::from("profiles"))
}

fn profile_dir(app: &AppHandle, key: &str) -> PathBuf {
    profiles_dir(app).join(key)
}

fn validate_key(key: &str) -> Result<(), String> {
    if key.is_empty()
        || key.len() > 64
        || !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err("Khóa hồ sơ không hợp lệ".into());
    }
    Ok(())
}

// ─── Helpers dùng chung (cả từ setup của lib.rs) ───

pub(crate) fn read_active_key(app_dir: &Path) -> Option<String> {
    let f = app_dir.join("active_profile.json");
    let raw = std::fs::read_to_string(f).ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    v.get("key").and_then(|k| k.as_str()).map(|s| s.to_string())
}

pub(crate) fn profile_name(dir: &Path, fallback: &str) -> String {
    let raw = std::fs::read_to_string(dir.join("profile.json")).unwrap_or_default();
    serde_json::from_str::<ProfileMeta>(&raw)
        .map(|m| m.name)
        .unwrap_or_else(|_| fallback.to_string())
}

pub(crate) fn profile_created_at(dir: &Path) -> String {
    let raw = std::fs::read_to_string(dir.join("profile.json")).unwrap_or_default();
    serde_json::from_str::<ProfileMeta>(&raw)
        .map(|m| m.created_at)
        .unwrap_or_default()
}

pub(crate) fn write_profile_meta(dir: &Path, name: &str) {
    let meta = ProfileMeta {
        name: name.to_string(),
        created_at: chrono_now(),
    };
    let _ = std::fs::write(dir.join("profile.json"), serde_json::to_string_pretty(&meta).unwrap_or_default());
}

pub(crate) fn set_active(app_dir: &Path, key: &str, name: &str) {
    let _ = std::fs::write(
        app_dir.join("active_profile.json"),
        serde_json::to_string(&json!({ "key": key, "name": name })).unwrap_or_default(),
    );
}

/// Đường dẫn file DB của hồ sơ đang mở (dùng cho backup/restore — DB giờ nằm trong `profiles/<key>/`).
pub(crate) fn active_db_path(app: &AppHandle) -> Result<PathBuf, String> {
    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let key = read_active_key(&app_dir)
        .filter(|k| app_dir.join("profiles").join(k).join("hkd.db").is_file())
        .unwrap_or_else(|| "default".to_string());
    Ok(app_dir.join("profiles").join(key).join("hkd.db"))
}

/// Thời điểm hiện tại (giờ địa phương) dạng "YYYY-MM-DD HH:MM:SS".
fn chrono_now() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Mở pool SQLite cho 1 file DB (WAL + FK qua builder — SQLx chỉ nhận
/// mode/cache/immutable/vfs trong URL, nên không dùng `?journal_mode=` được).
async fn open_pool(db_path: &Path) -> Result<SqlitePool, String> {
    let opts = SqliteConnectOptions::from_str(&format!("sqlite:{}", db_path.display()))
        .map_err(|e| e.to_string())?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true);
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
        .map_err(|e| e.to_string())
}

/// Migration + seed admin mặc định cho 1 file DB (nghiệp vụ nào mở DB cũng gọi).
pub(crate) async fn init_database(pool: &SqlitePool) {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .expect("failed to run migrations");
    crate::commands::auth::seed_default_users(pool).await;
}

fn now_secs() -> i64 {
    chrono::Utc::now().timestamp()
}

fn unique_key() -> String {
    let n: u32 = rand::random();
    format!("p{}_{:04x}", now_secs(), n & 0xffff)
}

// ─── Commands ───

/// Danh sách hồ sơ HKD trên máy (mỗi hồ sơ = 1 folder chứa hkd.db) + hồ sơ đang mở.
#[tauri::command]
pub(crate) async fn get_profiles(app: AppHandle) -> Result<String, String> {
    let dir = profiles_dir(&app);
    if !dir.is_dir() {
        return Ok(json!([]).to_string());
    }
    let active = read_active_key(
        &app.path().app_data_dir().map_err(|e| e.to_string())?,
    )
    .unwrap_or_default();
    let mut out: Vec<ProfileRow> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let key = e.file_name().to_string_lossy().to_string();
            let pdir = e.path();
            if !pdir.join("hkd.db").is_file() {
                continue;
            }
            out.push(ProfileRow {
                name: profile_name(&pdir, &key),
                created_at: profile_created_at(&pdir),
                active: key == active,
                key,
            });
        }
    }
    out.sort_by(|a, b| b.active.cmp(&a.active).then(a.name.cmp(&b.name)));
    Ok(json!(out).to_string())
}

/// Tạo hồ sơ HKD mới (folder riêng + DB riêng, migration + admin mặc định). Chỉ admin.
#[tauri::command]
pub(crate) async fn create_profile(
    state: State<'_, AppState>,
    app: AppHandle,
    name: String,
) -> Result<String, String> {
    require_role(&state, &["admin"]).await?;
    let name = name.trim().to_string();
    if name.is_empty() || name.len() > 60 {
        return Err("Tên hồ sơ phải có 1–60 ký tự".into());
    }
    let key = unique_key();
    let dir = profile_dir(&app, &key);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    write_profile_meta(&dir, &name);

    // Mở DB mới + chạy migration + seed admin (đóng pool ngay sau khi khởi tạo xong)
    let pool = open_pool(&dir.join("hkd.db")).await?;
    init_database(&pool).await;
    drop(pool);

    audit(&state, "create_profile", "profile", &key).await;
    let row = ProfileRow {
        name,
        created_at: profile_created_at(&dir),
        active: false,
        key: key.clone(),
    };
    Ok(json!(row).to_string())
}

/// Đổi tên hồ sơ (chỉ sửa profile.json). Chỉ admin.
#[tauri::command]
pub(crate) async fn rename_profile(
    state: State<'_, AppState>,
    app: AppHandle,
    key: String,
    name: String,
) -> Result<String, String> {
    require_role(&state, &["admin"]).await?;
    validate_key(&key)?;
    let name = name.trim().to_string();
    if name.is_empty() || name.len() > 60 {
        return Err("Tên hồ sơ phải có 1–60 ký tự".into());
    }
    let dir = profile_dir(&app, &key);
    if !dir.join("hkd.db").is_file() {
        return Err("Hồ sơ không tồn tại".into());
    }
    write_profile_meta(&dir, &name);
    // Nếu đang mở → cập nhật luôn file active để tên hiển thị ở màn hình đăng nhập
    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    if read_active_key(&app_dir).as_deref() == Some(key.as_str()) {
        set_active(&app_dir, &key, &name);
    }
    audit(&state, "rename_profile", "profile", &key).await;
    Ok(json!({ "ok": true, "key": key, "name": name }).to_string())
}

/// Xóa hồ sơ (xóa cả thư mục DB). Không xóa được hồ sơ đang mở. Chỉ admin.
#[tauri::command]
pub(crate) async fn delete_profile(
    state: State<'_, AppState>,
    app: AppHandle,
    key: String,
) -> Result<String, String> {
    require_role(&state, &["admin"]).await?;
    validate_key(&key)?;
    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    if read_active_key(&app_dir).as_deref() == Some(key.as_str()) {
        return Err("Không thể xóa hồ sơ đang mở. Hãy chuyển sang hồ sơ khác trước.".into());
    }
    let dir = profile_dir(&app, &key);
    if !dir.join("hkd.db").is_file() {
        return Err("Hồ sơ không tồn tại".into());
    }
    std::fs::remove_dir_all(&dir).map_err(|e| format!("Không xóa được hồ sơ: {}", e))?;
    audit(&state, "delete_profile", "profile", &key).await;
    Ok(json!({ "ok": true, "key": key }).to_string())
}

/// Mở hồ sơ HKD khác NGAY khi đang chạy: swap pool SQLite trong AppState,
/// đồng thời xóa phiên đăng nhập (mỗi hồ sơ có bộ người dùng riêng → phải đăng nhập lại).
#[tauri::command]
pub(crate) async fn switch_profile(
    state: State<'_, AppState>,
    app: AppHandle,
    key: String,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan", "kho", "xem"]).await?;
    validate_key(&key)?;
    let dir = profile_dir(&app, &key);
    if !dir.join("hkd.db").is_file() {
        return Err("Hồ sơ không tồn tại".into());
    }
    let name = profile_name(&dir, &key);

    // Mở pool mới + đảm bảo migration/seed (an toàn nếu tạo thủ công)
    let new_pool = open_pool(&dir.join("hkd.db")).await?;
    init_database(&new_pool).await;

    // Ghi audit với người dùng CŨ, rồi xóa phiên → buộc đăng nhập lại ở hồ sở mới
    audit(&state, "switch_profile", "profile", &key).await;
    *state.current_user.lock().await = None;

    // Ghi hồ sơ đang mở, rồi swap pool
    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    set_active(&app_dir, &key, &name);
    *state.pool.write().await = new_pool;

    Ok(json!({ "ok": true, "key": key, "name": name, "active": true }).to_string())
}

// ─── TÙY CHỌN ĐĂNG NHẬP THEO HỒ SƠ ───
// Lưu trong `app_data_dir/profile_prefs.json`: { "<key>": { last_username, auto_login, username, password } }.
// `password` là mật khẩu đã lưu để tự động đăng nhập — app local máy đơn, chấp nhận
// lưu trong file dữ liệu của app (tương tự profile.json).

#[derive(serde::Serialize, serde::Deserialize, Clone, Default)]
pub(crate) struct ProfilePrefs {
    /// Tên đăng nhập nhập lần trước ở màn hình đăng nhập của hồ sơ này.
    #[serde(default)]
    pub(crate) last_username: Option<String>,
    /// Có tự động đăng nhập hồ sơ này khi khởi động app không.
    #[serde(default)]
    pub(crate) auto_login: bool,
    /// Tài khoản dùng để tự động đăng nhập (lưu khi bật auto_login).
    #[serde(default)]
    pub(crate) username: Option<String>,
    #[serde(default)]
    pub(crate) password: Option<String>,
}

fn prefs_file(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .map(|d| d.join("profile_prefs.json"))
        .unwrap_or_else(|_| PathBuf::from("profile_prefs.json"))
}

fn read_prefs(app: &AppHandle) -> HashMap<String, ProfilePrefs> {
    let raw = std::fs::read_to_string(prefs_file(app)).unwrap_or_default();
    serde_json::from_str(&raw).unwrap_or_default()
}

fn write_prefs(app: &AppHandle, map: &HashMap<String, ProfilePrefs>) -> Result<(), String> {
    std::fs::write(
        prefs_file(app),
        serde_json::to_string_pretty(map).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

/// Toàn bộ tùy chọn đăng nhập của mọi hồ sơ. Đọc được khi CHƯA đăng nhập —
/// cần cho màn hình chọn hồ sơ / màn hình đăng nhập lúc khởi động.
#[tauri::command]
pub(crate) async fn get_profile_prefs(app: AppHandle) -> Result<String, String> {
    Ok(serde_json::to_string(&read_prefs(&app)).map_err(|e| e.to_string())?)
}

/// Ghi (ghi đè) tùy chọn đăng nhập cho 1 hồ sơ. Không cần đăng nhập.
#[tauri::command]
pub(crate) async fn save_profile_pref(
    app: AppHandle,
    key: String,
    prefs: ProfilePrefs,
) -> Result<String, String> {
    validate_key(&key)?;
    let mut map = read_prefs(&app);
    map.insert(key, prefs);
    write_prefs(&app, &map)?;
    Ok("ok".into())
}

/// Mở hồ sơ từ màn hình chọn hồ sơ lúc KHỞI ĐỘNG (chưa cần đăng nhập): đổi hồ sơ
/// đang mở + swap pool SQLite; frontend sẽ đưa người dùng tới màn hình đăng nhập
/// của hồ sơ mới. Khác `switch_profile` (yêu cầu đăng nhập + ghi audit).
#[tauri::command]
pub(crate) async fn select_profile(
    state: State<'_, AppState>,
    app: AppHandle,
    key: String,
) -> Result<String, String> {
    validate_key(&key)?;
    let dir = profile_dir(&app, &key);
    if !dir.join("hkd.db").is_file() {
        return Err("Hồ sơ không tồn tại".into());
    }
    let name = profile_name(&dir, &key);

    // Mở pool mới + đảm bảo migration/seed (an toàn nếu tạo thủ công)
    let new_pool = open_pool(&dir.join("hkd.db")).await?;
    init_database(&new_pool).await;

    // Chưa đăng nhập ở thời điểm này (khởi động) nhưng xóa phiên cho chắc chắn
    *state.current_user.lock().await = None;

    // Ghi hồ sơ đang mở, rồi swap pool
    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    set_active(&app_dir, &key, &name);
    *state.pool.write().await = new_pool;

    Ok(json!({ "ok": true, "key": key, "name": name, "active": true }).to_string())
}