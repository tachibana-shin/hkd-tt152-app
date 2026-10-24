// ─── HDDT portal (electronic invoice) — frontend commands ───
//
// Login flow against hoadondientu.gdt.gov.vn:
//   - the account is stored in `app_setting` (local DB, one set per HKD profile) so
//     the portal token can be refreshed automatically;
//   - `hddt_login` solves the captcha OFFLINE (`GlyphTemplateSolver`) and retries;
//     when it cannot pass the captcha it returns `{need_manual:true}` so the UI can
//     ask the user to type the code (`hddt_captcha` + `hddt_login_manual`);
//   - the JWT lives ~1 day → the session tracks its `exp` and the UI re-logs in
//     before it expires.
//
// `hddt_status` / `hddt_send_simulated` used to be mocks; `hddt_status` now returns
// the real connection state, `hddt_send_simulated` is kept as the local fallback.

use crate::commands::progress::ProgressJob;
use crate::hddt::sync::CachedInvoice;
use crate::hddt::{
    days_left_until, generate_password, rfc3339_to_unix, sync, GlyphTemplateSolver, HddtClient,
    InvoiceDirection, InvoiceKind, InvoiceQuery, LoginError, PortalSession,
};
use crate::helpers::{audit, require_role};
use crate::models::AppState;
use base64::Engine;
use serde_json::{json, Value};
use sqlx::SqlitePool;
use tauri::State;

const KEY_USERNAME: &str = "hddt_username";
const KEY_PASSWORD: &str = "hddt_password";
const KEY_BASE_URL: &str = "hddt_base_url";
const KEY_AUTO_CHANGE: &str = "hddt_auto_change_password";

/// Refresh the token when less than this many seconds remain.
const RELOGIN_MARGIN_SECS: i64 = 3600;

/// Rotate the portal password automatically when it expires within this many days.
const AUTO_CHANGE_PASSWORD_DAYS: i64 = 7;

#[derive(sqlx::FromRow)]
struct SettingKV {
    key: String,
    value: String,
}

/// 1 dòng nhật ký mật khẩu cổng (`hddt_password_journal`).
#[derive(sqlx::FromRow)]
struct PasswordJournalRow {
    id: i64,
    password: String,
    state: String,
    note: String,
    created_at: String,
    marked_at: Option<String>,
}

#[derive(Default)]
struct HddtConfig {
    username: String,
    password: String,
    base_url: String,
    /// Rotate the portal password automatically when it is about to expire.
    /// Off by default: recovering a lost portal password is hard, so the user
    /// must opt in explicitly from the UI switch.
    auto_change_password: bool,
}

impl HddtConfig {
    /// Ready to log in when both username and password are stored.
    fn configured(&self) -> bool {
        !self.username.trim().is_empty() && !self.password.is_empty()
    }

    /// Custom portal base URL, or `None` for the official one.
    fn base(&self) -> Option<&str> {
        let b = self.base_url.trim();
        if b.is_empty() {
            None
        } else {
            Some(b)
        }
    }
}

