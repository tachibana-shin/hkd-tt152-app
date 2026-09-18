#![allow(unused_imports)]

// Tra cứu thông tin doanh nghiệp / hộ kinh doanh theo mã số thuế
// qua masothue.com (endpoint /Ajax/Token + /Ajax/Search, đã reverse-engineer
// và verify bằng curl + browser 09/2026). Chạy ở backend để tránh CORS/Cloudflare.

use crate::helpers::require_role;
use crate::models::AppState;
use reqwest::cookie::Jar;
use reqwest::header::{ORIGIN, REFERER};
use regex::Regex;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tauri::State;

const BASE: &str = "https://masothue.com";
const UA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/152.0.0.0 Safari/537.36";

#[derive(Serialize)]
pub(crate) struct TaxInfo {
    pub name: Option<String>,
    pub tax_code: Option<String>,
    pub address: Option<String>,
    pub representative: Option<String>,
    pub status: Option<String>,
    pub phone: Option<String>,
    pub company_type: Option<String>,
    pub managed_by: Option<String>,
    pub issued_on: Option<String>,
    pub international_name: Option<String>,
    pub short_name: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct TaxResultItem {
    pub mst: String,
    pub name: String,
    pub url: String,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum LookupOutcome {
    Single { info: TaxInfo },
    Multiple { results: Vec<TaxResultItem> },
    NotFound { message: String },
}

// ─── HTTP helpers ───

fn build_client() -> Result<reqwest::Client, String> {
    let jar = Jar::default();
    let base_url: reqwest::Url = BASE.parse::<reqwest::Url>().map_err(|e| e.to_string())?;
    // 3 cookie do JS trang chủ set — cần thiết để server nhận diện "trình duyệt thật"
    jar.add_cookie_str("hm=1; Path=/; Domain=.masothue.com", &base_url);
    jar.add_cookie_str("res=1366x768; Path=/; Domain=.masothue.com", &base_url);
    jar.add_cookie_str("c_code_name=VN; Path=/; Domain=.masothue.com", &base_url);
    reqwest::Client::builder()
        .user_agent(UA)
        .cookie_provider(Arc::new(jar))
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())
}

/// Tạo session + lấy token chống bot: GET trang chủ rồi POST /Ajax/Token.
async fn fetch_token(client: &reqwest::Client) -> Result<String, String> {
    client
        .get(format!("{BASE}/"))
        .send()
        .await
        .map_err(|e| format!("Lỗi kết nối masothue: {e}"))?;
    let resp = client
        .post(format!("{BASE}/Ajax/Token"))
        .header("X-Requested-With", "XMLHttpRequest")
        .form(&[("r", "abc")])
        .send()
        .await
        .map_err(|e| format!("Lỗi lấy token: {e}"))?;
    let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    v.get("token")
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "Không lấy được token từ masothue — có thể tạm bị chặn.".into())
}

async fn ajax_search(
    client: &reqwest::Client,
    mst: &str,
    token: &str,
) -> Result<serde_json::Value, String> {
    let resp = client
        .post(format!("{BASE}/Ajax/Search"))
        .header("X-Requested-With", "XMLHttpRequest")
        .header(ORIGIN, BASE)
        .header(REFERER, format!("{BASE}/"))
        .form(&[
            ("q", mst),
            ("type", "auto"),
            ("token", token),
            ("force-search", "0"),
        ])
        .send()
        .await
        .map_err(|e| format!("Lỗi tra cứu: {e}"))?;
    resp.json().await.map_err(|e| e.to_string())
}

