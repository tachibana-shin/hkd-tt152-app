#![allow(unused_imports)]

// ─── HDDT portal client (hoadondientu.gdt.gov.vn) ───
//
// Protocol (reverse-engineered from the portal's Next.js bundle + verified with
// real Chrome + real API calls, 09/2026). Two-step login:
//
//   1. GET  {base}/api/captcha
//        → {"key":"<id>","content":"<svg 200x40>"}
//        (the frontend calls "api//captcha" with a double slash → blocked by the
//        F5 WAF; use "api/captcha")
//   2. POST {base}/api/security-taxpayer/authenticate
//        body  : {"username","password","ckey","cvalue"}
//        headers (app-level anti-bot — copied from the portal's axios interceptor):
//          request-id : UUID v4
//          Action     : "" (read from localStorage "action")
//          End-Point  : "/" (current pathname)
//          Accept-Language: "vi"
//        → 200 {"token":"<jwt>"} | 401 {"message":"Mã captcha không đúng."} ...
//        → then GET {base}/api/security-taxpayer/profile with `Authorization: Bearer <token>`
//
// The portal exposes many more useful endpoints (catalogs, invoice lookups, …) —
// all of them require the same 3 anti-bot headers; add them to `HddtClient` as
// needed.

use crate::hddt::captcha::Captcha;
use reqwest::cookie::Jar;
use reqwest::header::{HeaderValue, ACCEPT_LANGUAGE, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

const BASE_DEFAULT: &str = "https://hoadondientu.gdt.gov.vn";
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";

pub(crate) const PATH_CAPTCHA: &str = "/api/captcha";
pub(crate) const PATH_AUTHENTICATE: &str = "/api/security-taxpayer/authenticate";
pub(crate) const PATH_PROFILE: &str = "/api/security-taxpayer/profile";
pub(crate) const PATH_CHANGE_PASSWORD: &str = "/api/system-taxpayer/users/change-password";

// Invoice list — verified LIVE (09/2026) from the portal's own "Tra cứu hóa đơn"
// page for Hộ kinh doanh (`/quan-ly-hoa-don-phat-sinh/tra-cuu`):
//   GET /api/invoice/hdons/temp?sort=ntao:desc&size=…&state=…&search=…
// (single slash — the guard `api//…` blocks curl/browser-fetch without the F5
// cookie; the real page fires the single-slash form, 200).
// Response envelope: `{datas: [...], state, total, time}` where `state` is an
// opaque pagination cursor (pass it back as `state=` for the next page).
pub(crate) const PATH_HDONS_TEMP: &str = "/api/invoice/hdons/temp";

/// Filters for the invoice list. Field names follow the portal's real search
/// format (from `generateSearch`/`generateSearchString` in the portal bundle,
/// verified against the live request `search=hthdon==3;ntao=ge=21/08/2026T00:00:00;
/// ntao=le=20/09/2026T23:59:59;ttxly=in=(0,1,2,3,4,5,6)`):
///   - equal  → `field==v`
///   - range  → `field=ge=X;field=le=Y`
///   - in     → `field=in=(a,b)`
///
/// joined by `;`.
#[derive(Debug, Default, Clone)]
pub(crate) struct InvoiceQuery {
    /// Page size (`size=`).
    pub size: u32,
    /// Opaque pagination cursor from a previous response (`state=`).
    pub state: Option<String>,
    /// Ngày tạo từ ngày (`ntao=ge=dd/MM/yyyyT00:00:00`).
    pub from: Option<String>,
    /// Ngày tạo đến ngày (`ntao=le=dd/MM/yyyyT23:59:59`).
    pub to: Option<String>,
    /// Loại hóa đơn (`hdon`, mã từ danh mục `dmhdons`).
    pub hdon: Option<String>,
    /// Ký hiệu hóa đơn (`khhdon`).
    pub khhdon: Option<String>,
    /// Số hóa đơn (`shdon`).
    pub shdon: Option<String>,
    /// Mã hồ sơ (`mhso`).
    pub mhso: Option<String>,
    /// Trạng thái hóa đơn (`tthai`): 1 Hóa đơn mới … 5 Bị điều chỉnh.
    pub tthai: Option<String>,
    /// Trạng thái xử lý (`ttxly`): single value → `ttxly==v`; `None` →
    /// `ttxly=in=(0,1,2,3,4,5,6)` (trang thật luôn gửi dải này khi "Tất cả").
    pub ttxly: Option<String>,
    /// Mã số thuế lọc (`nbmst=in=(…)`, một giá trị).
    pub nbmst: Option<String>,
}

/// Paginated invoice list returned by the portal.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct InvoiceList {
    pub datas: Vec<serde_json::Value>,
    #[serde(default)]
    pub state: Option<serde_json::Value>,
    #[serde(default)]
    pub total: u64,
    #[serde(default)]
    pub time: u64,
}

