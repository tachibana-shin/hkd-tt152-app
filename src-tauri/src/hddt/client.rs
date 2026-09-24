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

// ─── Tra cứu hóa đơn (trang `/tra-cuu/tra-cuu-hoa-don`) ───
// Trang này có 2 tab lớn × 2 tab nhỏ, mỗi ô gọi một endpoint riêng — bắt live
// 24/09/2026 (mỗi request đều 200 kể cả khi không có hóa đơn):
//
//   | Tab lớn         | Tab nhỏ (loại)             | Endpoint                            |
//   |-----------------|----------------------------|-------------------------------------|
//   | Hóa đơn ra      | Hóa đơn điện tử           | `/api/query/invoices/sold`          |
//   | Hóa đơn ra      | Máy tính tiền (có mã CQT)  | `/api/sco-query/invoices/sold`      |
//   | Hóa đơn vào     | Hóa đơn điện tử           | `/api/query/invoices/purchase`      |
//   | Hóa đơn vào     | Máy tính tiền (có mã CQT)  | `/api/sco-query/invoices/purchase`  |
//
// Query: `?sort=tdlap:desc&size=…&state=…&search=…` (ngày lập HĐ = `tdlap`).
// `search` dùng đúng operator của trang: `field==v`, `field=ge=X;field=le=Y`,
// `unhiem==1` (chỉ khi tick), các mục nối bằng `;`.
// Response envelope: `{datas: [...], state, total, time}` — `state` là cursor
// phân trang (truyền lại làm `state=`).
const ENDPOINT_LOOKUP: &str = "/tra-cuu/tra-cuu-hoa-don";

/// Tab lớn của trang tra cứu: hóa đơn ra (bán ra) hay hóa đơn vào (mua vào).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InvoiceDirection {
    /// "Tra cứu hóa đơn điện tử bán ra" → `…/invoices/sold`.
    #[default]
    Sold,
    /// "Tra cứu hóa đơn điện tử mua vào" → `…/invoices/purchase`.
    Purchase,
}

impl InvoiceDirection {
    fn segment(self) -> &'static str {
        match self {
            Self::Sold => "sold",
            Self::Purchase => "purchase",
        }
    }

    /// Nhãn tiếng Việt dùng cho header `Action` (portal `encodeURIComponent` nó).
    fn action(self, kind: InvoiceKind) -> String {
        let base = match self {
            Self::Sold => "Tìm kiếm (hóa đơn bán ra)",
            Self::Purchase => "Tìm kiếm (hóa đơn mua vào)",
        };
        match kind {
            InvoiceKind::Regular => base.to_string(),
            InvoiceKind::CashRegister => base.replace("hóa đơn ", "hóa đơn máy tính tiền "),
        }
    }
}

/// Tab nhỏ (loại hóa đơn) — quyết định tiền tố API, không phải filter.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InvoiceKind {
    /// "Hóa đơn điện tử" (có mã của NNT) → tiền tố `query`.
    #[default]
    Regular,
    /// "Hóa đơn có mã khởi tạo từ máy tính tiền" → tiền tố `sco-query`.
    CashRegister,
}

impl InvoiceKind {
    fn prefix(self) -> &'static str {
        match self {
            Self::Regular => "query",
            Self::CashRegister => "sco-query",
        }
    }
}

