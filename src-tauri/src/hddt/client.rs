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

use crate::hddt::Captcha;
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

/// Cổng tự validate `size`: vượt 50 là HTTP 500 kèm message
/// `findInvoicePurchase.size: phải nhỏ hơn hoặc bằng 50` (bắt live 25/09/2026).
/// Client clamp theo đúng giới hạn này để không gửi giá trị bị cổng từ chối.
pub(crate) const MAX_PAGE_SIZE: u32 = 50;

/// Đọc body cổng thành `serde_json::Value`, **luôn decode UTF-8 từ byte thô**.
///
/// `Response::json()` của reqwest đi qua `text()` → tôn trọng `charset` trong
/// `Content-Type`, còn một số response lỗi của cổng khai charset 1-byte dù byte
/// thực là UTF-8 nên message tiếng Việt ra mojibake. Ưu tiên UTF-8; byte không
/// hợp lệ thì thay bằng U+FFFD.
fn decode_body(bytes: &[u8]) -> Result<serde_json::Value, String> {
    let text = match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => String::from_utf8_lossy(bytes).into_owned(),
    };
    serde_json::from_str(&text)
        .map_err(|e| format!("Phản hồi cổng HĐĐT sai định dạng: {e} (thân: {text})"))
}

async fn read_json_body(resp: reqwest::Response) -> Result<serde_json::Value, String> {
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("Đọc phản hồi cổng HĐĐT lỗi: {e}"))?;
    decode_body(&bytes)
}

/// Nội dung `message` trong body lỗi của cổng (Spring Boot: `{"message": …}`).
fn portal_message(v: &serde_json::Value, fallback: &str) -> String {
    v.get("message")
        .and_then(|m| m.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| fallback.to_string())
}

