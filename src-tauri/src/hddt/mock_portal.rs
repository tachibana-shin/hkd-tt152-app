//! Mock cổng HĐĐT cho test offline.
//!
//! Chỉ dựng các endpoint mà `HddtClient` dùng (captcha, đăng nhập, hồ sơ,
//! danh sách hóa đơn, chi tiết hóa đơn) trả payload JSON giống hình dạng thật
//! của cổng. Nhờ đó test không cần mạng, không cần credentials và chạy ổn định;
//! muốn kiểm chứng với cổng thật thì dùng các test `#[ignore]` trong
//! `hddt/live.rs`.
//!
//! Không thêm dependency: server là TCP thuần (mỗi request một connection).

#![cfg(test)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Một request mà mock đã nhận (để assert client gọi đúng URL/headers).
#[derive(Debug, Clone)]
pub(crate) struct RecordedRequest {
    pub path: String,
    pub query: String,
    pub headers: HashMap<String, String>,
}

impl RecordedRequest {
    /// Giá trị một query param (vd `size`, `khmshdon`).
    pub fn param(&self, key: &str) -> Option<String> {
        self.query.split('&').find_map(|kv| {
            let (k, v) = kv.split_once('=')?;
            (k == key).then(|| percent_decode(v))
        })
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.replace('+', " ").into_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(b) => {
                        out.push(b);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Server mock + danh sách request đã nhận.
pub(crate) struct MockPortal {
    pub base: String,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    /// Trả 429 `n` lần đầu cho mọi endpoint (test retry).
    pub rate_limit_first: Arc<Mutex<usize>>,
    /// Nội dung `hdhhdvu` trả về cho endpoint chi tiết.
    pub detail_lines: Arc<Mutex<String>>,
}

impl MockPortal {
    /// Dựng server trên cổng ngẫu nhiên; `base` = URL gốc dùng cho `HddtClient`.
    pub async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("mock portal: bind");
        let addr = listener.local_addr().expect("mock portal: addr");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let rate_limit_first = Arc::new(Mutex::new(0));
        let detail_lines = Arc::new(Mutex::new(
            r#"[{"ten":"Hàng mẫu","dvtinh":"Cái","mhhdvu":"SP-MOCK","sluong":2.0,"dgia":100000.0,"stckhau":0.0,"tsuat":0.08}]"#
                .to_string(),
        ));
        let srv = Self {
            base: format!("http://{addr}"),
            requests: requests.clone(),
            rate_limit_first: rate_limit_first.clone(),
            detail_lines: detail_lines.clone(),
        };
        tokio::spawn(async move {
            loop {
                let Ok((socket, _)) = listener.accept().await else {
                    return;
                };
                let reqs = requests.clone();
                let rl = rate_limit_first.clone();
                let lines = detail_lines.clone();
                tokio::spawn(async move {
                    let _ = handle(socket, reqs, rl, lines).await;
                });
            }
        });
        srv
    }

    pub fn requests(&self) -> Vec<RecordedRequest> {
        self.requests.lock().unwrap().clone()
    }

    /// Số request có path chứa `needle` (vd "invoices/purchase").
    pub fn count(&self, needle: &str) -> usize {
        self.requests()
            .iter()
            .filter(|r| r.path.contains(needle))
            .count()
    }

    pub fn reset(&self) {
        self.requests.lock().unwrap().clear();
    }

    pub fn set_detail_lines(&self, lines: &str) {
        *self.detail_lines.lock().unwrap() = lines.to_string();
    }
}

async fn handle(
    mut socket: tokio::net::TcpStream,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    rate_limit_first: Arc<Mutex<usize>>,
    detail_lines: Arc<Mutex<String>>,
) -> std::io::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut buf = vec![0u8; 16 * 1024];
    let n = socket.read(&mut buf).await?;
    let raw = String::from_utf8_lossy(&buf[..n]).to_string();
    let mut lines = raw.lines();
    let start = lines.next().unwrap_or_default();
    let mut parts = start.split_whitespace();
    let _method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or("/").to_string();
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target, String::new()),
    };
    let mut headers = HashMap::new();
    for line in lines {
        if line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }
    requests.lock().unwrap().push(RecordedRequest {
        path: path.clone(),
        query: query.clone(),
        headers,
    });

    // 429 cho N lần đầu nếu được cấu hình (trả về số lần 429 còn lại).
    let still_limited = {
        let mut rl = rate_limit_first.lock().unwrap();
        if *rl > 0 {
            *rl -= 1;
            true
        } else {
            false
        }
    };
    if still_limited {
        return write_response(
            &mut socket,
            429,
            "application/json;charset=UTF-8",
            r#"{"message":"Too Many Requests"}"#,
        )
        .await;
    }

    let body = match path.as_str() {
        "/api/captcha" => serde_json::json!({
            "key": "mock-captcha-key",
            "content": r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="40"></svg>"#,
        }),
        "/api/security-taxpayer/authenticate" => serde_json::json!({ "token": "mock-token" }),
        "/api/security-taxpayer/profile" => serde_json::json!({
            "name": "HỘ KINH DOANH MOCK",
            "username": "0100000000",
            "tinInfoTT86": { "mst": "0100000000", "mstUTien": "0100000000" },
        }),
        p if p.ends_with("/invoices/detail") => {
            // Cổng thật yêu cầu khmshdon dạng số, sai là 500 kiểu Spring Boot —
            // mock giữ nguyên hành vi để test bắt được lỗi gửi nhầm ký hiệu HĐ.
            let khmshdon = last_query_param(&query, "khmshdon").unwrap_or_default();
            if khmshdon.is_empty() || khmshdon.parse::<i64>().is_err() {
                let body = serde_json::json!({
                    "timestamp": "25/09/2026 01:18:41",
                    "message": format!(
                        "Failed to convert value of type 'java.lang.String' to required type \
                         'byte'; nested exception is java.lang.NumberFormatException: \
                         For input string: \"{khmshdon}\""
                    ),
                    "details": "",
                    "path": "uri=/invoices/detail",
                })
                .to_string();
                return write_response(&mut socket, 500, "application/json", &body).await;
            }
            let lines = detail_lines.lock().unwrap().clone();
            serde_json::json!({ "hdhhdvu": serde_json::from_str::<serde_json::Value>(&lines)
                .map(|v| v.as_array().cloned().unwrap_or_default())
                .unwrap_or_default() })
        }
        p if p.ends_with("/invoices/sold") || p.ends_with("/invoices/purchase") => {
            serde_json::json!({
                "datas": [mock_invoice_row()],
                "state": "",
                "total": 1,
                "time": 12,
            })
        }
        _ => serde_json::json!({ "message": "not found" }),
    };
    let status = if path == "/api/nope" { 404 } else { 200 };
    write_response(
        &mut socket,
        status,
        "application/json;charset=UTF-8",
        &body.to_string(),
    )
    .await
}