/// Bộ lọc tra cứu hóa đơn (khớp form trang `/tra-cuu/tra-cuu-hoa-don`).
///
/// Tên field và operator lấy đúng từ `onSubmit` của trang (đã đối chiếu với
/// request thật):
/// `search=tdlap=ge=dd/MM/yyyyT00:00:00;tdlap=le=dd/MM/yyyyT23:59:59;nbmst==…;
///  nmmst==…;tthai==…;ttxly==…;khmshdon==…;khhdon==…;shdon==…;nmcmnd==…;unhiem==1`
#[derive(Debug, Default, Clone)]
pub(crate) struct InvoiceQuery {
    /// Tab lớn (bán ra / mua vào) → chọn endpoint.
    pub direction: InvoiceDirection,
    /// Tab nhỏ (hóa đơn điện tử / máy tính tiền) → tiền tố endpoint.
    pub kind: InvoiceKind,
    /// Page size (`size=`).
    pub size: u32,
    /// Opaque pagination cursor từ response trước (`state=`).
    pub state: Option<String>,
    /// Ngày lập từ ngày (`tdlap=ge=dd/MM/yyyyT00:00:00`).
    pub from: Option<String>,
    /// Ngày lập đến ngày (`tdlap=le=dd/MM/yyyyT23:59:59`).
    pub to: Option<String>,
    /// Mã số thuế (`nmmst`) — select "Mã số thuế" của trang.
    pub nmmst: Option<String>,
    /// MST đối tác (`nbmst`) — "MST người mua" (bán ra) / "MST người bán" (mua vào).
    pub nbmst: Option<String>,
    /// CCCD người mua (`nmcmnd`; form gọi là `nmcccd` rồi map sang `nmcmnd`).
    pub nmcmnd: Option<String>,
    /// Trạng thái hóa đơn (`tthai`): 1 mới … 6 đã bị hủy; `0` = Tất cả (bỏ qua).
    pub tthai: Option<String>,
    /// Kết quả kiểm tra (`ttxly`): `-1` = Tất cả (bỏ qua), `5` = Đã cấp mã HĐ.
    pub ttxly: Option<String>,
    /// Ký hiệu mẫu số hóa đơn (`khmshdon`, 1..9 theo danh mục `dmhdons`).
    pub khmshdon: Option<String>,
    /// Ký hiệu hóa đơn (`khhdon`, portal tự upper-case).
    pub khhdon: Option<String>,
    /// Số hóa đơn (`shdon`).
    pub shdon: Option<String>,
    /// Hóa đơn ủy nhiệm (`unhiem==1`, chỉ gửi khi true).
    pub unhiem: bool,
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

    /// Chi tiết 1 hóa đơn (dòng hàng `hdhhdvu[]`) — dùng để tạo phiếu nhập kho.
    ///
    /// Kiểm chứng live 24/09/2026: endpoint chi tiết **luôn** dùng tiền tố
    /// `sco-query` cho cả hóa đơn thường lẫn máy tính tiền; gọi `query` trả
    /// 403 "Không có quyền xem hóa đơn này".
    ///
    /// `GET {base}/api/sco-query/invoices/detail?nbmst&khmshdon&khhdon&shdon[&id]`
    pub(crate) async fn invoice_detail(
        &self,
        token: &str,
        nbmst: &str,
        khmshdon: &str,
        khhdon: &str,
        shdon: &str,
        portal_id: &str,
    ) -> Result<serde_json::Value, String> {
        let mut params: Vec<(&str, String)> = vec![
            ("nbmst", nbmst.to_string()),
            ("khmshdon", khmshdon.to_string()),
            ("khhdon", khhdon.to_string()),
            ("shdon", shdon.to_string()),
        ];
        if !portal_id.is_empty() {
            params.push(("id", portal_id.to_string()));
        }
        let path = "/api/sco-query/invoices/detail";
        let url = reqwest::Url::parse_with_params(&format!("{}{}", self.base, path), &params)
            .map_err(|e| format!("URL chi tiết hóa đơn không hợp lệ: {e}"))?;
        let page = format!("{}{}", self.base, ENDPOINT_LOOKUP);
        let mut h = anti_bot_headers();
        h.insert("End-Point", HeaderValue::from_static(ENDPOINT_LOOKUP));
        let resp = self
            .http
            .get(url)
            .headers(h)
            .bearer_auth(token)
            .header(reqwest::header::REFERER, page)
            .header(reqwest::header::ACCEPT, "application/json, text/plain, */*")
            .send()
            .await
            .map_err(|e| format!("Lỗi kết nối cổng HĐĐT: {e}"))?;
        let status = resp.status();
        let v: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Phản hồi chi tiết hóa đơn sai định dạng: {e}"))?;
        if !status.is_success() {
            let msg = v
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("không rõ nguyên nhân");
            return Err(format!("Lỗi lấy chi tiết hóa đơn (HTTP {status}): {msg}"));
        }
        Ok(v)
    }