/// Successful login result: JWT token + taxpayer profile (if available).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct LoginSession {
    pub token: String,
    pub user: Option<serde_json::Value>,
}

/// Why an automatic login failed.
#[derive(Debug)]
pub(crate) enum LoginError {
    /// The captcha could not be solved within the attempt budget → the UI should
    /// fall back to letting the user type the code.
    Captcha,
    /// Network / protocol / credential error (message is already user-facing).
    Other(String),
}

/// Client bound to one HDDT portal session — reuses a `reqwest::Client` (cookie
/// jar) so later calls keep the session alive like a real browser.
pub(crate) struct HddtClient {
    http: reqwest::Client,
    base: String,
    username: String,
    password: String,
}

/// 3 anti-bot headers required for EVERY portal API call (copied from the
/// portal's own axios interceptor — verified 09/2026: without `request-id` → 403
/// "invalid behavior").
fn anti_bot_headers() -> reqwest::header::HeaderMap {
    use reqwest::header::{HeaderMap, HeaderValue};
    let mut h = HeaderMap::new();
    h.insert(
        "request-id",
        HeaderValue::from_str(&uuid::Uuid::new_v4().to_string())
            .unwrap_or(HeaderValue::from_static("")),
    );
    h.insert("Action", HeaderValue::from_static(""));
    h.insert("End-Point", HeaderValue::from_static("/"));
    h.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("vi"));
    h
}

impl HddtClient {
    /// Create a client pointing at the HDDT portal. `base_url` defaults to the
    /// government portal.
    pub(crate) fn new(
        username: &str,
        password: &str,
        base_url: Option<&str>,
    ) -> Result<Self, String> {
        let base = base_url
            .unwrap_or(BASE_DEFAULT)
            .trim_end_matches('/')
            .to_string();
        let jar = Jar::default();
        let _base_url: reqwest::Url = base
            .parse()
            .map_err(|e| format!("URL cổng HĐĐT không hợp lệ: {e}"))?;
        let mut default_headers = reqwest::header::HeaderMap::new();
        default_headers.insert(
            reqwest::header::REFERER,
            reqwest::header::HeaderValue::from_str(&base).unwrap(),
        );
        let http = reqwest::Client::builder()
            .user_agent(UA)
            .cookie_provider(Arc::new(jar))
            .timeout(Duration::from_secs(20))
            .default_headers(default_headers)
            .build()
            .map_err(|e| format!("Khởi tạo client HĐĐT: {e}"))?;
        Ok(Self {
            http,
            base,
            username: username.to_string(),
            password: password.to_string(),
        })
    }

    #[allow(dead_code)]
    pub(crate) fn base(&self) -> &str {
        &self.base
    }

