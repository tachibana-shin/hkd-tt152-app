// ─── HDDT portal (electronic invoice) — frontend commands ───
//
// Two layers:
//   1. **Live**: `hddt_captcha` + `hddt_login` call hoadondientu.gdt.gov.vn via
//      `crate::hddt::HddtClient` (protocol reverse-engineered + verified 09/2026).
//      `hddt_captcha_autosolve` logs in automatically: solves the captcha offline
//      with `GlyphTemplateSolver` (embedded glyph templates, no external binary)
//      and reloads the captcha on failure — verified 8/8 on the live portal.
//   2. **Simulated**: `hddt_status` + `hddt_send_simulated` (kept as a fallback
//      when the portal is not configured / not desired).
//
// TODO(hddt): store `hddt_username`/`hddt_password`/`hddt_base_url` in the
// `business` table + a settings screen, instead of passing them as command args.

use crate::hddt::{GlyphTemplateSolver, HddtClient};
use crate::helpers::require_role;
use crate::models::AppState;
use serde_json::json;
use tauri::State;

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

/// Check HDDT portal connection status (simulated).
#[tauri::command]
pub(crate) async fn hddt_status() -> Result<String, String> {
    Ok(json!({
        "mode": "simulated",
        "connected": false,
        "note": "Chưa cấu hình cổng HĐĐT — chế độ mô phỏng cục bộ (ghi nhận số/ký hiệu/ngày cục bộ).",
        "live": {
            "available": true,
            "note": "Đã có client thật (HddtClient): captcha + đăng nhập qua /api/captcha, /api/security-taxpayer/authenticate (verify 09/2026)."
        }
    })
    .to_string())
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

/// Fetch the HDDT captcha → `{key, svg}` so the frontend can show it for manual
/// entry. Standard flow: `hddt_captcha` → user types the code → `hddt_login`.
#[tauri::command]
pub(crate) async fn hddt_captcha(
    state: State<'_, AppState>,
    username: String,
    password: String,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let client = HddtClient::new(&username, &password, None)?;
    let cap = client.fetch_captcha().await?;
    Ok(json!({ "key": cap.key, "svg": cap.content }).to_string())
}

/// Log in to the HDDT portal with an already-solved captcha (manually entered on
/// the UI). Returns `{token, user}` on success; a 401 with the portal's message
/// (e.g. "Mã captcha không đúng.") when the captcha is wrong.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn hddt_login(
    state: State<'_, AppState>,
    username: String,
    password: String,
    captcha_key: String,
    captcha_value: String,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let client = HddtClient::new(&username, &password, None)?;
    let token = client.authenticate(&captcha_key, &captcha_value).await?;
    let user = client.profile(&token).await.ok();
    Ok(json!({ "token": token, "user": user }).to_string())
}

/// Log in to the HDDT portal AUTOMATICALLY (no manual captcha entry).
///
/// Uses `GlyphTemplateSolver`: solves the captcha offline via glyph template
/// matching (reference data embedded in the binary); on failure it fetches a new
/// captcha and retries. Verified live: 8/8 successful logins.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn hddt_captcha_autosolve(
    state: State<'_, AppState>,
    username: String,
    password: String,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let client = HddtClient::new(&username, &password, None)?;
    let solver = GlyphTemplateSolver;
    let attempt = client
        .login_with_solver(&solver, 3)
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!({
        "ok": true,
        "token": attempt.token,
        "user": attempt.user,
        "note": "Đăng nhập tự động bằng GlyphTemplateSolver (offline template-matching)."
    })
    .to_string())
}