/// Giá trị `size=` gửi lên cổng — luôn trong `[1, MAX_PAGE_SIZE]` vì cổng tự
/// validate (vượt 50 → HTTP 500, quét cổng bị chặn cả lô).
fn page_size_param(size: u32) -> String {
    size.clamp(1, MAX_PAGE_SIZE).to_string()
}

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
        self.as_str()
    }

    /// Mã máy của chiều hóa đơn — đúng giá trị lưu trong cột `direction` của
    /// bảng `hddt_invoice_xml` và dạng frontend truyền vào lệnh xuất XML.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Sold => "sold",
            Self::Purchase => "purchase",
        }
    }

    /// Đọc lại từ mã máy (`sold`/`purchase`); giá trị lạ → `Err`.
    pub(crate) fn parse(s: &str) -> Result<Self, String> {
        match s.trim() {
            "sold" | "" => Ok(Self::Sold),
            "purchase" => Ok(Self::Purchase),
            other => Err(format!("Chiều hóa đơn không hợp lệ: {other}")),
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

    /// Mã máy của loại hóa đơn — đúng giá trị lưu trong cột `kind` của bảng
    /// `hddt_invoice_xml` và dạng frontend truyền vào lệnh xuất XML.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Regular => "regular",
            Self::CashRegister => "cash-register",
        }
    }

    /// Đọc lại từ mã máy; giá trị lạ → `Err`.
    pub(crate) fn parse(s: &str) -> Result<Self, String> {
        match s.trim() {
            "regular" | "" => Ok(Self::Regular),
            "cash-register" => Ok(Self::CashRegister),
            other => Err(format!("Loại hóa đơn không hợp lệ: {other}")),
        }
    }

    /// Nhãn tiếng Việt cho header `Action` khi **xuất XML** (portal lưu vào
    /// `localStorage.action` rồi đọc lại thành header — bắt từ bundle 10/2026).
    fn export_action(self, direction: InvoiceDirection) -> String {
        let base = match direction {
            InvoiceDirection::Sold => "hóa đơn bán ra",
            InvoiceDirection::Purchase => "hóa đơn mua vào",
        };
        match self {
            InvoiceKind::Regular => format!("Xuất xml ({base})"),
            InvoiceKind::CashRegister => {
                format!("Xuất xml (hóa đơn máy tính tiền {base})")
            }
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
    /// Mốc thời gian request tra cứu gần nhất — dùng để giãn nhịp (xem `throttle`).
    last_call: std::sync::Mutex<Option<std::time::Instant>>,
    /// Khoảng nghỉ tối thiểu giữa 2 request (chống 429 của cổng).
    min_call_gap: Duration,
    /// Chờ rồi thử lại (giây) khi bị 429.
    retry_429_delays: Vec<u64>,
    /// Chờ rồi thử lại (giây) khi request hỏng giữa đường (timeout, đứt mạng).
    retry_net_delays: Vec<u64>,
}

/// Giãn nhịp giữa 2 request liên tiếp: cổng trả 429 "Too Many Requests" khi quét
/// liên tiếp nhiều hóa đơn (bắt lỗi thật 25/09/2026 khi lấy chi tiết 14 HĐ liền).
const MIN_CALL_GAP: Duration = Duration::from_millis(700);
/// Khi bị 429, chờ rồi thử lại theo từng lần.
const RETRY_429_DELAYS: [u64; 3] = [3, 8, 20];
/// Lỗi kết nối (timeout, đứt mạng): thử lại 3 lần rồi mới báo lỗi — thường chỉ
/// vài giây là kết nối lại được.
const RETRY_NET_DELAYS: [u64; 3] = [2, 5, 10];

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
            last_call: std::sync::Mutex::new(None),
            min_call_gap: MIN_CALL_GAP,
            retry_429_delays: RETRY_429_DELAYS.to_vec(),
            retry_net_delays: RETRY_NET_DELAYS.to_vec(),
        })
    }

    /// Client cho test offline: không giãn nhịp, retry 429 tức thì (xem
    /// `mock_portal`). Giữ nguyên mọi logic khác.
    #[cfg(test)]
    pub(crate) fn for_test(base: &str) -> Self {
        let mut c = Self::new("u", "p", Some(base)).expect("client test");
        c.min_call_gap = Duration::ZERO;
        c.retry_429_delays = vec![0, 0, 0];
        c.retry_net_delays = vec![0, 0, 0];
        c
    }

    /// Chờ đủ khoảng cách giữa 2 request tra cứu (chống 429 của cổng).
    async fn throttle(&self) {
        let wait = {
            let mut last = self.last_call.lock().unwrap_or_else(|e| e.into_inner());
            let now = std::time::Instant::now();
            let gap = match *last {
                Some(prev) => self.min_call_gap.saturating_sub(now.duration_since(prev)),
                None => Duration::ZERO,
            };
            *last = Some(now + gap);
            gap
        };
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
    }

    /// GET có giãn nhịp + tự thử lại khi cổng trả 429.
    async fn get_throttled(
        &self,
        url: reqwest::Url,
        token: &str,
        referer: &str,
    ) -> Result<reqwest::Response, String> {
        self.get_throttled_with(url, token, referer, String::new())
            .await
    }

    /// Như [`Self::get_throttled`], thêm header `Action` (bắt buộc cho endpoint
    /// tra cứu danh sách — header này chọn đúng hành động của tab trên cổng).
    ///
    /// Tự thử lại khi cổng bị giới hạn tần suất (429) **và** khi request hỏng
    /// giữa đường (timeout, đứt kết nối): quét hàng loạt dễ gặp cả hai, mà một
    /// lần rớt mạng thì hóa đơn bị đánh "Cần xử lý" dù lúc đó mọi thứ bình
    /// thường. Hết số lần thử thì trả lỗi kèm số lần đã thử để người dùng
    /// biết nguyên nhân.
    async fn get_throttled_with(
        &self,
        url: reqwest::Url,
        token: &str,
        referer: &str,
        action: String,
    ) -> Result<reqwest::Response, String> {
        let mut rate_attempt = 0;
        let mut net_attempt = 0;
        let max_net = self.retry_net_delays.len();
        loop {
            self.throttle().await;
            let mut h = anti_bot_headers();
            if !action.is_empty() {
                if let Ok(v) = HeaderValue::from_str(&action) {
                    h.insert("Action", v);
                }
            }
            h.insert("End-Point", HeaderValue::from_static(ENDPOINT_LOOKUP));
            let sent = self
                .http
                .get(url.clone())
                .headers(h)
                .bearer_auth(token)
                .header(reqwest::header::REFERER, referer.to_string())
                .header(reqwest::header::ACCEPT, "application/json, text/plain, */*")
                .send()
                .await;
            let resp = match sent {
                Ok(r) => r,
                Err(e) => {
                    if net_attempt >= max_net {
                        return Err(format!(
                            "Lỗi kết nối cổng HĐĐT (đã thử {} lần): {e}",
                            max_net + 1
                        ));
                    }
                    let secs = self.retry_net_delays[net_attempt];
                    net_attempt += 1;
                    rate_attempt = 0;
                    if secs > 0 {
                        tokio::time::sleep(Duration::from_secs(secs)).await;
                    }
                    continue;
                }
            };
            if resp.status() != reqwest::StatusCode::TOO_MANY_REQUESTS
                || rate_attempt >= self.retry_429_delays.len()
            {
                return Ok(resp);
            }
            let secs = self.retry_429_delays[rate_attempt];
            rate_attempt += 1;
            if secs > 0 {
                tokio::time::sleep(Duration::from_secs(secs)).await;
            }
        }
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
        let v = read_json_body(resp).await?;
        if !status.is_success() {
            let msg = portal_message(&v, "Đăng nhập cổng HĐĐT thất bại.");
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
        let v = read_json_body(resp).await?;
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
        let resp = self.get_throttled(url, token, &page).await?;
        let status = resp.status();
        let v = read_json_body(resp).await?;
        if !status.is_success() {
            let msg = portal_message(&v, "không rõ nguyên nhân");
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
        params.push(("size", page_size_param(q.size)));
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
        // `Action` phải set trước khi gửi; `get_throttled` dựng phần header còn lại.
        let resp = self.get_throttled_with(url, token, &page, action).await?;
        let status = resp.status();
        let v = read_json_body(resp).await?;
        if !status.is_success() {
            let msg = portal_message(&v, "không rõ nguyên nhân");
            return Err(format!("Lỗi lấy danh sách HĐĐT (HTTP {status}): {msg}"));
        }
        serde_json::from_value(v).map_err(|e| format!("Dữ liệu danh sách HĐĐT sai định dạng: {e}"))
    }

    /// Tải hồ sơ XML của **1** hóa đơn dưới dạng file ZIP.
    ///
    /// `GET {base}/api/{query|sco-query}/invoices/export-xml?nbmst&khhdon&shdon&khmshdon`
    ///
    /// Bắt từ bundle portal 10/2026 (nút "Xuất xml" ở trang `/tra-cuu/tra-cuu-hoa-don`):
    /// frontend gọi `sendGetBlob(url, {nbmst,khhdon,shdon,khmshdon}, jwt)` rồi
    /// `exportFile("invoice", blob, "zip")` — tức response là **ZIP binary**
    /// (không phải JSON), nên client đọc byte thô chứ không qua `read_json_body`.
    /// Header `Action` = nhãn xuất XML của đúng tab (xem [`InvoiceKind::export_action`]).
    ///
    /// Sides: tab "hóa đơn bán ra" kiểm `hsgoc` (hồ sơ gốc) trước khi gọi — nếu
    /// thiếu thì cổng không có gì để tải; tab "mua vào" gọi thẳng.
    pub(crate) async fn export_invoice_xml(
        &self,
        token: &str,
        key: &crate::hddt::xml::XmlInvoiceKey,
    ) -> Result<Vec<u8>, String> {
        let path = format!("/api/{}/invoices/export-xml", key.kind.prefix());
        let params: Vec<(&str, String)> = vec![
            ("nbmst", key.nbmst.clone()),
            ("khhdon", key.khhdon.clone()),
            ("shdon", key.shdon.clone()),
            ("khmshdon", key.khmshdon.to_string()),
        ];
        let url = reqwest::Url::parse_with_params(&format!("{}{}", self.base, path), &params)
            .map_err(|e| format!("URL xuất XML HĐĐT không hợp lệ: {e}"))?;
        let action = encode_uri_component(&key.kind.export_action(key.direction));
        let page = format!("{}{}", self.base, ENDPOINT_LOOKUP);
        let resp = self.get_throttled_with(url, token, &page, action).await?;
        let status = resp.status();
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| format!("Đọc file XML HĐĐT lỗi: {e}"))?;
        if !status.is_success() {
            // Lỗi của cổng vẫn là JSON (`{"message": …}`) — đọc để lấy message.
            let v = decode_body(&bytes).unwrap_or(serde_json::Value::Null);
            let msg = portal_message(&v, "không rõ nguyên nhân");
            return Err(format!("Lỗi tải XML hóa đơn (HTTP {status}): {msg}"));
        }
        // Phải là ZIP thật (magic `PK\x03\x04`); HTTP 200 mà trả JSON là cổng
        // báo lỗi trong thân — đọc message cho người dùng thay vì lưu nhầm.
        if bytes.len() < 4 || &bytes[..4] != b"PK\x03\x04" {
            let v = decode_body(&bytes).unwrap_or(serde_json::Value::Null);
            let msg = portal_message(&v, "không phải file ZIP");
            return Err(format!("Cổng không trả file XML hợp lệ: {msg}"));
        }
        Ok(bytes.to_vec())
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
        let bytes = resp.bytes().await.unwrap_or_default();
        // Xem read_json_body: decode UTF-8 từ byte thô, không theo charset khai báo.
        let text = match std::str::from_utf8(&bytes) {
            Ok(s) => s.to_string(),
            Err(_) => String::from_utf8_lossy(&bytes).into_owned(),
        };
        if !status.is_success() {
            let msg = serde_json::from_str::<serde_json::Value>(&text)
                .map(|v| portal_message(&v, "không rõ nguyên nhân"))
                .unwrap_or_else(|_| {
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
    pub(crate) async fn login_with_solver<S: crate::hddt::CaptchaSolver>(
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
    use crate::hddt::xml::XmlInvoiceKey;

    /// Dựng khóa định danh hóa đơn để test endpoint `export-xml`.
    fn xml_key(
        direction: InvoiceDirection,
        kind: InvoiceKind,
        nbmst: &str,
        khhdon: &str,
        shdon: &str,
        khmshdon: i64,
    ) -> XmlInvoiceKey {
        XmlInvoiceKey {
            direction,
            kind,
            nbmst: nbmst.to_string(),
            khmshdon,
            khhdon: khhdon.to_string(),
            shdon: shdon.to_string(),
        }
    }

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

    // ─── Test offline với mock portal (không cần mạng/credentials) ───

    #[tokio::test]
    async fn query_invoices_against_mock_uses_right_endpoint_and_headers() {
        let portal = crate::hddt::mock_portal::MockPortal::start().await;
        let c = HddtClient::for_test(&portal.base);
        let list = c
            .query_invoices(
                "mock-token",
                &InvoiceQuery {
                    direction: InvoiceDirection::Purchase,
                    kind: InvoiceKind::Regular,
                    size: 100,
                    from: Some("01/09/2026".into()),
                    to: Some("24/09/2026".into()),
                    ttxly: Some("-1".into()),
                    ..Default::default()
                },
            )
            .await
            .expect("tra cứu mock thất bại");

        assert_eq!(list.total, 1);
        let reqs = portal.requests();
        let r = &reqs[0];
        assert_eq!(r.path, "/api/query/invoices/purchase");
        assert_eq!(
            r.param("size").as_deref(),
            Some("50"),
            "size phải clamp ≤ 50"
        );
        assert_eq!(
            r.headers.get("authorization").map(String::as_str),
            Some("Bearer mock-token")
        );
        assert_eq!(
            r.headers.get("end-point").map(String::as_str),
            Some(ENDPOINT_LOOKUP)
        );
        assert!(
            r.headers.contains_key("action"),
            "endpoint tra cứu cần header Action"
        );
        assert!(
            r.headers.contains_key("request-id"),
            "thiếu anti-bot request-id"
        );
    }

    #[tokio::test]
    async fn cash_register_uses_sco_query_prefix() {
        let portal = crate::hddt::mock_portal::MockPortal::start().await;
        let c = HddtClient::for_test(&portal.base);
        c.query_invoices(
            "mock-token",
            &InvoiceQuery {
                direction: InvoiceDirection::Sold,
                kind: InvoiceKind::CashRegister,
                size: 15,
                ..Default::default()
            },
        )
        .await
        .expect("tra cứu mock thất bại");
        assert_eq!(portal.requests()[0].path, "/api/sco-query/invoices/sold");
    }

    #[tokio::test]
    async fn invoice_detail_needs_numeric_form_code() {
        // Cổng thật trả 500 khi khmshdon không phải số (lỗi đã gặp 25/09/2026:
        // gửi nhầm ký hiệu hóa đơn "C26THN"). Mock giữ nguyên hành vi này.
        let portal = crate::hddt::mock_portal::MockPortal::start().await;
        let c = HddtClient::for_test(&portal.base);

        let err = c
            .invoice_detail("mock-token", "0100000000", "C26THN", "C26MOCK", "0001", "")
            .await
            .expect_err("khmshdon chữ phải bị từ chối");
        assert!(err.contains("NumberFormatException"), "lỗi sai: {err}");

        let ok = c
            .invoice_detail("mock-token", "0100000000", "1", "C26MOCK", "0001", "")
            .await
            .expect("khmshdon số phải lấy được chi tiết");
        let lines = ok
            .get("hdhhdvu")
            .and_then(|v| v.as_array())
            .expect("có dòng hàng");
        assert_eq!(lines.len(), 1);
    }

    #[tokio::test]
    async fn rate_limit_is_retried() {
        let portal = crate::hddt::mock_portal::MockPortal::start().await;
        *portal.rate_limit_first.lock().unwrap() = 2;
        let c = HddtClient::for_test(&portal.base);
        let list = c
            .query_invoices(
                "mock-token",
                &InvoiceQuery {
                    direction: InvoiceDirection::Purchase,
                    kind: InvoiceKind::Regular,
                    size: 15,
                    ..Default::default()
                },
            )
            .await
            .expect("phải thử lại sau 429 rồi thành công");
        assert_eq!(list.total, 1);
        assert_eq!(
            portal.count("/invoices/purchase"),
            3,
            "2 lần 429 + 1 lần thành công"
        );
    }

    #[tokio::test]
    async fn export_xml_calls_export_endpoint_with_identity_params() {
        // Endpoint + 4 tham số phải khớp bundle portal 10/2026 — sai một tham số
        // là cổng trả lỗi JSON, ZIP không có nên file XML không lưu được.
        let portal = crate::hddt::mock_portal::MockPortal::start().await;
        let c = HddtClient::for_test(&portal.base);

        let zip = c
            .export_invoice_xml(
                "mock-token",
                &xml_key(
                    InvoiceDirection::Purchase,
                    InvoiceKind::Regular,
                    "0100000000",
                    "C26MOCK",
                    "0001",
                    1,
                ),
            )
            .await
            .expect("phải tải được ZIP");

        assert_eq!(&zip[..4], b"PK\x03\x04", "phải là ZIP binary thật");

        let req = portal
            .requests()
            .into_iter()
            .find(|r| r.path.ends_with("/invoices/export-xml"))
            .expect("phải gọi /invoices/export-xml");
        assert!(
            req.path.contains("/api/query/"),
            "tiền tố HĐĐT: {}",
            req.path
        );
        assert_eq!(req.param("nbmst").as_deref(), Some("0100000000"));
        assert_eq!(req.param("khhdon").as_deref(), Some("C26MOCK"));
        assert_eq!(req.param("shdon").as_deref(), Some("0001"));
        assert_eq!(req.param("khmshdon").as_deref(), Some("1"));
    }

    #[tokio::test]
    async fn export_xml_uses_sco_query_for_cash_register() {
        // Máy tính tiền phải đi tiền tố `sco-query` — nhầm là cổng 404, không
        // có file XML cho HĐ máy tính tiền.
        let portal = crate::hddt::mock_portal::MockPortal::start().await;
        let c = HddtClient::for_test(&portal.base);
        c.export_invoice_xml(
            "mock-token",
            &xml_key(
                InvoiceDirection::Purchase,
                InvoiceKind::CashRegister,
                "0100000000",
                "C26MOCK",
                "0001",
                1,
            ),
        )
        .await
        .expect("phải tải được ZIP");
        let req = portal
            .requests()
            .into_iter()
            .find(|r| r.path.ends_with("/invoices/export-xml"))
            .expect("phải gọi export-xml");
        assert!(
            req.path.contains("/api/sco-query/"),
            "phải là sco-query: {}",
            req.path
        );
    }

    #[tokio::test]
    async fn detail_returns_every_line_of_the_invoice() {
        let portal = crate::hddt::mock_portal::MockPortal::start().await;
        portal.set_detail_lines(
            r#"[{"ten":"A","dvtinh":"Cái","mhhdvu":"a","sluong":1,"dgia":10,"stckhau":0,"tsuat":0.08},
                {"ten":"B","dvtinh":"Bộ","mhhdvu":"b","sluong":2,"dgia":20,"stckhau":0,"tsuat":0.08}]"#,
        );
        let c = HddtClient::for_test(&portal.base);
        let detail = c
            .invoice_detail("mock-token", "0100000000", "1", "C26MOCK", "0001", "")
            .await
            .unwrap();
        let lines = detail.get("hdhhdvu").and_then(|v| v.as_array()).unwrap();
        assert_eq!(
            lines.len(),
            2,
            "mọi dòng hàng phải được lấy về để tạo phiếu nhập"
        );
    }

    #[tokio::test]
    async fn network_error_is_retried_then_reported() {
        // Lỗi mạng giữa chừng (timeout/đứt kết nối) phải được thử lại — nếu không,
        // một lần rớt mạng sẽ đẩy hóa đơn sang "Cần xử lý" dù cổng vẫn bình thường.
        let portal = crate::hddt::mock_portal::MockPortal::start().await;
        let c = HddtClient::for_test(&portal.base);

        // 2 lần đứt kết nối rồi mới thành công → vẫn trả kết quả.
        *portal.transport_fail_first.lock().unwrap() = 2;
        let list = c
            .query_invoices(
                "mock-token",
                &InvoiceQuery {
                    direction: InvoiceDirection::Purchase,
                    kind: InvoiceKind::Regular,
                    size: 15,
                    ..Default::default()
                },
            )
            .await
            .expect("phải thử lại sau lỗi mạng");
        assert_eq!(list.total, 1);
        assert_eq!(portal.count("/invoices/purchase"), 3);

        // Hết số lần thử → lỗi phải nói rõ đã thử mấy lần.
        *portal.transport_fail_first.lock().unwrap() = 99;
        let err = c
            .query_invoices(
                "mock-token",
                &InvoiceQuery {
                    direction: InvoiceDirection::Purchase,
                    kind: InvoiceKind::Regular,
                    size: 15,
                    ..Default::default()
                },
            )
            .await
            .expect_err("hết lượt thử phải báo lỗi");
        assert!(err.contains("đã thử 4 lần"), "thông báo sai: {err}");
    }

    #[tokio::test]
    async fn login_flow_against_mock() {
        let portal = crate::hddt::mock_portal::MockPortal::start().await;
        let c = HddtClient::for_test(&portal.base);
        let cap = c.fetch_captcha().await.expect("captcha mock");
        let token = c
            .authenticate(&cap.key, "ABCDEF")
            .await
            .expect("đăng nhập mock");
        assert_eq!(token, "mock-token");
        let profile = c.profile(&token).await.expect("hồ sơ mock");
        assert_eq!(
            profile.get("username").and_then(|v| v.as_str()),
            Some("0100000000")
        );
    }

    #[test]
    fn page_size_never_exceeds_portal_limit() {
        // Cổng trả HTTP 500 "findInvoicePurchase.size: phải nhỏ hơn hoặc bằng 50"
        // khi size > 50 → client phải clamp trước khi gửi.
        assert_eq!(page_size_param(100), "50");
        assert_eq!(page_size_param(51), "50");
        assert_eq!(page_size_param(50), "50");
        assert_eq!(page_size_param(15), "15");
        assert_eq!(page_size_param(0), "1");
    }

    #[test]
    fn error_body_decoded_as_utf8() {
        // Byte thô của body lỗi 500 bắt live 25/09/2026 (UTF-8, dù cổng có lúc
        // khai Content-Type với charset khác) → message phải đúng dấu.
        let raw = r#"{"timestamp":"25/09/2026 01:18:41","message":"findInvoicePurchase.size: phải nhỏ hơn hoặc bằng 50","details":"","path":"uri=/invoices/purchase"}"#;
        let v = decode_body(raw.as_bytes()).expect("body phải parse được");
        assert_eq!(
            portal_message(&v, "không rõ"),
            "findInvoicePurchase.size: phải nhỏ hơn hoặc bằng 50"
        );
    }

    #[test]
    fn latin1_mojibake_would_fail_without_utf8_decode() {
        // Decode byte UTF-8 theo charset ISO-8859-1 sẽ ra chuỗi khác hẳn.
        let utf8 = "phải nhỏ hơn hoặc bằng 50";
        let mojibake: String = utf8.as_bytes().iter().map(|&b| b as char).collect();
        assert_ne!(mojibake, utf8, "bộ mojibake không được khớp chuỗi gốc");
        assert!(!utf8.contains(mojibake.as_str()));
        // Byte UTF-8 thật vẫn phải ra đúng chuỗi có dấu.
        let raw = format!(r#"{{"message":"{utf8}"}}"#);
        let v = decode_body(raw.as_bytes()).expect("parse");
        assert_eq!(portal_message(&v, ""), utf8);
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