fn urlenc(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

// ─── Parse HTML ───

fn clean_text(s: &str) -> String {
    let re_tag = Regex::new(r"<[^>]+>").unwrap();
    let re_ws = Regex::new(r"\s+").unwrap();
    let t = re_ws
        .replace_all(&re_tag.replace_all(s, " "), " ")
        .to_string();
    let t = t.trim().to_string();
    t.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
}

/// Lấy giá trị cell kế bên label trong `<table class="table-taxinfo">`.
fn label_value(html: &str, label: &str) -> Option<String> {
    let pat = format!(r"{}\s*</td>\s*<td[^>]*>(.*?)</td>", regex::escape(label));
    let re = Regex::new(&pat).ok()?;
    let caps = re.captures(html)?;
    let v = clean_text(&caps[1]);
    if v.is_empty() {
        None
    } else {
        Some(v)
    }
}

/// Tên hộ/doanh nghiệp từ `<h1>MST - TÊN</h1>`, fallback từ slug URL.
fn parse_name(html: &str, url: &str) -> String {
    let re = Regex::new(r"(?s)<h1[^>]*>\s*\d{9,13}\s*-\s*(.*?)</h1>").unwrap();
    if let Some(c) = re.captures(html) {
        let n = clean_text(&c[1]);
        if !n.is_empty() {
            return n;
        }
    }
    url.split('/')
        .next_back()
        .map(|s| {
            s.split('-')
                .skip(1)
                .collect::<Vec<_>>()
                .join(" ")
                .to_uppercase()
        })
        .unwrap_or_default()
}

fn parse_detail(html: &str, url: &str) -> TaxInfo {
    let re_rep = Regex::new(r#"<span itemprop='name'>(?:[^<]*<a[^>]*>)?([^<]+)"#).unwrap();
    let representative = re_rep
        .captures(html)
        .map(|c| clean_text(&c[1]))
        .or_else(|| {
            label_value(html, "Người đại diện").map(|v| {
                v.split("Ngoài ra")
                    .next()
                    .unwrap_or(&v)
                    .trim()
                    .to_string()
            })
        });
    TaxInfo {
        name: Some(parse_name(html, url)),
        tax_code: label_value(html, "Mã số thuế"),
        address: label_value(html, "Địa chỉ Thuế").or_else(|| label_value(html, "Địa chỉ")),
        representative,
        status: label_value(html, "Tình trạng"),
        phone: label_value(html, "Điện thoại"),
        company_type: label_value(html, "Loại hình DN"),
        managed_by: label_value(html, "Quản lý bởi"),
        issued_on: label_value(html, "Ngày hoạt động"),
        international_name: label_value(html, "Tên quốc tế"),
        short_name: label_value(html, "Tên viết tắt"),
    }
}

fn parse_listing(html: &str) -> Vec<TaxResultItem> {
    let mut out: Vec<TaxResultItem> = Vec::new();
    let re = Regex::new(r"<h3><a href='([^']+)' title='[^']*'>([^<]+)</a></h3>").unwrap();
    for cap in re.captures_iter(html) {
        let path = cap[1].to_string();
        let name = clean_text(&cap[2]);
        if let Some(mst) = slug_mst(&path) {
            out.push(TaxResultItem { mst, name, url: path });
        }
    }
    // Fallback: block `data-prefetch='...'` (tên nằm trong <a> kế bên)
    if out.is_empty() {
        let re2 = Regex::new(r"data-prefetch='([^']+)'").unwrap();
        let mut seen: HashSet<String> = HashSet::new();
        for cap in re2.captures_iter(html) {
            let path = cap[1].to_string();
            if !seen.insert(path.clone()) {
                continue;
            }
            let start = cap.get(1).unwrap().start();
            let window = &html[start..(start + 600).min(html.len())];
            let name = Regex::new(r"<a[^>]*>([^<]{2,120})</a>")
                .unwrap()
                .captures(window)
                .map(|c| clean_text(&c[1]))
                .unwrap_or_default();
            let mst = slug_mst(&path).unwrap_or_default();
            if !name.is_empty() {
                out.push(TaxResultItem { mst, name, url: path });
            }
        }
    }
    out
}

fn slug_mst(path: &str) -> Option<String> {
    let seg = path.split('/').nth(1)?;
    let mst = seg.split('-').next().unwrap_or("");
    if !mst.is_empty() && mst.chars().all(|c| c.is_ascii_digit()) {
        Some(mst.to_string())
    } else {
        None
    }
}

fn type_for(type_id: i64) -> &'static str {
    match type_id {
        1 | 6 => "enterpriseTax",
        3 => "identity",
        4 => "enterpriseName",
        8 => "personalTax",
        _ => "auto",
    }
}

// ─── Commands ───

/// Tra cứu theo mã số thuế → 1 kết quả (Single), nhiều kết quả (Multiple)
/// hoặc không tìm thấy (NotFound). Trả về JSON chuỗi (theo convention app).
#[tauri::command]
pub(crate) async fn lookup_tax_code(
    state: State<'_, AppState>,
    mst: String,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan", "kho"]).await?;
    let mst = mst.trim().to_string();
    if mst.is_empty() {
        return Ok(json_outcome(LookupOutcome::NotFound {
            message: "Vui lòng nhập mã số thuế.".into(),
        }));
    }
    let client = build_client()?;
    let mut attempts = 0;
    loop {
        attempts += 1;
        let token = fetch_token(&client).await?;
        let v = ajax_search(&client, &mst, &token).await?;
        if let Some(outcome) = build_outcome(&client, &v, &mst, &token).await {
            return Ok(json_outcome(outcome));
        }
        if attempts >= 2 {
            return Err("Tra cứu không thành công — vui lòng thử lại sau ít phút.".into());
        }
    }
}

/// Lấy chi tiết từ URL trang masothue (dùng khi người dùng chọn trong danh sách).
#[tauri::command]
pub(crate) async fn lookup_tax_detail(
    state: State<'_, AppState>,
    url: String,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan", "kho"]).await?;
    let target = if url.starts_with("http") {
        url
    } else {
        format!("{BASE}{url}")
    };
    let client = build_client()?;
    let html = client
        .get(&target)
        .send()
        .await
        .map_err(|e| format!("Lỗi tải trang chi tiết: {e}"))?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    Ok(serde_json::to_string(&parse_detail(&html, &target)).unwrap_or_default())
}

fn json_outcome(o: LookupOutcome) -> String {
    serde_json::to_string(&o).unwrap_or_default()
}

async fn build_outcome(
    client: &reqwest::Client,
    v: &serde_json::Value,
    mst: &str,
    token: &str,
) -> Option<LookupOutcome> {
    if v.get("success").and_then(|s| s.as_i64()) != Some(1) {
        let message = v
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("Không tìm thấy kết quả.")
            .to_string();
        return Some(LookupOutcome::NotFound { message });
    }

    // Nhiều kết quả → trang listing /Search/?type=...
    if v.get("mutilResult").and_then(|m| m.as_i64()) == Some(1) {
        let type_id = v.get("typeId").and_then(|t| t.as_i64()).unwrap_or(0);
        let list_url = format!(
            "{BASE}/Search/?type={}&q={}&token={}&force-search=0",
            type_for(type_id),
            urlenc(mst),
            token
        );
        let html = client
            .get(&list_url)
            .header(REFERER, format!("{BASE}/"))
            .send()
            .await
            .ok()?
            .text()
            .await
            .ok()?;
        let results = parse_listing(&html);
        if results.is_empty() {
            return Some(LookupOutcome::NotFound {
                message: format!("Không tìm thấy thông tin cho mã số thuế {mst}."),
            });
        }
        return Some(LookupOutcome::Multiple { results });
    }

    let u = match v.get("url").and_then(|u| u.as_str()) {
        Some(u) if u != "/" => u,
        _ => {
            return Some(LookupOutcome::NotFound {
                message: format!("Không tìm thấy thông tin cho mã số thuế {mst}."),
            })
        }
    };
    let target = if u.starts_with("http") {
        u.to_string()
    } else {
        format!("{BASE}{u}")
    };
    let html = client.get(&target).send().await.ok()?.text().await.ok()?;
    Some(LookupOutcome::Single {
        info: parse_detail(&html, &target),
    })
}

// ─── Tests ───

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_text() {
        assert_eq!(
            clean_text("<span class='copy'>036186003184</span>"),
            "036186003184"
        );
        assert_eq!(
            clean_text("<a href='x'>Đang&amp;hoạt động</a> <br />ok"),
            "Đang&hoạt động ok"
        );
    }

    #[test]
    fn test_label_value() {
        let html = "<table class=\"table-taxinfo\">
            <tr><td><i class='fa fa-hashtag'></i> Mã số thuế</td>
            <td itemprop='taxID'><span class='copy'>036186003184</span></td></tr>
            <tr><td><i class='fa fa-map-marker'></i> Địa chỉ Thuế</td>
            <td itemprop='address'><span class='copy' id='tax-address-html'>Thửa đất số 187, Quốc lộ 10, Phường Nam Định, Tỉnh Ninh Bình, Việt Nam</span></td></tr>
        </table>";
        assert_eq!(
            label_value(html, "Mã số thuế").unwrap(),
            "036186003184"
        );
        assert_eq!(
            label_value(html, "Địa chỉ Thuế").unwrap(),
            "Thửa đất số 187, Quốc lộ 10, Phường Nam Định, Tỉnh Ninh Bình, Việt Nam"
        );
    }

    #[test]
    fn test_parse_name() {
        let html = "<h1>036186003184 - HỘ KINH DOANH HOTEL 19</h1>";
        assert_eq!(
            parse_name(html, "https://masothue.com/036186003184-ho-kinh-doanh-hotel-19"),
            "HỘ KINH DOANH HOTEL 19"
        );
    }

    #[test]
    fn test_parse_listing() {
        let html = "<div data-prefetch='/036186003184-ho-kinh-doanh-hotel-19'>
            <h3><a href='/036186003184-ho-kinh-doanh-hotel-19' title='Tra cứu mã số thuế 036186003184 HỘ KINH DOANH HOTEL 19'>HỘ KINH DOANH HOTEL 19</a></h3>
            <i class='fa fa-hashtag'></i> Mã số thuế: <a href='/036186003184-ho-kinh-doanh-hotel-19' title='Tra cứu mã số thuế 036186003184'>036186003184</a><br />
        </div>
        <div data-prefetch='/8623505476-nguyen-thi-mai-huong'>
            <h3><a href='/8623505476-nguyen-thi-mai-huong' title='Tra cứu mã số thuế 8623505476 NGUYỄN THỊ MAI HƯƠNG'>NGUYỄN THỊ MAI HƯƠNG</a></h3>
        </div>";
        let items = parse_listing(html);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].mst, "036186003184");
        assert_eq!(items[0].name, "HỘ KINH DOANH HOTEL 19");
        assert_eq!(items[0].url, "/036186003184-ho-kinh-doanh-hotel-19");
        assert_eq!(items[1].name, "NGUYỄN THỊ MAI HƯƠNG");
    }
}