    /// Tra cứu hóa đơn theo tab đang chọn (2×2: bán ra/mua vào × thường/máy tính tiền).
    ///
    /// `GET {base}/api/{query|sco-query}/invoices/{sold|purchase}` với
    /// `sort=tdlap:desc`, `size`, `state` (cursor) và `search` dựng theo đúng
    /// operator của trang. Headers bắt chước request thật (24/09/2026):
    /// `Action` = tên hành động đã URL-encode, `End-Point` = `/tra-cuu/tra-cuu-hoa-don`,
    /// `Referer` = trang tra cứu, `Authorization: Bearer …`.
    /// Response: `{datas, state, total, time}`.
    pub(crate) async fn query_invoices(
        &self,
        token: &str,
        q: &InvoiceQuery,
    ) -> Result<InvoiceList, String> {
        let path = format!(
            "/api/{}/invoices/{}",
            q.kind.prefix(),
            q.direction.segment()
        );
        let mut params: Vec<(&str, String)> = vec![("sort", "tdlap:desc".to_string())];
        params.push(("size", q.size.clamp(1, 500).to_string()));
        if let Some(state) = q.state.as_deref().filter(|s| !s.is_empty()) {
            params.push(("state", state.to_string()));
        }
        let search = build_search_string(q);
        if !search.is_empty() {
            params.push(("search", search));
        }
        let url = reqwest::Url::parse_with_params(&format!("{}{}", self.base, path), &params)
            .map_err(|e| format!("URL danh sách HĐĐT không hợp lệ: {e}"))?;
        let action = encode_uri_component(&q.direction.action(q.kind));
        let page = format!("{}{}", self.base, ENDPOINT_LOOKUP);
        let mut h = anti_bot_headers();
        if let Ok(v) = HeaderValue::from_str(&action) {
            h.insert("Action", v);
        }
        h.insert("End-Point", HeaderValue::from_static(ENDPOINT_LOOKUP));
        let resp = self
            .http
            .get(url)
            .headers(h)
            .bearer_auth(token)
            .header(reqwest::header::REFERER, page)
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
            let msg = v
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("không rõ nguyên nhân");
            return Err(format!("Lỗi lấy danh sách HĐĐT (HTTP {status}): {msg}"));
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

/// `encodeURIComponent` (portal dùng để build header `Action`): mọi ký tự ngoài
/// `A-Z a-z 0-9 - _ . ! ~ * ' ( )` đều percent-encode theo byte UTF-8.
fn encode_uri_component(input: &str) -> String {
    const UNRESERVED: &[u8] = b"-_.!~*'()";
    let mut out = String::with_capacity(input.len());
    for &b in input.as_bytes() {
        if b.is_ascii_alphanumeric() || UNRESERVED.contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Build chuỗi `search` cho trang tra cứu hóa đơn.
///
/// Thứ tự mục giống hệt trang portal (bắt live 24/09/2026):
/// khoảng ngày lập trước, rồi `nmmst`, `nbmst`, `tthai`, `ttxly`, `khmshdon`,
/// `shdon`, `khhdon`, `unhiem`, cuối cùng `nmcmnd` (portal map `nmcccd`→`nmcmnd`
/// sau khi dựng object nên nó nằm cuối chuỗi).
///
/// - `tthai == "0"` và `ttxly == "-1"` là giá trị "Tất cả" → bỏ qua.
/// - `unhiem` chỉ gửi khi true (portal hard-code `;unhiem==1`).
fn build_search_string(q: &InvoiceQuery) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(v) = q.from.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        parts.push(format!("tdlap=ge={v}T00:00:00"));
    }
    if let Some(v) = q.to.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        parts.push(format!("tdlap=le={v}T23:59:59"));
    }
    fn push_eq(parts: &mut Vec<String>, field: &str, val: Option<&str>, upper: bool) {
        let Some(v) = val.map(str::trim).filter(|s| !s.is_empty()) else {
            return;
        };
        let v = if upper {
            v.to_uppercase()
        } else {
            v.to_string()
        };
        parts.push(format!("{field}=={v}"));
    }
    push_eq(&mut parts, "nmmst", q.nmmst.as_deref(), false);
    push_eq(&mut parts, "nbmst", q.nbmst.as_deref(), false);
    // "Tất cả" của trạng thái HĐ (0) và kết quả kiểm tra (-1) không được gửi.
    push_eq(
        &mut parts,
        "tthai",
        q.tthai.as_deref().filter(|v| v.trim() != "0"),
        false,
    );
    push_eq(
        &mut parts,
        "ttxly",
        q.ttxly.as_deref().filter(|v| v.trim() != "-1"),
        false,
    );
    push_eq(&mut parts, "khmshdon", q.khmshdon.as_deref(), false);
    push_eq(&mut parts, "shdon", q.shdon.as_deref(), false);
    push_eq(&mut parts, "khhdon", q.khhdon.as_deref(), true);
    if q.unhiem {
        parts.push("unhiem==1".to_string());
    }
    push_eq(&mut parts, "nmcmnd", q.nmcmnd.as_deref(), false);
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
        // Request thật bắt 24/09/2026 — tab "mua vào" × "Hóa đơn điện tử":
        //   search=tdlap=ge=25/08/2026T00:00:00;tdlap=le=24/09/2026T23:59:59;ttxly==5
        let q = InvoiceQuery {
            direction: InvoiceDirection::Purchase,
            kind: InvoiceKind::Regular,
            size: 15,
            from: Some("25/08/2026".into()),
            to: Some("24/09/2026".into()),
            ttxly: Some("5".into()),
            ..Default::default()
        };
        assert_eq!(
            build_search_string(&q),
            "tdlap=ge=25/08/2026T00:00:00;tdlap=le=24/09/2026T23:59:59;ttxly==5"
        );
    }

