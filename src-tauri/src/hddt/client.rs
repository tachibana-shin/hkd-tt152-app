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
use reqwest::header::{ACCEPT_LANGUAGE, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

const BASE_DEFAULT: &str = "https://hoadondientu.gdt.gov.vn";
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";

pub(crate) const PATH_CAPTCHA: &str = "/api/captcha";
pub(crate) const PATH_AUTHENTICATE: &str = "/api/security-taxpayer/authenticate";
pub(crate) const PATH_PROFILE: &str = "/api/security-taxpayer/profile";

/// Successful login result: JWT token + taxpayer profile (if available).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct LoginSession {
    pub token: String,
    pub user: Option<serde_json::Value>,
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
            .get(&url)
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
            .get(&url)
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

    /// Single automatic login using a solver: try up to `max_attempts` captchas,
    /// reloading whenever the answer is wrong ("Mã captcha không đúng.").
    pub(crate) async fn login_with_solver<S: crate::hddt::captcha::CaptchaSolver>(
        &self,
        solver: &S,
        max_attempts: usize,
    ) -> Result<LoginSession, String> {
        let mut last_err = "Chưa thử lần nào.".to_string();
        for attempt in 1..=max_attempts {
            let cap = self.fetch_captcha().await?;
            let answer = match solver.solve(&cap) {
                Ok(a) if !a.trim().is_empty() => a.trim().to_string(),
                _ => {
                    last_err = "Solver không ra kết quả.".into();
                    continue;
                }
            };
            match self.authenticate(&cap.key, &answer).await {
                Ok(token) => {
                    let user = self.profile(&token).await.ok();
                    return Ok(LoginSession { token, user });
                }
                Err(e) if e.starts_with("HĐĐT: Mã captcha") => {
                    last_err = format!("{e} (lần {attempt}/{max_attempts})");
                }
                Err(e) => return Err(e),
            }
        }
        Err(last_err)
    }
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
}
