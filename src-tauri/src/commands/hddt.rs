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

use crate::hddt::sync::CachedInvoice;
use crate::hddt::{
    days_left_until, generate_password, rfc3339_to_unix, sync, GlyphTemplateSolver, HddtClient,
    InvoiceDirection, InvoiceKind, InvoiceQuery, LoginError, PortalSession,
};
use crate::helpers::{audit, require_role};
use crate::models::AppState;
use serde_json::{json, Value};
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
    Ok(json!({
        "configured": cfg.configured(),
        "username": username,
        "logged_in": logged_in,
        "expires_at": sess.as_ref().map(|s| s.expires_at).unwrap_or(0),
        "seconds_left": sess.as_ref().map(|s| s.seconds_left()).unwrap_or(0),
        "relogin_margin": RELOGIN_MARGIN_SECS,
        "user": sess.as_ref().and_then(|s| s.user.clone()),
        "auto_change_password": cfg.auto_change_password,
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
    Ok(cfg.password)
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
    let client = HddtClient::new(&cfg.username, &cfg.password, cfg.base())?;
    match client.login_with_solver(&GlyphTemplateSolver, 3).await {
        Ok(login) => {
            let session = PortalSession::new(cfg.username.clone(), login.token, login.user);
            *state.portal.lock().await = Some(session);
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
        Err(LoginError::Other(e)) => Err(e),
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
    let token = client.authenticate(&captcha_key, value).await?;
    let user = client.profile(&token).await.ok();
    let session = PortalSession::new(cfg.username.clone(), token, user);
    *state.portal.lock().await = Some(session);
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
    let client = HddtClient::new(&cfg.username, &cfg.password, cfg.base())?;
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
    let detail_json = detail.to_string();
    sqlx::query!(
        "INSERT INTO hddt_purchase_invoice
            (portal_id, portal_kind, tdlap, posting_date, nbmst, nbten, nmmst,
             khmshdon, khhdon, shdon, hthdon, tchat, tgtcthue, tgtthue, ttcktmai,
             tgtttbso, status, line_count, raw_json, detail_json, created_at)
         VALUES (?, 'regular', '2026-09-01T17:00:00Z', '2026-09-02', '0106773786',
                 'Cty TNHH ABC', '001170019085', 1, 'C26E2E', '001', 1, 1,
                 2000000, 160000, 0, 2160000, 'pending', ?, '{}', ?, datetime('now'))
         ON CONFLICT(portal_id) DO NOTHING",
        portal_id,
        line_count,
        detail_json
    )
    .execute(&*pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok("ok".into())
}

/// Lấy token phiên cổng (đã bắt buộc đăng nhập) + config, dùng cho các lệnh sync.
async fn portal_client(state: &State<'_, AppState>) -> Result<(HddtClient, String), String> {
    let session = state
        .portal
        .lock()
        .await
        .clone()
        .ok_or_else(|| "Chưa đăng nhập cổng HĐĐT — bấm Đăng nhập trước.".to_string())?;
    if session.seconds_left() <= 0 {
        return Err("Phiên cổng HĐĐT đã hết hạn — bấm Đăng nhập lại.".into());
    }
    let cfg = read_config(state).await?;
    let client = HddtClient::new(&cfg.username, &cfg.password, cfg.base())?;
    Ok((client, session.token))
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
/// mốc cuối là hôm qua. Ngày đã quét được cache nên lần sau không gọi lại.
#[tauri::command]
pub(crate) async fn hddt_sync_scan(
    state: State<'_, AppState>,
    from: Option<String>,
    to: Option<String>,
    kinds: Option<String>,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let (client, token) = portal_client(&state).await?;
    let pool = state.pool.read().await;
    let summary = sync::scan(
        &pool,
        &client,
        &token,
        from.as_deref().unwrap_or(""),
        to.as_deref().unwrap_or(""),
        &parse_kinds(kinds),
    )
    .await?;
    drop(pool);
    audit(&state, "hddt_sync_scan", "hddt", "sync scan").await;
    Ok(serde_json::to_string(&summary).unwrap_or_default())
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
    ids: Option<Vec<i64>>,
    warehouse_code: Option<String>,
    unit_code: Option<String>,
    debit_account: Option<String>,
    credit_account: Option<String>,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let pool = state.pool.read().await;
    let targets: Vec<CachedInvoice> = if let Some(list) = ids.filter(|l| !l.is_empty()) {
        let mut out = Vec::new();
        for id in list {
            let row: Option<CachedInvoice> = sqlx::query_as!(
                CachedInvoice,
                r#"SELECT id as "id!", portal_id, posting_date, nbmst, nbten,
                          khmshdon as "khmshdon!", khhdon, shdon, status, detail_json
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
            r#"SELECT id as "id!", portal_id, posting_date, nbmst, nbten,
                      khmshdon as "khmshdon!", khhdon, shdon, status, detail_json
                 FROM hddt_purchase_invoice
                WHERE status = 'pending' AND detail_json <> '' AND detail_error = ''
                ORDER BY posting_date, khhdon, shdon"#
        )
        .fetch_all(&*pool)
        .await
        .map_err(|e| e.to_string())?
    };

    let mut results: Vec<serde_json::Value> = Vec::new();
    for inv in &targets {
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
    let v = serde_json::json!({
        "imported": ok_count,
        "failed": results.len() - ok_count,
        "results": results,
    });
    drop(pool);
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