fn last_query_param(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == key).then(|| percent_decode(v))
    })
}

async fn write_response(
    socket: &mut tokio::net::TcpStream,
    status: u16,
    content_type: &str,
    body: &str,
) -> std::io::Result<()> {
    use tokio::io::AsyncWriteExt;
    let reason = if status == 429 {
        "Too Many Requests"
    } else {
        "OK"
    };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    socket.write_all(head.as_bytes()).await?;
    socket.write_all(body.as_bytes()).await?;
    socket.flush().await
}

/// Một dòng hóa đơn danh sách, đúng tên field của cổng (chỉ giữ field app dùng).
fn mock_invoice_row() -> serde_json::Value {
    serde_json::json!({
        "id": "11111111-2222-3333-4444-555555555555",
        "khmshdon": 1,
        "khhdon": "C26MOCK",
        "shdon": "0001",
        "tdlap": "2026-09-01T10:00:00Z",
        "tthai": 1,
        "ttxly": 5,
        "nbmst": "0100000000",
        "nbten": "CÔNG TY CỔ PHẦN MOCK",
        "nmmst": "0200000000",
        "nmtnmua": "HỘ KINH DOANH MOCK",
        "tgtcthue": 100000.0,
        "tgtthue": 8000.0,
        "ttcktmai": 0.0,
        "tgtttbso": 108000.0,
    })
}