    /// Step 1: fetch the captcha → `(key, svg)` to display or feed to a solver.
    pub(crate) async fn fetch_captcha(&self) -> Result<Captcha, String> {
        let url = format!("{}{}", self.base, PATH_CAPTCHA);
        let resp = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|e| format!("Lỗi kết nối cổng HĐĐT: {e}"))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(format!(
                "Không lấy được captcha HĐĐT (HTTP {status}) — cổng có thể đang chặn truy cập từ máy này."
            ));
        }
        resp.json::<Captcha>()
            .await
            .map_err(|e| format!("Phản hồi captcha HĐĐT sai định dạng: {e}"))
    }

    /// Step 2: submit login with the solved captcha → JWT token.
    pub(crate) async fn authenticate(&self, key: &str, cvalue: &str) -> Result<String, String> {
        let url = format!("{}{}", self.base, PATH_AUTHENTICATE);
        let body = serde_json::json!({
            "username": self.username,
            "password": self.password,
            "ckey": key,
            "cvalue": cvalue,
        });
        let resp = self
            .http
            .post(&url)
            .headers(anti_bot_headers())
            .header(CONTENT_TYPE, "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Lỗi kết nối cổng HĐĐT: {e}"))?;
        let status = resp.status();
        let v: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Phản hồi đăng nhập HĐĐT sai định dạng: {e}"))?;
        if !status.is_success() {
            let msg = v
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Đăng nhập cổng HĐĐT thất bại.");
            return Err(format!("HĐĐT: {msg}"));
        }
        v.get("token")
            .and_then(|t| t.as_str())
            .map(str::to_string)
            .ok_or_else(|| "Phản hồi đăng nhập HĐĐT thiếu token.".into())
    }

    /// (Optional) taxpayer profile once a token is available.
    pub(crate) async fn profile(&self, token: &str) -> Result<serde_json::Value, String> {
        let url = format!("{}{}", self.base, PATH_PROFILE);
        let resp = self
            .http
            .get(url)
            .headers(anti_bot_headers())
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| format!("Lỗi kết nối cổng HĐĐT: {e}"))?;
        let status = resp.status();
        let v: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Phản hồi hồ sơ HĐĐT sai định dạng: {e}"))?;
        if !status.is_success() {
            return Err(format!("Lỗi lấy hồ sơ HĐĐT (HTTP {status})."));
        }
        Ok(v)
    }

    /// List invoices from the portal (`GET {base}/api/invoice/hdons/temp`).
    ///
    /// Query params mirror the portal's own "Tra cứu hóa đơn" page (verified
    /// live 09/2026): `sort=ntao:desc`, `size`, optional `state` (cursor) and a
    /// `search` string built with the portal's own field operators. Response:
    /// `{datas, state, total, time}`.
    pub(crate) async fn query_invoices(
        &self,
        token: &str,
        q: &InvoiceQuery,
    ) -> Result<InvoiceList, String> {
        let mut params: Vec<(&str, String)> = vec![("sort", "ntao:desc".to_string())];
        params.push(("size", q.size.clamp(1, 500).to_string()));
        if let Some(state) = q.state.as_deref().filter(|s| !s.is_empty()) {
            params.push(("state", state.to_string()));
        }
        let search = build_search_string(q);
        if !search.is_empty() {
            params.push(("search", search));
        }
        let url =
            reqwest::Url::parse_with_params(&format!("{}{}", self.base, PATH_HDONS_TEMP), &params)
                .map_err(|e| format!("URL danh sách HĐĐT không hợp lệ: {e}"))?;
        // Headers giống hệt request thật từ trang tra cứu (đã bắt 09/2026):
        // `action: Tìm kiếm`, `end-point` = path trang, referer trang + Bearer.
        let mut h = anti_bot_headers();
        h.insert(
            "Action",
            HeaderValue::from_static("T%C3%ACm%20ki%E1%BA%BFm"),
        );
        h.insert(
            "End-Point",
            HeaderValue::from_static("/quan-ly-hoa-don-phat-sinh/tra-cuu"),
        );
        let resp = self
            .http
            .get(url)
            .headers(h)
            .bearer_auth(token)
            .header(
                reqwest::header::REFERER,
                format!("{}/quan-ly-hoa-don-phat-sinh/tra-cuu", self.base),
            )
            .header(reqwest::header::ACCEPT, "application/json, text/plain, */*")
            .send()
            .await
            .map_err(|e| format!("Lỗi kết nối cổng HĐĐT: {e}"))?;
        let status = resp.status();
        let v: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Phản hồi danh sách HĐĐT sai định dạng: {e}"))?;
        if !status.is_success() {
            return Err(format!("Lỗi lấy danh sách HĐĐT (HTTP {status}): {v}"));
        }
        serde_json::from_value(v).map_err(|e| format!("Dữ liệu danh sách HĐĐT sai định dạng: {e}"))
    }

    /// Change the portal password (`POST /api/system-taxpayers/users/change-password`,
    /// no captcha — verified from the portal bundle 09/2026). The portal then
    /// invalidates the current session, so the next login must use `new_password`.
    ///
    /// Payload: `{password: <old>, new_password: <new>}` + `Authorization: Bearer`.
    pub(crate) async fn change_password(
        &self,
        token: &str,
        old_password: &str,
        new_password: &str,
    ) -> Result<(), String> {
        let url = format!("{}{}", self.base, PATH_CHANGE_PASSWORD);
        let body = serde_json::json!({
            "password": old_password,
            "new_password": new_password,
        });
        let resp = self
            .http
            .post(&url)
            .headers(anti_bot_headers())
            .header(CONTENT_TYPE, "application/json")
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Lỗi kết nối cổng HĐĐT: {e}"))?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            let msg = serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|v| {
                    v.get("message")
                        .and_then(|m| m.as_str())
                        .map(str::to_string)
                })
                .unwrap_or_else(|| {
                    let t = text.trim();
                    if t.is_empty() {
                        format!("HTTP {status}")
                    } else {
                        format!("HTTP {status}: {t}")
                    }
                });
            return Err(format!("HĐĐT: Đổi mật khẩu thất bại — {msg}"));
        }
        Ok(())
    }

    /// Automatic login using a solver: try up to `max_attempts` captchas,
    /// reloading whenever the answer is wrong ("Mã captcha không đúng.").
    ///
    /// Returns [`LoginError::Captcha`] when the solver never passed, so the caller
    /// can fall back to manual entry; any other failure is returned as
    /// [`LoginError::Other`].
    pub(crate) async fn login_with_solver<S: crate::hddt::captcha::CaptchaSolver>(
        &self,
        solver: &S,
        max_attempts: usize,
    ) -> Result<LoginSession, LoginError> {
        for _ in 0..max_attempts {
            let cap = self.fetch_captcha().await.map_err(LoginError::Other)?;
            let answer = match solver.solve(&cap) {
                Ok(a) if !a.trim().is_empty() => a.trim().to_string(),
                _ => continue,
            };
            match self.authenticate(&cap.key, &answer).await {
                Ok(token) => {
                    let user = self.profile(&token).await.ok();
                    return Ok(LoginSession { token, user });
                }
                // Wrong captcha → fetch a new one and retry.
                Err(e) if e.starts_with("HĐĐT: Mã captcha") => continue,
                Err(e) => return Err(LoginError::Other(e)),
            }
        }
        Err(LoginError::Captcha)
    }
}

