//! Định dạng số/ngày, escape HTML và trích `CN` khỏi Subject chữ ký số —
//! port 1-1 từ `render-invoice-html.ts` (`escapeValue`, `formatVND`,
//! `formatVietnameseDate`) và `capture.js` (`Vn`).

use regex::Regex;
use serde_json::Value;
use std::sync::LazyLock;

/// `khai=` — khớp với `[^,\s=]+=` của JS (Rust regex không có lookahead nên
/// giữ luôn dấu `=` rồi cắt ở [`key_name`]).
static KEY_EQ: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^,\s=]+=").unwrap());

/// Tách giá trị DN — khớp với `/,*\s*[^,\s=]+=/` của JS.
static SPLIT_DN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r",*\s*[^,\s=]+=").unwrap());

/// Escape giá trị chèn vào HTML (bản TS: `ESCAPES`).
pub(crate) fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// `String(v)` của JavaScript cho giá trị JSON (thiếu trường → chuỗi rỗng).
pub(crate) fn js_string(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => js_number_str(n.as_f64()),
        // `String([...])` / `String({...})` trong JS là JSON; template không dùng
        // tới các trường này nên chỉ cần không ném.
        _ => v.to_string(),
    }
}

/// `Number(v)` của JavaScript — `null`/sai kiểu → `None` (JS cho ra NaN).
pub(crate) fn js_number(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

/// In số theo cách JS in `String(2.0)` → `"2"` (không có `.0` như Debug của Rust).
fn js_number_str(v: Option<f64>) -> String {
    match v {
        None => String::new(),
        Some(f) if !f.is_finite() => "NaN".to_string(),
        Some(f) if f.fract() == 0.0 && f.abs() < 9.0e15 => format!("{}", f as i64),
        Some(f) => format!("{f}"),
    }
}

/// `Intl.NumberFormat("vi-VN").format(v)` — nhóm `.` mỗi 3 chữ số, dấu phẩy
/// thập phân tối đa 3 chữ số (bỏ số 0 thừa). Không phải số → `"0"`.
pub(crate) fn format_vnd(v: &Value) -> String {
    let Some(f) = js_number(v).filter(|f| f.is_finite()) else {
        return "0".to_string();
    };

    let mut num = format!("{f:.3}");
    if num.contains('.') {
        while num.ends_with('0') {
            num.pop();
        }
        if num.ends_with('.') {
            num.pop();
        }
    }
    // Gom phần nguyên theo cụm 3.
    let (int_part, frac_part) = match num.split_once('.') {
        Some((i, f)) => (i.to_string(), Some(f.to_string())),
        None => (num.clone(), None),
    };
    let negative = int_part.starts_with('-');
    let trimmed = int_part.trim_start_matches('-');
    // `-0.0001` thành rỗng sau khi bỏ dấu thập phân → coi như "0".
    let digits = if trimmed.is_empty() { "0" } else { trimmed };
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            grouped.push('.');
        }
        grouped.push(c);
    }
    if negative {
        grouped.insert(0, '-');
    }
    match frac_part {
        Some(f) if !f.is_empty() => format!("{grouped},{f}"),
        _ => grouped,
    }
}

/// `formatVietnameseDate` — dùng mốc UTC như `Date#getUTC*` của JS.
pub(crate) fn format_vietnamese_date(v: &Value) -> String {
    let raw = js_string(v);
    let raw = raw.trim();
    if raw.is_empty() {
        return String::new();
    }
    let Some((y, m, d)) = parse_utc_date(raw) else {
        return String::new();
    };
    format!("Ngày {d:02} tháng {m:02} năm {y}")
}

/// Nhận cả `2026-09-07T05:03:36Z` (cổng HĐĐT) lẫn `2026-09-07`.
fn parse_utc_date(raw: &str) -> Option<(i32, u32, u32)> {
    use chrono::{Datelike, NaiveDate, Utc};

    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(raw) {
        let dt = dt.with_timezone(&Utc);
        return Some((dt.year(), dt.month(), dt.day()));
    }
    // Không kèm giờ (hoặc giờ không có múi): bỏ phần giờ rồi đọc ngày.
    let day_part = raw.split(['T', ' ']).next().unwrap_or("");
    let date = NaiveDate::parse_from_str(day_part, "%Y-%m-%d").ok()?;
    Some((date.year(), date.month(), date.day()))
}

/// Chữ ký số (`nbcks` là chuỗi JSON) — thiếu/hỏng thì coi như chưa ký.
pub(crate) fn parse_signature(value: &Value) -> Signature {
    let raw = js_string(value);
    if raw.is_empty() {
        return Signature::default();
    }
    let Ok(parsed) = serde_json::from_str::<Value>(&raw) else {
        return Signature::default();
    };
    match parsed {
        Value::Object(map) => Signature {
            subject: map.get("Subject").map(js_string).unwrap_or_default(),
            signing_time: map.get("SigningTime").map(js_string).unwrap_or_default(),
        },
        _ => Signature::default(),
    }
}

/// Hai trường mà khối "Signature Valid" hiển thị.
#[derive(Debug, Default, Clone)]
pub(crate) struct Signature {
    pub subject: String,
    pub signing_time: String,
}

impl Signature {
    /// JS: `Boolean(signature.Subject || signature.SigningTime)` — chuỗi rỗng là falsy.
    pub(crate) fn is_signed(&self) -> bool {
        !self.subject.is_empty() || !self.signing_time.is_empty()
    }
}

/// Lấy giá trị của khoá `CN` trong chuỗi DN (`capture.js` → `Vn`).
///
/// JS làm: xoá `khoá=` đầu tiên, cắt chuỗi còn lại theo `,*\s*khoá=`, rồi ghép
/// khoá ↔ giá trị theo vị trí. Không có khoá nào (không có dấu `=`) thì JS ném
/// lỗi *trước* khi gán lại `innerText` → giữ nguyên Subject.
pub(crate) fn extract_cn(subject: &str) -> String {
    let keys: Vec<String> = KEY_EQ
        .find_iter(subject)
        .map(|m| m.as_str()[..m.as_str().len() - 1].to_string())
        .collect();
    if keys.is_empty() {
        return subject.to_string();
    }

    let trimmed = subject.trim();
    let without_first = KEY_EQ.replace(trimmed, "");
    let values: Vec<&str> = SPLIT_DN.split(&without_first).collect();

    for (i, key) in keys.iter().enumerate() {
        if key == "CN" {
            return values.get(i).copied().unwrap_or("").to_string();
        }
    }
    String::new()
}