/// Read the HDDT account from `app_setting` (runtime query — the table is a plain
/// key/value store, so no schema change is needed).
async fn read_config(state: &State<'_, AppState>) -> Result<HddtConfig, String> {
    let rows: Vec<SettingKV> = sqlx::query_as::<_, SettingKV>(
        "SELECT key, value FROM app_setting WHERE key IN (?, ?, ?, ?)",
    )
    .bind(KEY_USERNAME)
    .bind(KEY_PASSWORD)
    .bind(KEY_BASE_URL)
    .bind(KEY_AUTO_CHANGE)
    .fetch_all(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;

    let mut cfg = HddtConfig::default();
    for r in rows {
        match r.key.as_str() {
            KEY_USERNAME => cfg.username = r.value,
            KEY_PASSWORD => cfg.password = r.value,
            KEY_BASE_URL => cfg.base_url = r.value,
            KEY_AUTO_CHANGE => cfg.auto_change_password = r.value == "1",
            _ => {}
        }
    }
    Ok(cfg)
}

/// Upsert a single `app_setting` key.
async fn upsert(state: &State<'_, AppState>, key: &str, value: &str) -> Result<(), String> {
    sqlx::query!(
        "INSERT INTO app_setting (key, value) VALUES (?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        key,
        value
    )
    .execute(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

// ─── Nhật ký mật khẩu cổng (migration 20261022001) — chống khóa tài khoản ───
//
// Yêu cầu 22/10/2026: mọi mật khẩu từng dùng/lưu phải có trong bảng riêng
// (`hddt_password_journal`); khi cổng phản hồi "mật khẩu không đúng" thì đánh
// dấu `bad` và KHÔNG tự thử lại mật khẩu đó nữa (cổng khóa tạm tài khoản do
// nhập sai nhiều lần). Khi tài khoản đăng nhập thành công → reset các dấu
// `bad` của tài khoản và ghi mật khẩu đang dùng thành `ok`.

const JOURNAL_OK: &str = "ok";
const JOURNAL_BAD: &str = "bad";

/// Phản hồi của cổng coi là "mật khẩu không đúng": message có "mật khẩu" hoặc
/// báo khóa tài khoản, và KHÔNG phải lỗi captcha ("Mã captcha không đúng.").
fn is_wrong_password_message(err: &str) -> bool {
    let m = err.to_lowercase();
    if m.contains("captcha") {
        return false;
    }
    m.contains("mật khẩu") || (m.contains("tài khoản") && m.contains("khóa"))
}

/// Ghi nhận mật khẩu vào nhật ký (idempotent — row có sẵn thì giữ nguyên state
/// đã biết, chỉ bổ sung lần ghi đầu). Không bao giờ xóa dòng: mật khẩu cũ của
/// người dùng không mất khi đổi mật khẩu.
async fn journal_note(
    pool: &SqlitePool,
    username: &str,
    password: &str,
    note: &str,
) -> Result<(), String> {
    sqlx::query!(
        "INSERT INTO hddt_password_journal (username, password, note)
         VALUES (?, ?, ?)
         ON CONFLICT(username, password) DO NOTHING",
        username,
        password,
        note
    )
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Ghi kết quả thử gần nhất của mật khẩu (ok/bad) — tạo dòng nếu chưa có.
async fn journal_mark(
    pool: &SqlitePool,
    username: &str,
    password: &str,
    new_state: &str,
) -> Result<(), String> {
    sqlx::query!(
        "INSERT INTO hddt_password_journal (username, password, state, marked_at)
         VALUES (?, ?, ?, datetime('now'))
         ON CONFLICT(username, password)
         DO UPDATE SET state = excluded.state, marked_at = datetime('now')",
        username,
        password,
        new_state
    )
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Đăng nhập *thành công* (auto lẫn manual) → theo quy ước: reset mọi dấu
/// `bad` của TÀI KHOẠN (mật khẩu cũ được phép thử lại bằng tay) và ghi mật
/// khẩu đang dùng thành `ok`.
async fn journal_record_ok(
    pool: &SqlitePool,
    username: &str,
    password: &str,
) -> Result<(), String> {
    journal_mark(pool, username, password, JOURNAL_OK).await?;
    sqlx::query!(
        "UPDATE hddt_password_journal
            SET state = 'unknown', marked_at = NULL
          WHERE username = ? AND state = 'bad'",
        username
    )
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Mật khẩu active đã bị cổng từ chối (state `bad`) → cấm tự thử lại.
async fn journal_is_bad(pool: &SqlitePool, username: &str, password: &str) -> Result<bool, String> {
    if username.is_empty() || password.is_empty() {
        return Ok(false);
    }
    let state_str: Option<String> = sqlx::query_scalar!(
        "SELECT state FROM hddt_password_journal WHERE username = ? AND password = ?",
        username,
        password
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(state_str.as_deref() == Some(JOURNAL_BAD))
}

/// Current connection state (never exposes the token or the password).
async fn status_value(state: &State<'_, AppState>) -> Result<Value, String> {
    let cfg = read_config(state).await?;
    let sess = state.portal.lock().await.clone();
    let logged_in = sess.as_ref().map(|s| s.seconds_left() > 0).unwrap_or(false);
    let username = if !cfg.username.is_empty() {
        cfg.username.clone()
    } else {
        sess.as_ref()
            .map(|s| s.username.clone())
            .unwrap_or_default()
    };
    // Mật khẩu active đang bị đánh dấu "cổng từ chối" — UI bật banner đỏ nhắc sửa.
    let active_password_bad =
        journal_is_bad(&*state.pool.read().await, &cfg.username, &cfg.password)
            .await
            .unwrap_or(false);
    Ok(json!({
        "configured": cfg.configured(),
        "username": username,
        "logged_in": logged_in,
        "expires_at": sess.as_ref().map(|s| s.expires_at).unwrap_or(0),
        "seconds_left": sess.as_ref().map(|s| s.seconds_left()).unwrap_or(0),
        "relogin_margin": RELOGIN_MARGIN_SECS,
        "user": sess.as_ref().and_then(|s| s.user.clone()),
        "auto_change_password": cfg.auto_change_password,
        "active_password_bad": active_password_bad,
    }))
}

/// Read the stored HDDT account (password is never returned — only `has_password`).
#[tauri::command]
pub(crate) async fn hddt_get_config(state: State<'_, AppState>) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let cfg = read_config(&state).await?;
    Ok(json!({
        "username": cfg.username,
        "base_url": cfg.base_url,
        "has_password": !cfg.password.is_empty(),
        "configured": cfg.configured(),
        "auto_change_password": cfg.auto_change_password,
    })
    .to_string())
}

/// Đọc mật khẩu HĐĐT đã lưu (Plaintext — chỉ dùng trong app local, có phân quyền).
#[tauri::command]
pub(crate) async fn hddt_get_password(state: State<'_, AppState>) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let cfg = read_config(&state).await?;
    if cfg.password.is_empty() {
        return Err("Chưa lưu mật khẩu.".into());
    }
    // Trả JSON-stringify (như mọi lệnh khác) — frontend đọc bằng `parse()` =
    // JSON.parse đúng một lần; trả chuỗi thô sẽ làm JSON.parse ném SyntaxError
    // với mật khẩu thường → app không bao giờ soi được mật khẩu đã lưu.
    Ok(json!(cfg.password).to_string())
}

/// Danh sách mật khẩu từng lưu/thử của tài khoản hiện tại (nhân sự có quyền —
/// trả plaintext như `hddt_get_password`, app local không mã hóa).
#[tauri::command]
pub(crate) async fn hddt_password_journal(state: State<'_, AppState>) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let cfg = read_config(&state).await?;
    let rows: Vec<PasswordJournalRow> = sqlx::query_as!(
        PasswordJournalRow,
        "SELECT id as \"id!\", password, state, note, created_at, marked_at
           FROM hddt_password_journal
          WHERE username = ?
          ORDER BY id DESC",
        cfg.username
    )
    .fetch_all(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    // Nối `is_current` trước khi render: so sánh trong Rust thôi (mật khẩu
    // không bao giờ xuất hiện 2 lần → the danh sách chắc chắn đúng).
    let out: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.id,
                "password": r.password,
                "state": r.state,
                "note": r.note,
                "created_at": r.created_at,
                "marked_at": r.marked_at,
                "is_current": r.password == cfg.password,
            })
        })
        .collect();
    Ok(json!(out).to_string())
}

/// Chọn một mật khẩu trong nhật ký làm mật khẩu active ("Dùng" — cứu khi lỡ
/// lưu sai mật khẩu; không phải gõ lại mật khẩu còn hiệu lực).
#[tauri::command]
pub(crate) async fn hddt_use_journal_password(
    state: State<'_, AppState>,
    id: i64,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let cfg = read_config(&state).await?;
    let password: Option<String> = sqlx::query_scalar!(
        "SELECT password FROM hddt_password_journal WHERE id = ? AND username = ?",
        id,
        cfg.username
    )
    .fetch_optional(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    let Some(password) = password else {
        return Err(
            "Mật khẩu không có trong nhật ký của tài khoản này (có thể vừa đổi tên đăng nhập)."
                .into(),
        );
    };
    upsert(&state, KEY_PASSWORD, &password).await?;
    *state.portal.lock().await = None; // credentials changed → token no longer trusted
    audit(&state, "hddt_use_journal_password", "hddt", &cfg.username).await;
    Ok(json!({
        "ok": true,
        "status": status_value(&state).await?,
    })
    .to_string())
}

/// Store the HDDT account. An empty `password` keeps the existing one; an empty
/// `base_url` resets to the official portal. Any change clears the cached token.
#[tauri::command]
pub(crate) async fn hddt_save_config(
    state: State<'_, AppState>,
    username: String,
    password: String,
    base_url: String,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let username = username.trim().to_string();
    if username.is_empty() {
        return Err("Tên đăng nhập HĐĐT không được để trống.".into());
    }
    upsert(&state, KEY_USERNAME, &username).await?;
    if !password.is_empty() {
        upsert(&state, KEY_PASSWORD, &password).await?;
        // Nhật ký: mật khẩu mới chưa thử → giữ state đã biết (ok/bad),
        // lần đầu = 'unknown'. Mật khẩu cũ vẫn nguyên dòng trong bảng.
        journal_note(&*state.pool.read().await, &username, &password, "lưu tay").await?;
    }
    upsert(&state, KEY_BASE_URL, base_url.trim()).await?;
    *state.portal.lock().await = None; // credentials changed → token no longer trusted
    audit(&state, "hddt_save_config", "hddt", &username).await;

    let cfg = read_config(&state).await?;
    Ok(json!({
        "username": cfg.username,
        "base_url": cfg.base_url,
        "has_password": !cfg.password.is_empty(),
        "configured": cfg.configured(),
        "auto_change_password": cfg.auto_change_password,
    })
    .to_string())
}

/// Real connection status (no side effects).
#[tauri::command]
pub(crate) async fn hddt_status(state: State<'_, AppState>) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    Ok(status_value(&state).await?.to_string())
}

/// Log in from scratch (solving the captcha offline), store the session and
/// return the standard login result.
async fn do_full_login(state: &State<'_, AppState>) -> Result<Value, String> {
    let cfg = read_config(state).await?;
    if !cfg.configured() {
        return Err(
            "Chưa cấu hình tài khoản HĐĐT. Vui lòng nhập tên đăng nhập và mật khẩu.".into(),
        );
    }
    // Mật khẩu active đã bị cổng từ chối → KHÔNG tự động thử lại. Cổng khóa tạm
    // tài khoản nếu nhập sai liên tiếp; người dùng phải sửa mật khẩu trên màn
    // HĐĐT trước (đúng màu banner đỏ).
    if journal_is_bad(&*state.pool.read().await, &cfg.username, &cfg.password).await? {
        return Ok(json!({
            "ok": false,
            "need_manual": false,
            "blocked_password": true,
            "reason": "Mật khẩu đang lưu đã bị cổng từ chối (sai). Vui lòng sửa mật khẩu trên màn HĐĐT — app không tự thử lại để tránh khóa tài khoản.",
        }));
    }
    let client = HddtClient::new(&cfg.username, &cfg.password, cfg.base())?;
    match client.login_with_solver(&GlyphTemplateSolver, 3).await {
        Ok(login) => {
            let session = PortalSession::new(cfg.username.clone(), login.token, login.user);
            *state.portal.lock().await = Some(session);
            // Thành công → ghi mật khẩu đang dùng `ok` + reset các dấu `bad` của tài khoản.
            journal_record_ok(&*state.pool.read().await, &cfg.username, &cfg.password).await?;
            Ok(json!({
                "ok": true,
                "need_manual": false,
                "status": status_value(state).await?,
            }))
        }
        Err(LoginError::Captcha) => Ok(json!({
            "ok": false,
            "need_manual": true,
            "reason": "Không giải được captcha tự động sau 3 lần. Vui lòng nhập mã captcha thủ công.",
        })),
        Err(LoginError::Other(e)) => {
            if is_wrong_password_message(&e) {
                // Cổng rõ ràng phản hồi mật khẩu sai → đánh dấu, lần thử sau bị chặn.
                journal_mark(
                    &*state.pool.read().await,
                    &cfg.username,
                    &cfg.password,
                    JOURNAL_BAD,
                )
                .await?;
            }
            Err(e)
        }
    }
}

/// Change the portal password to a freshly generated one and store it.
/// The portal invalidates the current session, so the caller must log in
/// again with the new password. Returns the new password (to show the user).
async fn perform_change_password(state: &State<'_, AppState>) -> Result<String, String> {
    let cfg = read_config(state).await?;
    if !cfg.configured() {
        return Err("Chưa cấu hình tài khoản HĐĐT.".into());
    }
    // The portal requires a live session.
    let token = {
        let sess = state.portal.lock().await;
        match sess.as_ref() {
            Some(s) if s.seconds_left() > 0 => s.token.clone(),
            _ => {
                return Err(
                    "Chưa đăng nhập cổng HĐĐT nên không thể đổi mật khẩu. Hãy đăng nhập trước."
                        .into(),
                )
            }
        }
    };
    let client = HddtClient::new(&cfg.username, &cfg.password, cfg.base())?;
    // The portal rejects reusing the current password.
    let mut new_password = generate_password();
    while new_password == cfg.password {
        new_password = generate_password();
    }
    client
        .change_password(&token, &cfg.password, &new_password)
        .await?;
    upsert(state, KEY_PASSWORD, &new_password).await?;
    // Nhật ký: mật khẩu mới do app sinh ra (cổng đã xác nhận đổi) — note riêng
    // để phân biệt với mật khẩu người dùng lưu tay; đăng nhập kế tiếp sẽ mark ok.
    journal_note(
        &*state.pool.read().await,
        &cfg.username,
        &new_password,
        "tự đổi",
    )
    .await?;
    *state.portal.lock().await = None; // the portal invalidated the old token
    audit(state, "hddt_change_password", "hddt", "rotated").await;
    Ok(new_password)
}

/// Rotate the password proactively when it expires within
/// `AUTO_CHANGE_PASSWORD_DAYS` and the user enabled the switch.
/// Returns the new password when a change was performed.
async fn maybe_auto_change_password(state: &State<'_, AppState>) -> Result<Option<String>, String> {
    let cfg = read_config(state).await?;
    if !cfg.configured() || !cfg.auto_change_password {
        return Ok(None);
    }
    let expire = {
        let sess = state.portal.lock().await;
        sess.as_ref()
            .and_then(|s| s.user.as_ref())
            .and_then(|u| u.get("password_expire"))
            .and_then(|v| v.as_str())
            .and_then(rfc3339_to_unix)
    };
    let Some(expire) = expire else {
        return Ok(None);
    };
    if days_left_until(expire) > AUTO_CHANGE_PASSWORD_DAYS {
        return Ok(None);
    }
    perform_change_password(state).await.map(Some)
}

/// Ensure logged in: rotate the password proactively if needed, reuse the
/// cached token while it is still valid, otherwise solve the captcha
/// offline and log in.
///
/// On success → `{ok:true, need_manual:false, status[, password_rotated, new_password]}`.
/// When the captcha cannot be solved automatically → `{ok:false, need_manual:true, reason}`.
#[tauri::command]
pub(crate) async fn hddt_login(state: State<'_, AppState>) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;

    // Rotate the password first if it is about to expire and the option is on.
    let rotated = maybe_auto_change_password(&state).await?;

    // Reuse a token that is still comfortably valid (unless we just rotated it).
    if rotated.is_none() {
        let sess = state.portal.lock().await;
        if let Some(s) = sess.as_ref() {
            if s.is_valid(RELOGIN_MARGIN_SECS) {
                drop(sess);
                return Ok(json!({
                    "ok": true,
                    "need_manual": false,
                    "status": status_value(&state).await?,
                })
                .to_string());
            }
        }
    }

    let mut result = do_full_login(&state).await?;
    audit(&state, "hddt_login", "hddt", "auto").await;
    if let Some(pw) = rotated {
        result["password_rotated"] = json!(true);
        result["new_password"] = json!(pw);
    }
    Ok(result.to_string())
}

/// Fetch a fresh captcha for manual entry (fallback when auto-solve fails).
#[tauri::command]
pub(crate) async fn hddt_captcha(state: State<'_, AppState>) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let cfg = read_config(&state).await?;
    if !cfg.configured() {
        return Err("Chưa cấu hình tài khoản HĐĐT.".into());
    }
    let client = HddtClient::new(&cfg.username, &cfg.password, cfg.base())?;
    let cap = client.fetch_captcha().await?;
    Ok(json!({ "key": cap.key, "svg": cap.content }).to_string())
}

/// Log in with a captcha typed by the user (manual fallback).
#[tauri::command]
pub(crate) async fn hddt_login_manual(
    state: State<'_, AppState>,
    captcha_key: String,
    captcha_value: String,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let value = captcha_value.trim();
    if value.is_empty() {
        return Err("Vui lòng nhập mã captcha.".into());
    }
    let cfg = read_config(&state).await?;
    if !cfg.configured() {
        return Err("Chưa cấu hình tài khoản HĐĐT.".into());
    }
    let client = HddtClient::new(&cfg.username, &cfg.password, cfg.base())?;
    // Gate giống do_full_login: mật khẩu active bị đánh dấu "cổng từ chối"
    // thì không bắn authenticate nữa — kể cả captcha người dùng vô đúng.
    if journal_is_bad(&*state.pool.read().await, &cfg.username, &cfg.password).await? {
        return Ok(json!({
            "ok": false,
            "need_manual": false,
            "blocked_password": true,
            "reason": "Mật khẩu đang lưu đã bị cổng từ chối (sai). Sửa mật khẩu ở ô trên rồi bấm Lưu tài khoản — app không tự thử lại để tránh khóa tài khoản.",
        }).to_string());
    }
    let token = match client.authenticate(&captcha_key, value).await {
        Ok(t) => t,
        Err(e) => {
            if is_wrong_password_message(&e) {
                journal_mark(
                    &*state.pool.read().await,
                    &cfg.username,
                    &cfg.password,
                    JOURNAL_BAD,
                )
                .await?;
            }
            return Err(e);
        }
    };
    let user = client.profile(&token).await.ok();
    let session = PortalSession::new(cfg.username.clone(), token, user);
    *state.portal.lock().await = Some(session);
    journal_record_ok(&*state.pool.read().await, &cfg.username, &cfg.password).await?;
    audit(&state, "hddt_login", "hddt", "manual").await;
    Ok(json!({
        "ok": true,
        "need_manual": false,
        "status": status_value(&state).await?,
    })
    .to_string())
}

/// Drop the cached portal token (keeps the saved account).
#[tauri::command]
pub(crate) async fn hddt_logout(state: State<'_, AppState>) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    *state.portal.lock().await = None;
    audit(&state, "hddt_logout", "hddt", "").await;
    Ok("ok".into())
}

/// Enable/disable automatic password rotation.
#[tauri::command]
pub(crate) async fn hddt_set_auto_change_password(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    upsert(&state, KEY_AUTO_CHANGE, if enabled { "1" } else { "0" }).await?;
    audit(
        &state,
        "hddt_set_auto_change_password",
        "hddt",
        if enabled { "on" } else { "off" },
    )
    .await;
    Ok(json!({ "auto_change_password": enabled }).to_string())
}

/// Change the portal password on demand (user-initiated). The portal
/// invalidates the current session, so a fresh login follows.
/// Returns the new password once — keep it, recovering a lost portal
/// password is hard.
#[tauri::command]
pub(crate) async fn hddt_change_password(state: State<'_, AppState>) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let new_password = perform_change_password(&state).await?;
    let mut result = do_full_login(&state).await?;
    result["password_rotated"] = json!(true);
    result["new_password"] = json!(new_password);
    Ok(result.to_string())
}

/// Send an invoice (simulated) — always returns OK so the app works immediately.
pub(crate) fn send_invoice_simulated(
    invoice_no: &str,
    symbol: &str,
    total: f64,
) -> Result<String, String> {
    Ok(format!(
        "SIMULATED|{}|{}|{}",
        symbol, invoice_no, total as i64
    ))
}

/// Send a test invoice to the portal (simulated) — to exercise the HDDT flow.
#[tauri::command]
pub(crate) async fn hddt_send_simulated(
    state: State<'_, AppState>,
    invoice_no: String,
    symbol: String,
    total: f64,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    send_invoice_simulated(&invoice_no, &symbol, total)
}

/// Tra cứu hóa đơn trên cổng — bản "Tra cứu hóa đơn" của portal
/// (`/tra-cuu/tra-cuu-hoa-don`), gồm 2 tab lớn × 2 tab nhỏ:
///
///   | Hóa đơn    | Loại               | Endpoint                           |
///   |------------|---------------------|------------------------------------|
///   | ra (bán ra)| HĐĐT thường         | `/api/query/invoices/sold`         |
///   | ra (bán ra)| máy tính tiền       | `/api/sco-query/invoices/sold`     |
///   | vào (mua vào)| HĐĐT thường       | `/api/query/invoices/purchase`     |
///   | vào (mua vào)| máy tính tiền     | `/api/sco-query/invoices/purchase` |
///
/// (bắt live 24/09/2026). `search` dựng đúng format của trang:
/// `tdlap=ge=…;tdlap=le=…;<field>==<value>…`. Trả về `{datas, state, total, time}`
/// (`state` = cursor phân trang).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn hddt_list_invoices(
    state: State<'_, AppState>,
    direction: Option<String>,
    kind: Option<String>,
    from: Option<String>,
    to: Option<String>,
    state_filter: Option<String>,
    nmmst: Option<String>,
    nbmst: Option<String>,
    nmcmnd: Option<String>,
    tthai: Option<String>,
    ttxly: Option<String>,
    khmshdon: Option<String>,
    khhdon: Option<String>,
    shdon: Option<String>,
    unhiem: Option<bool>,
    size: Option<u32>,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let session = state
        .portal
        .lock()
        .await
        .clone()
        .ok_or_else(|| "Chưa đăng nhập cổng HĐĐT — bấm Đăng nhập trước.".to_string())?;
    let cfg = read_config(&state).await?;
    // Tra cứu là thao tác tương tác: cổng treo thì báo lỗi sau 1 lần thử (10 s)
    // thay vì quay nút 4 lần (~100 s) như quét hàng loạt.
    let client = HddtClient::interactive(&cfg.username, &cfg.password, cfg.base())?;
    fn clean(v: Option<String>) -> Option<String> {
        v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
    }
    let q = InvoiceQuery {
        direction: match direction.as_deref() {
            Some("purchase") => InvoiceDirection::Purchase,
            _ => InvoiceDirection::Sold,
        },
        kind: match kind.as_deref() {
            Some("cash-register") => InvoiceKind::CashRegister,
            _ => InvoiceKind::Regular,
        },
        size: size.unwrap_or(15),
        state: clean(state_filter),
        from: clean(from),
        to: clean(to),
        nmmst: clean(nmmst),
        nbmst: clean(nbmst),
        nmcmnd: clean(nmcmnd),
        tthai: clean(tthai),
        ttxly: clean(ttxly),
        khmshdon: clean(khmshdon),
        khhdon: clean(khhdon),
        shdon: clean(shdon),
        unhiem: unhiem.unwrap_or(false),
    };
    let list = client.query_invoices(&session.token, &q).await?;
    let v = serde_json::json!({
        "datas": list.datas,
        "state": list.state,
        "total": list.total,
        "time": list.time,
    });
    audit(&state, "hddt_list_invoices", "hddt", "list").await;
    Ok(v.to_string())
}

// ─── Đồng bộ hóa đơn mua vào ───

/// CHỈ DÙNG CHO E2E: chèn 1 hóa đơn mua đã cache (giả lập kết quả quét cổng)
/// để kiểm thử luồng nhập kho mà không cần gọi cổng HĐĐT. Không có trong
/// invoke_handler của Tauri; chỉ expose qua web server khi build test.
#[tauri::command]
pub(crate) async fn hddt_sync_test_seed(
    state: State<'_, AppState>,
    portal_id: String,
    detail: serde_json::Value,
) -> Result<String, String> {
    let pool = state.pool.read().await;
    let lines = detail
        .get("hdhhdvu")
        .and_then(serde_json::Value::as_array)
        .map(|a| a.len())
        .unwrap_or(0);
    // Biểu thức tạm bind trước: macro giữ tham chiếu tới hết `.await`.
    let line_count = lines as i64;
    // Tổng hóa đơn tính TỪ các dòng chi tiết: import đối chiếu `tgtttbso`
    // với tổng dòng và dàn phần lệch xuống từng dòng, nên giả "tổng 2 triệu"
    // cho hóa đơn 250 nghìn sẽ để đơn giá dòng bị đẩy lên để bù chênh lệch.
    let num = |v: &serde_json::Value| v.as_f64().unwrap_or(0.0);
    let r2 = |n: f64| (n * 100.0).round() / 100.0;
    let mut tgtcthue = 0.0;
    let mut tgtthue = 0.0;
    for line in detail
        .get("hdhhdvu")
        .and_then(serde_json::Value::as_array)
        .map(|a| a.as_slice())
        .unwrap_or(&[])
    {
        let gross = num(line.get("sluong").unwrap_or(&serde_json::json!(0)))
            * num(line.get("dgia").unwrap_or(&serde_json::json!(0)));
        // Tiền CK không vượt quá thành tiền (hóa đơn âm → giữ nguyên CK).
        let discount_raw = num(line.get("stckhau").unwrap_or(&serde_json::json!(0)));
        let discount = if gross >= 0.0 {
            discount_raw.clamp(0.0, gross)
        } else {
            discount_raw
        };
        let net = r2(gross - discount);
        tgtcthue += net;
        tgtthue += r2(net * num(line.get("tsuat").unwrap_or(&serde_json::json!(0))));
    }
    let tgtcthue = r2(tgtcthue);
    let tgtthue = r2(tgtthue);
    let tgtttbso = r2(tgtcthue + tgtthue);
    let detail_json = detail.to_string();
    // Số hóa đơn RIÊNG cho từng lần seed: guard "mỗi hóa đơn chỉ được nhập 1
    // phiếu" (stock.rs) chặn nhập lại hóa đơn đã có trong phiếu nhập. Số phải
    // suy ra từ CẢ cache lẫn phiếu nhập đã có — test trước có thể đã xoá cache
    // nhưng phiếu nhập thì vẫn còn, nên chỉ nhìn cache sẽ bị trùng số.
    let cached: Vec<String> =
        sqlx::query_scalar!("SELECT shdon FROM hddt_purchase_invoice WHERE khhdon = 'C26E2E'")
            .fetch_all(&*pool)
            .await
            .map_err(|e| e.to_string())?;
    let imported: Vec<String> = sqlx::query_scalar!(
        "SELECT invoice_no FROM inbound_voucher WHERE lower(trim(invoice_serial)) = 'c26e2e'"
    )
    .fetch_all(&*pool)
    .await
    .map_err(|e| e.to_string())?;
    let max_no = cached
        .iter()
        .chain(imported.iter())
        .filter_map(|s| s.trim().parse::<i64>().ok())
        .max()
        .unwrap_or(0);
    let shdon = format!("{:03}", max_no + 1);
    sqlx::query!(
        "INSERT INTO hddt_purchase_invoice
            (portal_id, portal_kind, tdlap, posting_date, nbmst, nbten, nmmst,
             khmshdon, khhdon, shdon, hthdon, tchat, tgtcthue, tgtthue, ttcktmai,
             tgtttbso, status, line_count, raw_json, detail_json, created_at)
         VALUES (?, 'regular', '2026-09-01T17:00:00Z', '2026-09-02', '0106773786',
                 'Cty TNHH ABC', '001170019085', 1, 'C26E2E', ?, 1, 1,
                 ?, ?, 0, ?, 'pending', ?, '{}', ?, datetime('now'))
         ON CONFLICT(portal_id) DO NOTHING",
        portal_id,
        shdon,
        tgtcthue,
        tgtthue,
        tgtttbso,
        line_count,
        detail_json
    )
    .execute(&*pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok("ok".into())
}

/// Token phiên cổng còn hạn — dùng chung cho client thường và client tương tác.
async fn portal_token(state: &State<'_, AppState>) -> Result<String, String> {
    let session = state
        .portal
        .lock()
        .await
        .clone()
        .ok_or_else(|| "Chưa đăng nhập cổng HĐĐT — bấm Đăng nhập trước.".to_string())?;
    if session.seconds_left() <= 0 {
        return Err("Phiên cổng HĐĐT đã hết hạn — bấm Đăng nhập lại.".into());
    }
    Ok(session.token)
}

/// Lấy token phiên cổng (đã bắt buộc đăng nhập) + config, dùng cho các lệnh sync.
pub(crate) async fn portal_client(
    state: &State<'_, AppState>,
) -> Result<(HddtClient, String), String> {
    let token = portal_token(state).await?;
    let cfg = read_config(state).await?;
    let client = HddtClient::new(&cfg.username, &cfg.password, cfg.base())?;
    Ok((client, token))
}

/// Client timeout ngắn, không thử lại — cho thao tác người dùng đang chờ
/// (Xem PDF/Nhập kho từng hóa đơn) để báo lỗi nhanh thay vì treo hàng chục giây.
async fn portal_client_interactive(
    state: &State<'_, AppState>,
) -> Result<(HddtClient, String), String> {
    let token = portal_token(state).await?;
    let cfg = read_config(state).await?;
    let client = HddtClient::interactive(&cfg.username, &cfg.password, cfg.base())?;
    Ok((client, token))
}

/// Tự tìm XML còn thiếu cho **báo cáo kỳ thuế** — chạy trước khi giữ lock
/// CSDL (hàm này tự đọc config + danh sách hóa đơn qua `pool.read()`).
///
/// Đúng lúc này chứ không phải lúc gán số HĐĐT: sau khi ký, cổng còn cache nên
/// trang tra cứu đôi khi chưa trả hồ sơ gốc (`hsgoc`) — bấm xuất hồ sơ mới là
/// lúc chắc chắn có gì đó để tải.
///
/// Không có/đã hết phiên → trả `Err` lý do để người gọi ghi **nhật ký** rồi bỏ
/// qua; không tự `audit` ở đây vì người gọi đang giữ `pool.read()` (audit cũng
/// đọc CSDL — lồng read-lock là nguy cơ chờ deadlock khi có writer xếp hàng).
pub(crate) async fn fill_missing_for_report(
    state: &State<'_, AppState>,
    dir: &std::path::Path,
    from: &str,
    to: &str,
) -> Result<crate::hddt::FillOutcome, String> {
    let (client, token) = portal_client(state).await?;
    let pool = state.pool.read().await;
    let out = crate::hddt::fill_missing_in_period(&pool, dir, &client, &token, from, to).await;
    drop(pool);
    Ok(out)
}

/// Thư mục `hddt_xml/` của hồ sơ đang mở (file XML sống cùng DB nên đi theo
/// hồ sơ khi đổi/backup).
pub(crate) fn xml_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let db = crate::commands::profile::active_db_path(app)?;
    let profile = db
        .parent()
        .ok_or_else(|| "Đường dẫn hồ sơ không hợp lệ".to_string())?
        .to_path_buf();
    Ok(crate::hddt::xml_dir_under(&profile))
}

fn parse_kinds(kinds: Option<String>) -> Vec<InvoiceKind> {
    let raw = kinds.unwrap_or_default();
    let list: Vec<InvoiceKind> = raw
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| match s {
            "cash-register" => InvoiceKind::CashRegister,
            _ => InvoiceKind::Regular,
        })
        .collect();
    if list.is_empty() {
        vec![InvoiceKind::Regular, InvoiceKind::CashRegister]
    } else {
        list
    }
}

/// Quét cổng theo khoảng ngày và cache hóa đơn mua kèm chi tiết dòng hàng.
/// Mốc bắt đầu lấy từ `hddt_start_date` (khai trong popup cấu hình HKD);
/// mốc cuối là hôm nay. Ngày đã quét được cache nên lần sau không gọi lại.
///
/// `job_id` chỉ có khi người dùng bấm "Quét cổng": frontend sinh UUID rồi hỏi
/// `get_job_progress` để hiện nhật ký realtime. Timer chạy nền gọi cùng lệnh
/// mà không truyền `job_id` → không đụng tới hộp thoại của việc khác.
#[tauri::command]
pub(crate) async fn hddt_sync_scan(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    from: Option<String>,
    to: Option<String>,
    kinds: Option<String>,
    job_id: Option<String>,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let job = job_id
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(|id| ProgressJob::begin(&id, "Quét cổng HĐĐT"));
    let out = run_hddt_sync_scan(state, app, from, to, kinds, job.as_ref()).await;
    if let Some(j) = &job {
        j.finish(out.as_ref().err().cloned());
    }
    out
}

async fn run_hddt_sync_scan(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    from: Option<String>,
    to: Option<String>,
    kinds: Option<String>,
    job: Option<&ProgressJob>,
) -> Result<String, String> {
    if let Some(j) = job {
        j.step("Đang kiểm tra phiên đăng nhập cổng HĐĐT…");
    }
    // Mỗi lúc chỉ 1 lượt quét/nhập gọi cổng: timer chạy nền trùng thao tác tay sẽ
    // dồn request và bị cổng giới hạn tần suất/khoá tài khoản.
    let _bulk = crate::hddt::BULK_SYNC_LOCK.try_lock().map_err(|_| {
        "Đang có lượt quét/nhập hóa đơn khác chạy — chờ xong rồi bấm lại.".to_string()
    })?;
    let (client, token) = portal_client(&state).await?;
    let pool = state.pool.read().await;
    let kinds = parse_kinds(kinds);
    let summary = match job {
        Some(j) => {
            sync::scan_with_progress(
                &pool,
                &client,
                &token,
                from.as_deref().unwrap_or(""),
                to.as_deref().unwrap_or(""),
                &kinds,
                Some(j),
            )
            .await?
        }
        // Chạy nền: không đụng ô tiến độ chung của hộp thoại đang mở.
        None => {
            sync::scan(
                &pool,
                &client,
                &token,
                from.as_deref().unwrap_or(""),
                to.as_deref().unwrap_or(""),
                &kinds,
            )
            .await?
        }
    };
    // Tự động lưu file XML cho hóa đơn mới (quy định lưu trữ HĐĐT). Lỗi từng
    // hóa đơn không làm hỏng lượt quét — cộng vào `xml` để UI báo.
    let xml = match job {
        Some(j) => {
            crate::hddt::fill_missing_after_scan_with_progress(
                &pool,
                &xml_dir(&app)?,
                &client,
                &token,
                Some(j),
            )
            .await
        }
        None => crate::hddt::fill_missing_after_scan(&pool, &xml_dir(&app)?, &client, &token).await,
    };
    drop(pool);
    audit(&state, "hddt_sync_scan", "hddt", "sync scan").await;
    let mut v = serde_json::to_value(&summary).unwrap_or_default();
    v["xml"] = serde_json::to_value(&xml).unwrap_or_default();
    Ok(v.to_string())
}

/// Bỏ dấu "ngày đã quét" + xoá cache hóa đơn CỦA NGÀY ĐÓ để lần quét tới tải
/// lại từ cổng. Dùng khi hóa đơn xuất hiện SAU khi ngày đó đã quét (ký trễ,
/// cổng cập nhật chậm) — cache theo ngày trước đây bỏ sót vĩnh viễn những hóa
/// đơn như thế. Hóa đơn đã nhập kho nằm ở bảng riêng nên không bị đụng tới.
#[tauri::command]
pub(crate) async fn hddt_uncache_day(
    state: State<'_, AppState>,
    day: String,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let day = day.trim().to_string();
    if chrono::NaiveDate::parse_from_str(&day, "%Y-%m-%d").is_err() {
        return Err(format!("Ngày '{day}' không hợp lệ (cần yyyy-mm-dd)"));
    }
    let (invoices, days) = sync::uncache_day(&*state.pool.read().await, &day).await?;
    audit(&state, "hddt_uncache_day", "hddt", &day).await;
    Ok(
        serde_json::json!({ "ok": true, "invoices_deleted": invoices, "days_deleted": days })
            .to_string(),
    )
}

/// Xem trước kết quả trong cache (không gọi cổng) + cho phép thử lại các
/// hóa đơn lần trước chưa lấy được chi tiết.
#[tauri::command]
pub(crate) async fn hddt_sync_preview(
    state: State<'_, AppState>,
    from: Option<String>,
    to: Option<String>,
    retry_failed: Option<bool>,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let pool = state.pool.read().await;
    if retry_failed.unwrap_or(false) {
        sync::retry_failed_details(&pool).await?;
    }
    let (rows, summary) = sync::preview(
        &pool,
        from.as_deref().unwrap_or(""),
        to.as_deref().unwrap_or(""),
    )
    .await?;
    Ok(serde_json::json!({ "rows": rows, "summary": summary }).to_string())
}

/// Nhập kho các hóa đơn đã cache: tạo mặt hàng/NCC còn thiếu, tạo phiếu nhập
/// (1 hóa đơn = 1 phiếu) và liên kết hóa đơn chính thức với phiếu.
/// `ids` rỗng = nhập tất cả hóa đơn đang chờ.
#[tauri::command]
pub(crate) async fn hddt_sync_import(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    ids: Option<Vec<i64>>,
    warehouse_code: Option<String>,
    unit_code: Option<String>,
    debit_account: Option<String>,
    credit_account: Option<String>,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    // Mỗi lúc chỉ 1 lượt quét/nhập gọi cổng: timer chạy nền trùng thao tác tay sẽ
    // dồn request và bị cổng giới hạn tần suất/khoá tài khoản.
    let _bulk = crate::hddt::BULK_SYNC_LOCK.try_lock().map_err(|_| {
        "Đang có lượt quét/nhập hóa đơn khác chạy — chờ xong rồi bấm lại.".to_string()
    })?;
    let import_ids: Vec<i64> = ids.clone().unwrap_or_default();
    // Lấy client cổng TRƯỚC khi giữ pool: `portal_client` đọc config bằng
    // `pool.read()`, lồng read-lock trong lúc có writer xếp hàng là nguy cơ deadlock.
    let portal = portal_client(&state).await.ok();
    let pool = state.pool.read().await;
    let mut targets: Vec<CachedInvoice> = if let Some(list) = ids.filter(|l| !l.is_empty()) {
        let mut out = Vec::new();
        for id in list {
            let row: Option<CachedInvoice> = sqlx::query_as!(
                CachedInvoice,
                r#"SELECT id as "id!", portal_id, portal_kind, posting_date, nbmst, nbten,
                          khmshdon as "khmshdon!", khhdon, shdon, status, detail_json,
                  tgtcthue, tgtthue, tgtttbso
                     FROM hddt_purchase_invoice WHERE id = ?"#,
                id
            )
            .fetch_optional(&*pool)
            .await
            .map_err(|e| e.to_string())?;
            if let Some(r) = row {
                out.push(r);
            }
        }
        out
    } else {
        sqlx::query_as!(
            CachedInvoice,
            r#"SELECT id as "id!", portal_id, portal_kind, posting_date, nbmst, nbten,
                      khmshdon as "khmshdon!", khhdon, shdon, status, detail_json,
                  tgtcthue, tgtthue, tgtttbso
                 FROM hddt_purchase_invoice
                WHERE status = 'pending' AND detail_json <> '' AND detail_error = ''
                ORDER BY posting_date, khhdon, shdon"#
        )
        .fetch_all(&*pool)
        .await
        .map_err(|e| e.to_string())?
    };

    // Hóa đơn chưa có dòng hàng (lượt quét trước đứt mạng giữa chừng) → tải chi
    // tiết ngay tại đây để bấm "Nhập kho" là chạy được. Tải TUẦN TỰ từng hóa đơn
    // (không chạy song song): cổng giới hạn tần suất, dồn request là bị khoá.
    let mut detail_errors: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    if let Some((client, token)) = &portal {
        for inv in targets.iter_mut() {
            if !inv.detail_json.trim().is_empty() {
                continue;
            }
            match inv.fetch_detail(&pool, client, token).await {
                Ok(detail_json) => {
                    inv.detail_json = detail_json;
                }
                Err(msg) => {
                    let skip = format!("Không lấy được chi tiết: {msg}");
                    sqlx::query!(
                        "UPDATE hddt_purchase_invoice
                            SET status = 'manual', detail_error = ?, skip_reason = ?
                          WHERE id = ?",
                        msg,
                        skip,
                        inv.id
                    )
                    .execute(&*pool)
                    .await
                    .map_err(|e| e.to_string())?;
                    detail_errors.insert(
                        inv.portal_id.clone(),
                        format!(
                            "Hóa đơn {} {} chưa tải được dòng hàng: {msg}",
                            inv.khhdon, inv.shdon
                        ),
                    );
                }
            }
        }
    }

    let mut results: Vec<serde_json::Value> = Vec::new();
    for inv in &targets {
        if let Some(msg) = detail_errors.get(&inv.portal_id) {
            results.push(serde_json::json!({
                "portal_id": inv.portal_id, "ok": false, "voucher_no": "",
                "products_created": 0, "message": msg,
            }));
            continue;
        }
        if inv.status != "pending" {
            results.push(serde_json::json!({
                "portal_id": inv.portal_id, "ok": false, "voucher_no": "",
                "products_created": 0,
                "message": format!("Bỏ qua — trạng thái '{}'", inv.status),
            }));
            continue;
        }
        match sync::import_invoice(
            &pool,
            inv,
            warehouse_code.as_deref().unwrap_or(""),
            unit_code.as_deref().unwrap_or("HKD"),
            debit_account.as_deref().unwrap_or(""),
            credit_account.as_deref().unwrap_or(""),
        )
        .await
        {
            Ok(o) => results.push(serde_json::to_value(o).unwrap_or_default()),
            Err(e) => results.push(serde_json::json!({
                "portal_id": inv.portal_id, "ok": false, "voucher_no": "",
                "products_created": 0, "message": e,
            })),
        }
    }
    let ok_count = results.iter().filter(|r| r["ok"] == true).count();
    let mut v = serde_json::json!({
        "imported": ok_count,
        "failed": results.len() - ok_count,
        "results": results,
    });
    drop(pool);

    // Tự động lưu file XML **sau khi phiếu đã tạo** — tạo phiếu là việc chính,
    // còn XML là việc lưu trữ nên không được chặn hay làm hỏng kết quả nhập.
    // Chạy được cả với hóa đơn quét từ trước khi app có tính năng này (backfill);
    // thiếu phiên cổng thì bỏ qua — nhập kho vẫn chạy offline được từ cache.
    if let Ok((client, token)) = portal_client(&state).await {
        let post = state.pool.read().await;
        let out = crate::hddt::fill_missing_before_import(
            &post,
            &xml_dir(&app)?,
            &client,
            &token,
            &import_ids,
        )
        .await;
        drop(post);
        v["xml"] = serde_json::to_value(&out).unwrap_or_default();
    }
    audit(
        &state,
        "hddt_sync_import",
        "hddt",
        &format!("imported {ok_count}"),
    )
    .await;
    Ok(v.to_string())
}

/// Xoá cache đồng bộ (hóa đơn chưa nhập kho + dấu ngày đã quét) để quét lại từ
/// đầu. Hóa đơn đã nhập kho giữ nguyên — liên kết hóa đơn ↔ phiếu không được mất.
#[tauri::command]
pub(crate) async fn hddt_sync_clear_cache(state: State<'_, AppState>) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let pool = state.pool.read().await;
    let out = sync::clear_cache(&pool).await?;
    drop(pool);
    audit(
        &state,
        "hddt_sync_clear_cache",
        "hddt",
        &format!(
            "xóa {} hóa đơn chưa nhập, {} ngày cache",
            out.invoices_deleted, out.days_deleted
        ),
    )
    .await;
    Ok(serde_json::to_string(&out).unwrap_or_default())
}

// ─── Xem PDF hóa đơn điện tử ───
//
// Nguồn chi tiết hóa đơn (`detail_json`) theo từng màn hình:
//   • Đồng bộ HĐĐT → cache `hddt_purchase_invoice` hoặc `hddt_imported_invoice`;
//   • Phiếu nhập    → `hddt_imported_invoice` qua `inbound_voucher_id`;
//   • Tra cứu HĐĐT  → chưa chắc đã lưu nên gọi thẳng cổng.
// Chuỗi trả về được frontend dựng thành HTML rồi in ra PDF (`render_invoice_pdf`).

/// Lấy `detail_json` của hóa đơn trong màn Đồng bộ HĐĐT (chưa nhập kho nằm ở
/// cache, đã nhập kho nằm ở bảng hóa đơn chính thức — cache bị xoá khi import).
///
/// Chưa có trong DB thì **tự tải riêng hóa đơn này** từ cổng rồi lưu cache —
/// không bắt người dùng quét lại cả ngày chỉ để xem 1 PDF.
#[tauri::command]
pub(crate) async fn hddt_sync_invoice_detail(
    state: State<'_, AppState>,
    portal_id: String,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let inv = {
        let pool = state.pool.read().await;
        if let Some(detail) = sqlx::query_scalar!(
            "SELECT detail_json FROM hddt_purchase_invoice
              WHERE portal_id = ? AND detail_json <> ''",
            portal_id
        )
        .fetch_optional(&*pool)
        .await
        .map_err(|e| e.to_string())?
        {
            return Ok(detail);
        }
        if let Some(detail) = sqlx::query_scalar!(
            "SELECT detail_json FROM hddt_imported_invoice
              WHERE portal_id = ? AND detail_json <> ''",
            portal_id
        )
        .fetch_optional(&*pool)
        .await
        .map_err(|e| e.to_string())?
        {
            return Ok(detail);
        }
        sync::cached_by_portal_id(&pool, &portal_id).await?
    };
    let Some(mut inv) = inv else {
        return Err(
            "Hóa đơn này không còn trong cache và chưa có chi tiết — hãy quét lại ngày của hóa đơn."
                .into(),
        );
    };
    // Không giữ pool trong lúc đọc config/gọi cổng: writer đang chờ sẽ bị chặn.
    let (client, token) = portal_client_interactive(&state).await?;
    let pool = state.pool.read().await;
    inv.fetch_detail(&pool, &client, &token).await
}

/// Lấy `detail_json` để xem PDF hóa đơn gắn với 1 phiếu kho.
///
/// * Phiếu nhập: hóa đơn mua đã import nằm ở `hddt_imported_invoice`, nối qua
///   `inbound_voucher_id`.
/// * Phiếu xuất: hóa đơn bán nối 1-1 qua `invoice.voucher_no`; chỉ nhận hóa đơn
///   đã có số HĐĐT (bản nháp chưa phải hóa đơn điện tử). PDF dựng từ dữ liệu
///   hóa đơn nội bộ — cùng nguồn với nút "Xem PDF" ở tab Hóa đơn.
async fn voucher_invoice_detail_core(
    pool: &SqlitePool,
    voucher_no: &str,
) -> Result<String, String> {
    if let Some(detail) = sqlx::query_scalar!(
        "SELECT hi.detail_json
           FROM inbound_voucher iv
           JOIN hddt_imported_invoice hi ON hi.inbound_voucher_id = iv.id
          WHERE iv.voucher_no = ? AND hi.detail_json <> ''
          LIMIT 1",
        voucher_no
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    {
        return Ok(detail);
    }

    let invoice_id: Option<i64> = sqlx::query_scalar!(
        "SELECT id FROM invoice
          WHERE voucher_no = ? AND e_invoice_no <> ''
          ORDER BY id DESC LIMIT 1",
        voucher_no
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;
    match invoice_id {
        Some(id) => crate::commands::invoice::invoice_preview_json(pool, id).await,
        None => Err(format!("Phiếu {voucher_no} chưa gắn hóa đơn điện tử nào.")),
    }
}

/// Lấy `detail_json` của hóa đơn đã gắn với 1 phiếu nhập/xuất kho (dùng ở popup phiếu).
#[tauri::command]
pub(crate) async fn hddt_voucher_invoice_detail(
    state: State<'_, AppState>,
    voucher_no: String,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let pool = state.pool.read().await;
    voucher_invoice_detail_core(&pool, voucher_no.trim()).await
}

/// Lấy chi tiết hóa đơn từ cổng HĐĐT cho màn Tra cứu HĐĐT (dòng list chưa có
/// chi tiết trong DB nên phải gọi thật).
#[tauri::command]
pub(crate) async fn hddt_invoice_detail(
    state: State<'_, AppState>,
    nbmst: String,
    khmshdon: i64,
    khhdon: String,
    shdon: String,
    kind: Option<String>,
    direction: Option<String>,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    if khmshdon <= 0 {
        return Err("Thiếu mã mẫu số (khmshdon) — không lấy được chi tiết hóa đơn.".to_string());
    }
    // Tab tra cứu biết rõ loại HĐ; thiếu thì suy từ ký hiệu (CG = máy tính tiền)
    // để chọn đúng tiền tố `query` / `sco-query` của endpoint chi tiết.
    let kind = match kind.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => crate::hddt::InvoiceKind::parse(s)?,
        None => crate::hddt::InvoiceKind::from_symbol(&khhdon),
    };
    // Header `Action` của cổng phụ thuộc cả chiều (mua vào/bán ra) lẫn loại HĐ.
    // Màn Đồng bộ chỉ có hóa đơn mua vào → mặc định `purchase` khi thiếu tham số.
    let direction = match direction
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(s) => crate::hddt::InvoiceDirection::parse(s)?,
        None => crate::hddt::InvoiceDirection::Purchase,
    };
    let session = state
        .portal
        .lock()
        .await
        .clone()
        .ok_or_else(|| "Chưa đăng nhập cổng HĐĐT — bấm Đăng nhập trước.".to_string())?;
    let cfg = read_config(&state).await?;
    // Xem PDF cũng là thao tác tương tác — timeout 10 s, không retry 4 lần.
    let client = HddtClient::interactive(&cfg.username, &cfg.password, cfg.base())?;
    let detail = client
        .invoice_detail(
            &session.token,
            direction,
            kind,
            &nbmst,
            &khmshdon.to_string(),
            &khhdon,
            &shdon,
        )
        .await?;
    audit(
        &state,
        "hddt_invoice_detail",
        "hddt",
        &format!("{khhdon} {shdon}"),
    )
    .await;
    Ok(detail.to_string())
}

// ─── Lưu file XML hóa đơn điện tử (quy định lưu trữ HĐĐT) ───

/// Tải XML 1 hóa đơn từ cổng rồi ghi vào `…/profiles/<key>/hddt_xml/`.
///
/// Dùng cho nút "Tải XML" từng dòng ở màn Tra cứu HĐĐT (bán ra lẫn mua vào)
/// và màn Đồng bộ. Nếu file đã có từ trước thì trả `already: true` (không tải
/// lại) — người dùng bấm hai lần vẫn chỉ có 1 file.
///
/// Danh sách tham số phẳng theo đúng tên field của lệnh (web mode nhận JSON
/// từng khoá) — gom vào struct sẽ phá hợp đồng API với frontend.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub(crate) async fn hddt_save_xml(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    direction: Option<String>,
    kind: Option<String>,
    nbmst: String,
    khmshdon: Option<i64>,
    khhdon: String,
    shdon: String,
    portal_id: Option<String>,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let (client, token) = portal_client(&state).await?;
    // Không truyền `kind` (nút "Tải XML" ở chi tiết hóa đơn không biết hộ dùng
    // HĐ điện tử hay máy tính tiền) → suy từ ký hiệu: máy tính tiền bắt buộc
    // chứa chữ `CG`. Truyền sai thì cổng trả 404 chứ không tự đoán lại.
    let kind = match kind.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => crate::hddt::InvoiceKind::parse(s)?,
        None => crate::hddt::InvoiceKind::from_symbol(&khhdon),
    };
    let key = crate::hddt::XmlInvoiceKey {
        direction: crate::hddt::InvoiceDirection::parse(direction.as_deref().unwrap_or(""))?,
        kind,
        nbmst: nbmst.trim().to_string(),
        khmshdon: khmshdon.unwrap_or(0),
        khhdon: khhdon.trim().to_string(),
        shdon: shdon.trim().to_string(),
    };
    if key.nbmst.is_empty() || key.khhdon.is_empty() {
        return Err("Thiếu MST bán ra hoặc ký hiệu — không gọi được cổng xuất XML.".to_string());
    }
    let pool = state.pool.read().await;
    let saved = crate::hddt::save_manual(
        &pool,
        &xml_dir(&app)?,
        &client,
        &token,
        &key,
        portal_id.as_deref().unwrap_or(""),
    )
    .await?;
    drop(pool);
    audit(
        &state,
        "hddt_save_xml",
        "hddt",
        &format!("{} {} {}", key.khhdon, key.shdon, saved.file_name),
    )
    .await;
    Ok(serde_json::to_string(&saved).unwrap_or_default())
}

/// Danh sách file XML đã lưu — màn Tra cứu/Đồng bộ tra "dòng nào đã có file".
#[tauri::command]
pub(crate) async fn hddt_list_xml(state: State<'_, AppState>) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let pool = state.pool.read().await;
    let rows = crate::hddt::list_saved(&pool).await?;
    Ok(serde_json::to_string(&rows).unwrap_or_default())
}

/// Gộp nhiều file XML đã lưu thành **1 file ZIP** để nộp/gửi — trả base64
/// (frontend giải mã rồi tải về, cùng cách với PDF hóa đơn).
///
/// `ids` = danh sách `hddt_invoice_xml.id` mà frontend lấy từ `hddt_list_xml`
/// (tức đúng các dòng người dùng đang thấy); bỏ trống/rỗng = gộp **tất cả**.
#[tauri::command]
pub(crate) async fn hddt_export_xml_zip(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    ids: Option<Vec<i64>>,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let ids = ids.unwrap_or_default();
    let pool = state.pool.read().await;
    let zip = crate::hddt::export_zip(&pool, &xml_dir(&app)?, &ids).await?;
    drop(pool);
    audit(
        &state,
        "hddt_export_xml_zip",
        "hddt",
        &if ids.is_empty() {
            "tất cả hóa đơn đã lưu XML".to_string()
        } else {
            format!("{} hóa đơn", ids.len())
        },
    )
    .await;
    Ok(base64::engine::general_purpose::STANDARD.encode(&zip))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;

    // ─── is_wrong_password_message (thuần) ───

    #[test]
    fn captcha_error_khong_coi_la_sai_mat_khau() {
        assert!(!is_wrong_password_message("HĐĐT: Mã captcha không đúng."));
        assert!(!is_wrong_password_message("Không lấy được captcha HĐĐT."));
    }

    #[test]
    fn sai_mat_khau_va_khoa_tai_khoan_duoc_nhan_biet() {
        assert!(is_wrong_password_message(
            "HĐĐT: Thông tin tài khoản hoặc mật khẩu không chính xác."
        ));
        assert!(is_wrong_password_message(
            "HĐĐT: Tài khoản đã tạm khóa do nhập sai thông tin."
        ));
    }

    #[test]
    fn loi_khac_khong_ghi_nhan() {
        assert!(!is_wrong_password_message("Tài khoản không tồn tại."));
        assert!(!is_wrong_password_message(
            "Lỗi kết nối cổng HĐĐT: connection refused"
        ));
    }

    // ─── Nhật ký mật khẩu (pool thật, migration đã chạy) ───

    #[tokio::test]
    async fn note_then_bad_then_reset_on_ok() {
        let pool = test_pool().await;
        journal_note(&pool, "1234567890-001", "pw1", "lưu tay")
            .await
            .expect("note");
        // idempotent — ghi lại không đổi ghi chú/state.
        journal_note(&pool, "1234567890-001", "pw1", "lưu tay lại")
            .await
            .expect("note lại");
        assert!(!journal_is_bad(&pool, "1234567890-001", "pw1")
            .await
            .expect("is_bad"));

        journal_mark(&pool, "1234567890-001", "pw1", JOURNAL_BAD)
            .await
            .expect("mark bad");
        assert!(journal_is_bad(&pool, "1234567890-001", "pw1")
            .await
            .expect("is bad"));

        // Mật khẩu khác của cùng tài khoản đăng nhập thành công (mark ok) →
        // mọi dấu bad của TÀI KHOẠN đó được reset (quy ước "thành công thì reset").
        journal_mark(&pool, "1234567890-001", "pw2", JOURNAL_BAD)
            .await
            .expect("mark bad pw2");
        journal_record_ok(&pool, "1234567890-001", "pw2")
            .await
            .expect("record ok");
        assert!(!journal_is_bad(&pool, "1234567890-001", "pw1")
            .await
            .expect("reset pw1"));
        assert!(!journal_is_bad(&pool, "1234567890-001", "pw2")
            .await
            .expect("ok pw2"));

        // ... nhưng dấu bad của TÀI KHOẠN KHÁC không bị lây (reset theo username).
        journal_mark(&pool, "0987654321-001", "pw1", JOURNAL_BAD)
            .await
            .expect("mark bad TK khác");
        assert!(journal_is_bad(&pool, "0987654321-001", "pw1")
            .await
            .expect("bad của TK khác"));
    }

    #[tokio::test]
    async fn journal_is_bad_bo_qua_o_trong() {
        let pool = test_pool().await;
        assert!(!journal_is_bad(&pool, "", "pw")
            .await
            .expect("username rỗng"));
        assert!(!journal_is_bad(&pool, "TK", "")
            .await
            .expect("password rỗng"));
    }

    // ─── PDF hóa đơn gắn với phiếu kho ───

    /// Phiếu xuất nối 1-1 với hóa đơn bán qua `invoice.voucher_no`; hóa đơn đã
    /// có số HĐĐT thì popup phiếu phải xem được PDF.
    #[tokio::test]
    async fn phieu_xuat_co_hoa_don_dien_tu_tra_ve_pdf() {
        let pool = test_pool().await;
        sqlx::query(
            "INSERT INTO invoice
                 (number, date, customer, status, e_invoice_no, e_invoice_date,
                  voucher_no, total)
             VALUES ('HD-PX', '2026-10-01', 'Khách xuất', 'official', '10',
                     '2026-10-01', 'PXTEST', 1000)",
        )
        .execute(&pool)
        .await
        .expect("seed hóa đơn bán");

        let raw = voucher_invoice_detail_core(&pool, "PXTEST")
            .await
            .expect("phải trả detail_json");
        let data: serde_json::Value = serde_json::from_str(&raw).expect("JSON hợp lệ");
        assert_eq!(data["nmten"], "Khách xuất");
        assert_eq!(data["shdon"], "10", "số HĐĐT, không phải số nội bộ");
        assert_eq!(data["tgtttbso"], 1000.0);
    }

    /// Hóa đơn nháp (chưa có số HĐĐT) hoặc phiếu không nối hóa đơn → không có
    /// nút xem PDF, backend trả lỗi rõ ràng.
    #[tokio::test]
    async fn phieu_xuat_chua_co_hoa_don_dien_tu_bao_loi() {
        let pool = test_pool().await;
        sqlx::query(
            "INSERT INTO invoice (number, date, status, voucher_no)
             VALUES ('HD-NHAP', '2026-10-01', 'draft', 'PXDRAFT')",
        )
        .execute(&pool)
        .await
        .expect("seed hóa đơn nháp");

        for voucher_no in ["PXDRAFT", "PX-NONE"] {
            let err = voucher_invoice_detail_core(&pool, voucher_no)
                .await
                .expect_err("không có HĐĐT phải lỗi");
            assert!(err.contains("chưa gắn hóa đơn điện tử"), "{err}");
        }
    }

    /// Phiếu nhập vẫn lấy đúng `detail_json` của hóa đơn mua đã import (đường cũ).
    #[tokio::test]
    async fn phieu_nhap_van_tra_detail_tu_hoa_don_mua() {
        let pool = test_pool().await;
        let voucher_id: i64 = sqlx::query_scalar(
            "INSERT INTO inbound_voucher (voucher_no, posting_date)
             VALUES ('PNTEST', '2026-10-01') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("seed phiếu nhập");
        sqlx::query(
            "INSERT INTO hddt_imported_invoice
                 (portal_id, inbound_voucher_id, detail_json)
             VALUES ('uuid-test', ?, '{\"shdon\":\"10\"}')",
        )
        .bind(voucher_id)
        .execute(&pool)
        .await
        .expect("seed hóa đơn mua");

        let detail = voucher_invoice_detail_core(&pool, "PNTEST")
            .await
            .expect("phải trả detail_json");
        assert_eq!(detail, r#"{"shdon":"10"}"#);
    }
}