/// Build the `search` query string for `hdons/temp` using the portal's own
/// operator format (from `generateSearch` + `generateSearchString` in the portal
/// bundle, cross-checked against the live request 09/2026):
///
///   - equal op → `field==value`
///   - range → `field=ge=X;field=le=Y`
///   - in → `field=in=(a,b)`
///
/// fields joined by `;`. Dates use `dd/MM/yyyyTHH:mm:ss` (the portal appends the
/// time — `T00:00:00` từ ngày, `T23:59:59` đến ngày). `hthdon==3` ("Hóa đơn điện
/// tử") luôn được thêm trước; `ttxly` rỗng → `in=(0,1,2,3,4,5,6)` (giống trang
/// thật khi chọn "Tất cả").
fn build_search_string(q: &InvoiceQuery) -> String {
    let mut parts: Vec<String> = vec!["hthdon==3".to_string()];
    fn push_eq(parts: &mut Vec<String>, field: &str, val: &str) {
        if !val.is_empty() {
            parts.push(format!("{field}=={val}"));
        }
    }
    if let Some(v) = q.nbmst.as_deref() {
        let v = v.trim();
        if !v.is_empty() {
            parts.push(format!("nbmst=in=({v})"));
        }
    }
    if let Some(v) = q.hdon.as_deref() {
        push_eq(&mut parts, "hdon", v.trim());
    }
    if let Some(v) = q.khhdon.as_deref() {
        push_eq(&mut parts, "khhdon", v.trim().to_uppercase().as_str());
    }
    if let Some(v) = q.shdon.as_deref() {
        push_eq(&mut parts, "shdon", v.trim());
    }
    if let Some(v) = q.mhso.as_deref() {
        push_eq(&mut parts, "mhso", v.trim());
    }
    if let Some(v) = q.tthai.as_deref() {
        push_eq(&mut parts, "tthai", v.trim());
    }
    if let Some(v) = q.from.as_deref().filter(|s| !s.is_empty()) {
        parts.push(format!("ntao=ge={v}T00:00:00"));
    }
    if let Some(v) = q.to.as_deref().filter(|s| !s.is_empty()) {
        parts.push(format!("ntao=le={v}T23:59:59"));
    }
    match q.ttxly.as_deref().filter(|s| !s.is_empty()) {
        Some(v) => parts.push(format!("ttxly=={v}")),
        None => parts.push("ttxly=in=(0,1,2,3,4,5,6)".to_string()),
    }
    parts.join(";")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anti_bot_headers_has_uuid_request_id() {
        let h = anti_bot_headers();
        let rid = h
            .get("request-id")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let uuid_re = regex::Regex::new(
            r"^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$",
        )
        .unwrap();
        assert!(
            uuid_re.is_match(rid),
            "request-id must be a UUID v4, got: {rid}"
        );
        assert_eq!(h.get("End-Point").and_then(|v| v.to_str().ok()), Some("/"));
        assert_eq!(
            h.get(ACCEPT_LANGUAGE).and_then(|v| v.to_str().ok()),
            Some("vi")
        );
    }

    #[test]
    fn base_url_normalized() {
        let c = HddtClient::new("u", "p", Some("https://hoadondientu.gdt.gov.vn/")).unwrap();
        assert_eq!(c.base(), "https://hoadondientu.gdt.gov.vn");
    }

    #[test]
    fn captcha_deserialize_from_live_shape() {
        let raw = r##"{"key":"6aaec73f3ea3b029a8fa4669","content":"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"200\" height=\"40\"><path fill=\"#111\" d=\"M1 2 Z\"/></svg>"}"##;
        let c: Captcha = serde_json::from_str(raw).unwrap();
        assert_eq!(c.key.len(), 24);
        assert!(c.content.contains("<svg "));
    }

    #[test]
    fn search_string_matches_live_request() {
        // Request thật từ trang tra cứu (chụp 09/2026):
        //   search=hthdon==3;ntao=ge=21/08/2026T00:00:00;ntao=le=20/09/2026T23:59:59;ttxly=in=(0,1,2,3,4,5,6)
        let q = InvoiceQuery {
            size: 15,
            from: Some("21/08/2026".into()),
            to: Some("20/09/2026".into()),
            ..Default::default()
        };
        assert_eq!(
            build_search_string(&q),
            "hthdon==3;ntao=ge=21/08/2026T00:00:00;ntao=le=20/09/2026T23:59:59;ttxly=in=(0,1,2,3,4,5,6)"
        );
    }

    #[test]
    fn search_string_equal_and_in_operators() {
        let q = InvoiceQuery {
            size: 50,
            hdon: Some("01GTKT0".into()),
            khhdon: Some("01GTKT0/001".into()),
            shdon: Some(" 12 ".into()),
            mhso: Some("HOSO-1".into()),
            tthai: Some("3".into()),
            ttxly: Some("5".into()),
            nbmst: Some("0108537801".into()),
            ..Default::default()
        };
        assert_eq!(
            build_search_string(&q),
            "hthdon==3;nbmst=in=(0108537801);hdon==01GTKT0;khhdon==01GTKT0/001;shdon==12;mhso==HOSO-1;tthai==3;ttxly==5"
        );
    }
}