    #[test]
    fn search_string_sold_regular_has_no_ttxly_when_all() {
        // Tab "bán ra" × "Hóa đơn điện tử", "Kết quả kiểm tra" = Tất cả (-1).
        let q = InvoiceQuery {
            direction: InvoiceDirection::Sold,
            kind: InvoiceKind::Regular,
            size: 15,
            from: Some("25/08/2026".into()),
            to: Some("24/09/2026".into()),
            ttxly: Some("-1".into()),
            ..Default::default()
        };
        assert_eq!(
            build_search_string(&q),
            "tdlap=ge=25/08/2026T00:00:00;tdlap=le=24/09/2026T23:59:59"
        );
    }

    #[test]
    fn search_string_all_fields_order_matches_live_request() {
        // Hình dạng request thật khi điền MST đối tác + CCCD + số HĐ + ký hiệu + ủy nhiệm.
        // MST/CCCD dùng số giả (không nhúng dữ liệu đối tác thật vào repo).
        let q = InvoiceQuery {
            direction: InvoiceDirection::Purchase,
            kind: InvoiceKind::Regular,
            size: 15,
            from: Some("25/08/2026".into()),
            to: Some("24/09/2026".into()),
            nbmst: Some("0100888888".into()),
            nmcmnd: Some("001088888888".into()),
            ttxly: Some("5".into()),
            shdon: Some("821".into()),
            khhdon: Some("c26thn".into()),
            unhiem: true,
            ..Default::default()
        };
        assert_eq!(
            build_search_string(&q),
            "tdlap=ge=25/08/2026T00:00:00;tdlap=le=24/09/2026T23:59:59;\
             nbmst==0100888888;ttxly==5;shdon==821;khhdon==C26THN;unhiem==1;\
             nmcmnd==001088888888"
        );
    }

    #[test]
    fn search_string_skips_blank_and_all_values() {
        let q = InvoiceQuery {
            nmmst: Some("  ".into()),
            nbmst: Some("0100999999".into()),
            tthai: Some("0".into()),
            ttxly: Some("-1".into()),
            khmshdon: Some("1".into()),
            ..Default::default()
        };
        assert_eq!(build_search_string(&q), "nbmst==0100999999;khmshdon==1");
    }

    #[test]
    fn endpoints_and_actions_match_portal() {
        let cases = [
            (
                InvoiceDirection::Sold,
                InvoiceKind::Regular,
                "/api/query/invoices/sold",
                "T%C3%ACm%20ki%E1%BA%BFm%20(h%C3%B3a%20%C4%91%C6%A1n%20b%C3%A1n%20ra)",
            ),
            (
                InvoiceDirection::Sold,
                InvoiceKind::CashRegister,
                "/api/sco-query/invoices/sold",
                "T%C3%ACm%20ki%E1%BA%BFm%20(h%C3%B3a%20%C4%91%C6%A1n%20m%C3%A1y%20t%C3%ADnh%20\
                 ti%E1%BB%81n%20b%C3%A1n%20ra)",
            ),
            (
                InvoiceDirection::Purchase,
                InvoiceKind::Regular,
                "/api/query/invoices/purchase",
                "T%C3%ACm%20ki%E1%BA%BFm%20(h%C3%B3a%20%C4%91%C6%A1n%20mua%20v%C3%A0o)",
            ),
            (
                InvoiceDirection::Purchase,
                InvoiceKind::CashRegister,
                "/api/sco-query/invoices/purchase",
                "T%C3%ACm%20ki%E1%BA%BFm%20(h%C3%B3a%20%C4%91%C6%A1n%20m%C3%A1y%20t%C3%ADnh%20\
                 ti%E1%BB%81n%20mua%20v%C3%A0o)",
            ),
        ];
        for (dir, kind, path, action) in cases {
            assert_eq!(
                format!("/api/{}/invoices/{}", kind.prefix(), dir.segment()),
                path
            );
            assert_eq!(encode_uri_component(&dir.action(kind)), action);
        }
    }

    #[test]
    fn unhiem_omitted_when_unchecked() {
        let q = InvoiceQuery::default();
        assert_eq!(build_search_string(&q), "");
    }
}
