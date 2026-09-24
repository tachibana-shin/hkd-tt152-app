// Đồng bộ hóa đơn mua vào từ cổng HĐĐT → hóa đơn chính thức + phiếu nhập kho.
//
// Luồng: `scan` (gọi cổng, cache theo ngày) → `preview` (đọc cache, không mạng)
//         → `import` (tạo mặt hàng/NCC/phiếu nhập + liên kết hóa đơn).
//
// Nguyên tắc:
//   * Idempotent — `hddt_purchase_invoice.portal_id` UNIQUE (id của cổng) nên
//     quét lại hay import lại 2 lần đều không tạo trùng.
//   * Ngày đã quét thì cache trong `hddt_sync_day` (hóa đơn ngày đã qua không đổi).
//   * Hôm nay bỏ qua — hóa đơn hôm nay còn có thể phát sinh/điều chỉnh thêm.

use chrono::{Datelike, Duration, NaiveDate};
use serde::Serialize;
use serde_json::Value;
use sqlx::{FromRow, SqlitePool};

use crate::commands::stock::save_inbound_core;
use crate::hddt::{HddtClient, InvoiceKind, InvoiceQuery};
use crate::models::InboundItemInput;

/// Ngày cổng chấp nhận trong 1 request (HTTP 400 nếu vượt: "Khoảng thời gian
/// tìm kiếm không được lớn hơn 1 tháng").
const MAX_WINDOW_DAYS: i64 = 30;

/// Danh tính mặt hàng: `tên` + `đơn vị tính`, chuẩn hoá Unicode.
/// Dùng `\u{1f}` (unit separator) làm dấu phân cách để không nhập nhầm
/// khi tên chứa ký tự lạ.
pub(crate) fn identity_key(name: &str, unit: &str) -> String {
    format!(
        "{}\u{1f}{}",
        name.trim().to_lowercase(),
        unit.trim().to_lowercase()
    )
}

/// Gán `identity_key` cho mọi mặt hàng, chỉ dòng id nhỏ nhất của mỗi nhóm
/// (tên, đơn vị) được gán — hàng trùng sẵn có giữ khoá rỗng để đồng bộ tạo
/// mặt hàng mới thay vì nhập nhầm. Chuẩn hoá bằng Rust (không dùng `lower()`
/// của SQLite vì chỉ hỗ trợ ASCII, lệch với tiếng Việt có dấu).
pub(crate) async fn ensure_product_identity_keys(pool: &SqlitePool) -> Result<(), String> {
    let rows: Vec<(i64, String, String, String)> =
        sqlx::query_as("SELECT id, name, unit, identity_key FROM product")
            .fetch_all(pool)
            .await
            .map_err(|e| e.to_string())?;

    // nhóm (key) -> id nhỏ nhất
    let mut winner: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    for (id, name, unit, _) in &rows {
        if name.trim().is_empty() {
            continue;
        }
        let key = identity_key(name, unit);
        winner
            .entry(key)
            .and_modify(|min| *min = (*min).min(*id))
            .or_insert(*id);
    }

    for (id, name, unit, current) in rows {
        let want = if name.trim().is_empty() {
            String::new()
        } else {
            let key = identity_key(&name, &unit);
            if winner.get(&key) == Some(&id) {
                key
            } else {
                String::new()
            }
        };
        if current != want {
            sqlx::query("UPDATE product SET identity_key = ? WHERE id = ?")
                .bind(&want)
                .bind(id)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// yyyy-mm-dd → dd/MM/yyyy.
fn iso_to_portal_day(s: &str) -> String {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
        .map(|d| d.format("%d/%m/%Y").to_string())
        .unwrap_or_default()
}

/// Tách số từ chuỗi (chịu cả number lẫn string của cổng).
fn as_f64(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(s) => s.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn as_i64(v: &Value) -> i64 {
    match v {
        Value::Number(n) => n.as_i64().unwrap_or(0),
        Value::String(s) => s.trim().parse().unwrap_or(0),
        _ => 0,
    }
}

fn as_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.trim().to_string(),
        Value::Number(n) => n.to_string(),
        Value::Null => String::new(),
        other => other.to_string().trim().to_string(),
    }
}

fn f(v: &Value, key: &str) -> String {
    v.get(key).map(as_str).unwrap_or_default()
}

fn num(v: &Value, key: &str) -> f64 {
    v.get(key).map(as_f64).unwrap_or(0.0)
}

fn int(v: &Value, key: &str) -> i64 {
    v.get(key).map(as_i64).unwrap_or(0)
}

/// Hàng trong DB cần cho việc import, đọc theo id của hóa đơn cache.
#[derive(Debug, FromRow)]
pub(crate) struct CachedInvoice {
    pub(crate) id: i64,
    pub(crate) portal_id: String,
    pub(crate) posting_date: String,
    pub(crate) nbmst: String,
    pub(crate) nbten: String,
    /// Mã mẫu số dạng số (1..9) — endpoint chi tiết BẮT BUỘC đúng field này.
    pub(crate) khmshdon: i64,
    pub(crate) khhdon: String,
    pub(crate) shdon: String,
    pub(crate) status: String,
    pub(crate) detail_json: String,
}

/// Tóm tắt 1 lần quét cổng.
#[derive(Debug, Default, serde::Serialize)]
pub(crate) struct ScanSummary {
    /// Số ngày thực sự gọi cổng (không tính ngày đã cache).
    pub(crate) days_scanned: usize,
    /// Số ngày bỏ qua vì đã quét trước đó.
    pub(crate) days_cached: usize,
    /// Hóa đơn mới ghi vào cache.
    pub(crate) invoices_new: usize,
    /// Hóa đơn đã có trong cache từ trước (không ghi đè).
    pub(crate) invoices_existing: usize,
    /// Hóa đơn lấy được chi tiết thành công (có dòng hàng).
    pub(crate) details_ok: usize,
    /// Hóa đơn không lấy được chi tiết — cần thử lại, KHÔNG tạo phiếu.
    pub(crate) details_failed: usize,
    /// Hóa đơn bị đánh dấu cần người xử lý (thay thế/điều chỉnh/hủy).
    pub(crate) need_manual: usize,
}

/// Quét cổng trong khoảng ngày và ghi cache.
///
/// `from`/`to` = yyyy-mm-dd. Mốc thấp nhất lấy từ `hddt_start_date` (khai
/// báo trong popup cấu hình HKD). Mốc cao nhất là **hôm qua**.
pub(crate) async fn scan(
    pool: &SqlitePool,
    client: &HddtClient,
    token: &str,
    from: &str,
    to: &str,
    kinds: &[InvoiceKind],
) -> Result<ScanSummary, String> {
    let mut sum = ScanSummary::default();

    let start_date = resolve_start(pool, from).await?;
    let end_date = resolve_end(to).await?;
    if start_date > end_date {
        return Err(format!(
            "Khoảng ngày không hợp lệ: {} → {} (mốc bắt đầu HĐĐT của hộ là {})",
            start_date, end_date, start_date
        ));
    }

    // Chỉ quét những ngày CHƯA có cache cho loại HĐ đó: hóa đơn ngày đã qua không
    // đổi nên không cần gọi lại cổng. Ngày hôm nay không bao giờ quét/cache.
    for kind in kinds {
        let cached = cached_days(pool, *kind).await?;
        // `NaiveDate` không có `Iterator::filter` thuận tiện → dựng list ngày trước.
        let mut all_days: Vec<NaiveDate> = Vec::new();
        let mut d = start_date;
        while d <= end_date {
            all_days.push(d);
            d += Duration::days(1);
        }
        let missing: Vec<NaiveDate> = all_days
            .iter()
            .copied()
            .filter(|d| !cached.contains(&d.format("%Y-%m-%d").to_string()))
            .collect();
        sum.days_cached += all_days.len() - missing.len();
        if missing.is_empty() {
            continue;
        }

        // Gom ngày còn thiếu thành cửa sổ liên tiếp ≤ 30 ngày (cổng chặn > 1 tháng).
        let mut i = 0;
        while i < missing.len() {
            let win_start = missing[i];
            let mut j = i;
            // Mở rộng cửa sổ chừng nên các ngày liên tiếp (cache rời rạc vẫn gộp
            // được) và không vượt quá giới hạn cổng.
            while j + 1 < missing.len()
                && (missing[j + 1] - missing[j]).num_days() == 1
                && (missing[j + 1] - win_start).num_days() < MAX_WINDOW_DAYS
            {
                j += 1;
            }
            let win_end = missing[j];
            let days = (win_end - win_start).num_days() + 1;

            let mut state: Option<String> = None;
            loop {
                let q = InvoiceQuery {
                    direction: crate::hddt::InvoiceDirection::Purchase,
                    kind: *kind,
                    // Cổng từ chối size > 50 (HTTP 500) → lấy tối đa 50/lần.
                    size: crate::hddt::MAX_PAGE_SIZE,
                    state: state.clone(),
                    from: Some(iso_to_portal_day(&win_start.to_string())),
                    to: Some(iso_to_portal_day(&win_end.to_string())),
                    // Tất cả kết quả kiểm tra: bỏ lọc để không bỏ sót HĐ.
                    ttxly: Some("-1".into()),
                    ..Default::default()
                };
                let list = client.query_invoices(token, &q).await?;
                let rows = list.datas.len();
                for row in &list.datas {
                    upsert_invoice(pool, row, *kind, sum.need_manual).await?;
                    let portal_id = f(row, "id");
                    if portal_id.is_empty() {
                        continue;
                    }
                    match cache_invoice(pool, &portal_id).await? {
                        CacheState::New => sum.invoices_new += 1,
                        CacheState::Existing => sum.invoices_existing += 1,
                    }
                }
                state = list
                    .state
                    .as_ref()
                    .and_then(|s| s.as_str())
                    .map(str::to_string)
                    .filter(|s| !s.is_empty());
                if state.is_none() || rows == 0 {
                    break;
                }
            }
            // Đánh dấu ngày đã quét cho loại này (kể cả 0 hóa đơn).
            sum.days_scanned += days as usize;
            mark_days_scanned(pool, win_start, win_end, *kind).await?;
            i = j + 1;
        }
    }

    // Lấy chi tiết cho các hóa đơn chưa có (đủ dòng hàng mới tạo được phiếu).
    let pending = pending_without_detail(pool).await?;
    for inv in pending {
        if inv.khmshdon <= 0 {
            // Endpoint chi tiết bắt buộc khmshdon dạng số; thiếu thì không đoán.
            let msg = "Thiếu mã mẫu số (khmshdon) — cần quét lại từ cổng";
            sqlx::query("UPDATE hddt_purchase_invoice SET detail_error = ? WHERE id = ?")
                .bind(msg)
                .bind(inv.id)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
            sum.details_failed += 1;
            continue;
        }
        match client
            .invoice_detail(
                token,
                &inv.nbmst,
                &inv.khmshdon.to_string(),
                &inv.khhdon,
                &inv.shdon,
                &inv.portal_id,
            )
            .await
        {
            Ok(detail) => {
                let lines = detail
                    .get("hdhhdvu")
                    .and_then(Value::as_array)
                    .map(|a| a.len())
                    .unwrap_or(0);
                sqlx::query(
                    "UPDATE hddt_purchase_invoice SET detail_json = ?, line_count = ?, detail_error = '' WHERE id = ?",
                )
                .bind(detail.to_string())
                .bind(lines as i64)
                .bind(inv.id)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
                sum.details_ok += 1;
            }
            Err(msg) => {
                sqlx::query("UPDATE hddt_purchase_invoice SET detail_error = ? WHERE id = ?")
                    .bind(&msg)
                    .bind(inv.id)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                sum.details_failed += 1;
            }
        }
    }

    // Gán cờ cần người xử lý sau khi đã biết chi tiết.
    sum.need_manual = mark_manual_cases(pool).await?;
    Ok(sum)
}

#[derive(PartialEq)]
enum CacheState {
    New,
    Existing,
}

/// Ghi 1 hóa đơn vào cache (ON CONFLICT portal_id DO NOTHING — không đè dữ liệu
/// đã nhập kho).
async fn upsert_invoice(
    pool: &SqlitePool,
    row: &Value,
    kind: InvoiceKind,
    _need_manual: usize,
) -> Result<(), String> {
    let portal_id = f(row, "id");
    if portal_id.is_empty() {
        return Ok(());
    }
    let tdlap = f(row, "tdlap");
    // tdlap của cổng là ISO UTC (vd 2026-09-23T17:00:00Z = 24/09 giờ Việt).
    let posting_date = parse_tdlap(&tdlap);
    let status = if int(row, "tthai") == 1 {
        "pending"
    } else {
        "manual"
    };
    let reason = if int(row, "tthai") == 1 {
        String::new()
    } else {
        format!(
            "Hóa đơn tthai={} — không tự tạo phiếu nhập",
            int(row, "tthai")
        )
    };
    sqlx::query(
        "INSERT INTO hddt_purchase_invoice
            (portal_id, portal_kind, tdlap, posting_date, nbmst, nbten, nmmst,
             khmshdon, khhdon, shdon, hthdon, tchat,
             tgtcthue, tgtthue, ttcktmai, tgtttbso, status, skip_reason,
             raw_json, created_at)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,datetime('now'))
         ON CONFLICT(portal_id) DO NOTHING",
    )
    .bind(&portal_id)
    .bind(kind_str(kind))
    .bind(&tdlap)
    .bind(&posting_date)
    .bind(f(row, "nbmst"))
    .bind(f(row, "nbten"))
    .bind(f(row, "nmmst"))
    .bind(int(row, "khmshdon"))
    .bind(f(row, "khhdon"))
    .bind(f(row, "shdon"))
    .bind(int(row, "hthdon"))
    .bind(int(row, "tchat"))
    .bind(num(row, "tgtcthue"))
    .bind(num(row, "tgtthue"))
    .bind(num(row, "ttcktmai"))
    .bind(num(row, "tgtttbso"))
    .bind(status)
    .bind(reason)
    .bind(row.to_string())
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn kind_str(kind: InvoiceKind) -> &'static str {
    match kind {
        InvoiceKind::Regular => "regular",
        InvoiceKind::CashRegister => "cash-register",
    }
}

/// tdlap của cổng là ISO-UTC; ngày quyết toán theo giờ Việt (UTC+7).
fn parse_tdlap(tdlap: &str) -> String {
    if tdlap.trim().is_empty() {
        return String::new();
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(tdlap.trim()) {
        return (dt + Duration::hours(7)).format("%Y-%m-%d").to_string();
    }
    if let Ok(d) = NaiveDate::parse_from_str(tdlap.trim(), "%Y-%m-%d") {
        return d.format("%Y-%m-%d").to_string();
    }
    String::new()
}

async fn cache_invoice(pool: &SqlitePool, portal_id: &str) -> Result<CacheState, String> {
    let n: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM hddt_purchase_invoice WHERE portal_id = ?")
            .bind(portal_id)
            .fetch_one(pool)
            .await
            .map_err(|e| e.to_string())?;
    Ok(if n.0 > 0 {
        CacheState::Existing
    } else {
        CacheState::New
    })
}

async fn mark_days_scanned(
    pool: &SqlitePool,
    from: NaiveDate,
    to: NaiveDate,
    kind: InvoiceKind,
) -> Result<(), String> {
    let mut d = from;
    while d <= to {
        sqlx::query(
            "INSERT INTO hddt_sync_day (day, kind, scanned_at) VALUES (?, ?, datetime('now'))
             ON CONFLICT(day, kind) DO NOTHING",
        )
        .bind(d.format("%Y-%m-%d").to_string())
        .bind(kind_str(kind))
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
        d += Duration::days(1);
    }
    Ok(())
}

/// Hóa đơn chờ xử lý mà chưa có chi tiết (và lần gọi trước không lỗi).
async fn pending_without_detail(pool: &SqlitePool) -> Result<Vec<CachedInvoice>, String> {
    let rows: Vec<CachedInvoice> = sqlx::query_as(
        "SELECT id, portal_id, portal_kind, posting_date, nbmst, nbten, khmshdon, khhdon, shdon,
                status, detail_json
         FROM hddt_purchase_invoice
         WHERE status = 'pending' AND detail_json = '' AND detail_error = ''",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows)
}

/// Ngày đã quét theo loại HĐ (yyyy-mm-dd) — dùng để bỏ qua khi quét lại.
async fn cached_days(
    pool: &SqlitePool,
    kind: InvoiceKind,
) -> Result<std::collections::HashSet<String>, String> {
    let rows: Vec<(String,)> = sqlx::query_as("SELECT day FROM hddt_sync_day WHERE kind = ?")
        .bind(kind_str(kind))
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(rows.into_iter().map(|(d,)| d).collect())
}

/// Xoá cache đồng bộ: chỉ xoá hóa đơn CHƯA nhập kho + các dấu ngày đã quét.
///
/// Hóa đơn đã nhập kho giữ nguyên để không mất liên kết hóa đơn ↔ phiếu nhập
/// (`portal_id` UNIQUE nên quét lại cũng không tạo trùng).
pub(crate) async fn clear_cache(pool: &SqlitePool) -> Result<ClearCacheOutcome, String> {
    let deleted = sqlx::query("DELETE FROM hddt_purchase_invoice WHERE status <> 'imported'")
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?
        .rows_affected();
    let days = sqlx::query("DELETE FROM hddt_sync_day")
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?
        .rows_affected();
    Ok(ClearCacheOutcome {
        invoices_deleted: deleted as usize,
        days_deleted: days as usize,
    })
}

#[derive(Debug, Default, serde::Serialize)]
pub(crate) struct ClearCacheOutcome {
    pub(crate) invoices_deleted: usize,
    pub(crate) days_deleted: usize,
}

/// Đưa hóa đơn lỗi chi tiết về lại 'pending' để lần quét sau thử tải lại.
///
/// Hóa đơn bị đánh 'manual' vì `tthai != 1` thì giữ nguyên (không tự tạo phiếu).
pub(crate) async fn retry_failed_details(pool: &SqlitePool) -> Result<usize, String> {
    let r = sqlx::query(
        "UPDATE hddt_purchase_invoice
            SET detail_error = '',
                status = 'pending',
                skip_reason = ''
          WHERE detail_error <> '' AND status = 'manual'",
    )
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    let r2 =
        sqlx::query("UPDATE hddt_purchase_invoice SET detail_error = '' WHERE detail_error <> ''")
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
    Ok((r.rows_affected() + r2.rows_affected()) as usize)
}

/// Đánh dấu các hóa đơn cần người xử lý: không lấy được dòng hàng, hoặc
/// thuộc loại hóa đơn không tạo phiếu tự động.
async fn mark_manual_cases(pool: &SqlitePool) -> Result<usize, String> {
    let r = sqlx::query(
        "UPDATE hddt_purchase_invoice
            SET status = 'manual',
                skip_reason = CASE
                    WHEN detail_error <> '' THEN 'Không lấy được chi tiết: ' || detail_error
                    WHEN status = 'manual' AND skip_reason <> '' THEN skip_reason
                    ELSE 'Cần xử lý thủ công' END
          WHERE status = 'pending' AND detail_error <> ''",
    )
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(r.rows_affected() as usize)
}

/// Mốc bắt đầu: max(hddt_start_date của hộ, tham số `from`).
async fn resolve_start(pool: &SqlitePool, from: &str) -> Result<NaiveDate, String> {
    let configured: Option<(String,)> =
        sqlx::query_as("SELECT hddt_start_date FROM business WHERE id = 1")
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;
    let configured = configured
        .map(|(s,)| s)
        .filter(|s| !s.trim().is_empty())
        .and_then(|s| NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok());
    let asked = NaiveDate::parse_from_str(from.trim(), "%Y-%m-%d").ok();
    match (configured, asked) {
        (Some(c), Some(a)) => Ok(c.max(a)),
        (Some(c), None) => Ok(c),
        (None, Some(a)) => Ok(a),
        (None, None) => {
            Err("Chưa khai báo ngày bắt đầu dùng HĐĐT — mở Cài đặt thông tin HKD để nhập.".into())
        }
    }
}

/// Mốc cuối: min(tham số `to`, hôm qua) — hóa đơn hôm nay còn đang phát sinh.
async fn resolve_end(to: &str) -> Result<NaiveDate, String> {
    let yesterday = chrono::Local::now().date_naive() - Duration::days(1);
    let asked = NaiveDate::parse_from_str(to.trim(), "%Y-%m-%d").ok();
    Ok(match asked {
        Some(a) => a.min(yesterday),
        None => yesterday,
    })
}

// ─── Import: tạo phiếu nhập kho từ hóa đơn đã cache ───

/// Kết quả 1 hóa đơn sau khi nhập kho.
#[derive(Debug, serde::Serialize)]
pub(crate) struct ImportOutcome {
    pub(crate) portal_id: String,
    pub(crate) ok: bool,
    pub(crate) message: String,
    pub(crate) voucher_no: String,
    pub(crate) products_created: usize,
}

/// Nhập kho 1 hóa đơn đã cache. Trả về lỗi (không panic) để caller báo từng HĐ.
pub(crate) async fn import_invoice(
    pool: &SqlitePool,
    inv: &CachedInvoice,
    warehouse_code: &str,
    unit_code: &str,
    debit_account: &str,
    credit_account: &str,
) -> Result<ImportOutcome, String> {
    let detail: Value = serde_json::from_str(&inv.detail_json)
        .map_err(|e| format!("Dữ liệu chi tiết hóa đơn hỏng: {e}"))?;
    let lines = detail
        .get("hdhhdvu")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if lines.is_empty() {
        return Err("Hóa đơn không có dòng hàng".into());
    }

    ensure_product_identity_keys(pool).await?;

    // 1) Mặt hàng: khớp theo (tên, đơn vị); thiếu thì tạo mới.
    let mut items: Vec<InboundItemInput> = Vec::new();
    let mut created = 0usize;
    for line in &lines {
        let name = f(line, "ten");
        // Dịch vụ (phí, phí dịch vụ…) không có đơn vị tính → cổng trả null.
        // Đơn vị gộp về "Dịch vụ", nhưng TÊN giữ nguyên verbatim (phương án B —
        // người dùng chọn giữ chi tiết đầy đủ): dòng phí có tên kèm kỳ/tháng
        // ("…PPS 08/2026…") nên mỗi kỳ là một mặt hàng riêng. Số mặt hàng mới
        // luôn hiển thị ở cột "HH mới" của bảng xem trước trước khi nhập kho.
        let raw_unit = f(line, "dvtinh");
        let unit = if raw_unit.trim().is_empty() {
            "Dịch vụ"
        } else {
            raw_unit.as_str()
        };
        if name.trim().is_empty() {
            continue;
        }
        let key = identity_key(&name, unit);
        let existing: Option<(String,)> =
            sqlx::query_as("SELECT code FROM product WHERE identity_key = ?")
                .bind(&key)
                .fetch_optional(pool)
                .await
                .map_err(|e| e.to_string())?;
        let code = match existing {
            Some((c,)) => c,
            None => {
                let code = next_product_code_core(pool).await?;
                let cost = num(line, "dgia");
                // Dòng không có đơn vị tính = dịch vụ (cổng trả dvtinh = null):
                // đánh dấu is_service để không tính tồn kho cho nó.
                let is_service = i64::from(raw_unit.trim().is_empty());
                sqlx::query(
                    "INSERT INTO product (code, name, unit, sale_price, cost_price,
                                          min_stock, vat_rate, import_tax_rate, is_service,
                                          industry_code, identity_key, portal_code)
                     VALUES (?, ?, ?, 0, ?, 0, ?, 0, ?, '', ?, ?)",
                )
                .bind(&code)
                .bind(&name)
                .bind(unit)
                .bind(cost)
                .bind(num(line, "tsuat"))
                .bind(is_service)
                .bind(&key)
                .bind(f(line, "mhhdvu"))
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
                created += 1;
                code
            }
        };
        items.push(InboundItemInput {
            product_code: code,
            quantity: num(line, "sluong"),
            unit_price: num(line, "dgia"),
            // Chiết khấu: `stckhau` là SỐ TIỀN CK trên dòng (đã kiểm chứng
            // live 24/09/2026: tlckhau = 50.0 là TỶ LỆ %, stckhau = 2.860.000
            // là tiền CK, dùng cho dòng 2 × 2.860.000). Sổ của app cần số tiền.
            discount: num(line, "stckhau"),
        });
    }
    if items.is_empty() {
        return Err("Hóa đơn không có dòng hàng hợp lệ".into());
    }

    // 2) Nhà cung cấp: khớp theo MST, thiếu thì tạo (mã = MST để đối chiếu).
    let supplier_code = ensure_supplier(pool, &inv.nbmst, &inv.nbten).await?;

    // 3) Số phiếu nhập — đánh số trên bảng đầu phiếu (authoritative).
    let voucher_no = next_inbound_voucher_no(pool).await?;

    // 4) Ghi phiếu: save_inbound_core tạo journal_entry + stock_lot trong 1 tx.
    let description = format!(
        "Nhập hàng theo hóa đơn {}{}",
        inv.khhdon,
        if inv.shdon.is_empty() {
            String::new()
        } else {
            format!(" số {}", inv.shdon)
        }
    );
    let reference_no = format!("{}{}", inv.khhdon, inv.shdon);
    let vat_rate = 0.0; // hộ nộp thuế theo doanh thu: giá nhập đã gồm thuế
    let result = save_inbound_core(
        pool,
        &inv.posting_date,
        &voucher_no,
        &description,
        &supplier_code,
        warehouse_code,
        if unit_code.is_empty() {
            "HKD"
        } else {
            unit_code
        },
        &items,
        "",
        "purchase",
        &reference_no,
        vat_rate,
        debit_account,
        credit_account,
        false, // không tự tạo phiếu chi — thanh toán do người dùng quyết định
        "",
    )
    .await?;

    // 5) Ghi đầu phiếu + liên kết hóa đơn chính thức (FK thật).
    let header_id: (i64,) = sqlx::query_as("SELECT id FROM inbound_voucher WHERE voucher_no = ?")
        .bind(&voucher_no)
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;
    let supplier_id: Option<(i64,)> = sqlx::query_as("SELECT id FROM supplier WHERE code = ?")
        .bind(&supplier_code)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query(
        "UPDATE hddt_purchase_invoice
            SET status = 'imported', voucher_no = ?, inbound_voucher_id = ?,
                supplier_id = ?, new_product_count = ?, imported_at = datetime('now')
          WHERE id = ?",
    )
    .bind(&voucher_no)
    .bind(header_id.0)
    .bind(supplier_id.map(|(i,)| i))
    .bind(created as i64)
    .bind(inv.id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(ImportOutcome {
        portal_id: inv.portal_id.clone(),
        ok: true,
        message: format!(
            "Đã tạo phiếu {} · {} dòng · {} hàng mới · tổng {}",
            voucher_no, result.entries, created, result.total
        ),
        voucher_no,
        products_created: created,
    })
}

/// Đảm bảo có nhà cung cấp theo MST (mã = MST, trùng thì dùng lại).
pub(crate) async fn ensure_supplier(
    pool: &SqlitePool,
    mst: &str,
    name: &str,
) -> Result<String, String> {
    let mst = mst.trim();
    if mst.is_empty() {
        return Err("Hóa đơn thiếu MST người bán".into());
    }
    let existing: Option<(String,)> = sqlx::query_as("SELECT code FROM supplier WHERE code = ?")
        .bind(mst)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?;
    if let Some((c,)) = existing {
        return Ok(c);
    }
    sqlx::query("INSERT OR IGNORE INTO supplier (code, name, address, tax_code, phone) VALUES (?, ?, '', ?, '')")
        .bind(mst)
        .bind(if name.trim().is_empty() { mst } else { name.trim() })
        .bind(mst)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(mst.to_string())
}

/// Số phiếu nhập tiếp theo: PN0001… đánh số trên bảng inbound_voucher.
pub(crate) async fn next_inbound_voucher_no(pool: &SqlitePool) -> Result<String, String> {
    let rows: Vec<(String,)> = sqlx::query_as("SELECT voucher_no FROM inbound_voucher")
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
    let max = rows
        .iter()
        .filter_map(|(v,)| v.strip_prefix("PN").and_then(|n| n.parse::<i64>().ok()))
        .max()
        .unwrap_or(0);
    Ok(format!("PN{:04}", max + 1))
}

/// Sinh mã mặt hàng tiếp theo (bản core, không cần Tauri State).
pub(crate) async fn next_product_code_core(pool: &SqlitePool) -> Result<String, String> {
    let prefix = setting(pool, "product_code_prefix").await?;
    let prefix = if prefix.trim().is_empty() {
        "SP".to_string()
    } else {
        prefix.trim().to_string()
    };
    let start: i64 = setting(pool, "product_code_start")
        .await?
        .trim()
        .parse()
        .unwrap_or(1)
        .max(1);
    let digits: usize = setting(pool, "product_code_digits")
        .await?
        .trim()
        .parse()
        .unwrap_or(4)
        .clamp(1, 12);
    let rows: Vec<(String,)> = sqlx::query_as("SELECT code FROM product")
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
    let mut max_num = start - 1;
    for (code,) in &rows {
        if let Some(num) = code.strip_prefix(&prefix) {
            if let Ok(n) = num.trim_start_matches('0').trim().parse::<i64>() {
                max_num = max_num.max(n);
            }
        }
    }
    Ok(format!(
        "{}{:0>width$}",
        prefix,
        max_num + 1,
        width = digits
    ))
}

async fn setting(pool: &SqlitePool, key: &str) -> Result<String, String> {
    let row: Option<(String,)> = sqlx::query_as("SELECT value FROM app_setting WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(row.map(|(v,)| v).unwrap_or_default())
}

// ─── Xem trước (đọc cache, không gọi cổng) ───

/// 1 dòng xem trước cho UI.
#[derive(Debug, Serialize, FromRow)]
pub(crate) struct PreviewRow {
    pub(crate) id: i64,
    pub(crate) portal_id: String,
    pub(crate) posting_date: String,
    pub(crate) nbmst: String,
    pub(crate) nbten: String,
    pub(crate) khhdon: String,
    pub(crate) shdon: String,
    pub(crate) portal_kind: String,
    pub(crate) status: String,
    pub(crate) skip_reason: String,
    pub(crate) line_count: i64,
    pub(crate) new_product_count: i64,
    pub(crate) tgtcthue: f64,
    pub(crate) tgtthue: f64,
    pub(crate) tgtttbso: f64,
    pub(crate) voucher_no: String,
    pub(crate) detail_error: String,
}

/// Tóm tắt xem trước.
#[derive(Debug, Serialize)]
pub(crate) struct PreviewSummary {
    pub(crate) total: usize,
    pub(crate) pending: usize,
    pub(crate) imported: usize,
    pub(crate) manual: usize,
    pub(crate) new_products: usize,
    pub(crate) total_value: f64,
    pub(crate) days_cached: usize,
}

/// Xem trước trong khoảng ngày (không gọi cổng).
pub(crate) async fn preview(
    pool: &SqlitePool,
    from: &str,
    to: &str,
) -> Result<(Vec<PreviewRow>, PreviewSummary), String> {
    let from = from.trim().to_string();
    let to = to.trim().to_string();
    let rows: Vec<PreviewRow> = sqlx::query_as(
        "SELECT id, portal_id, posting_date, nbmst, nbten, khhdon, shdon, portal_kind,
                status, skip_reason, line_count, new_product_count,
                tgtcthue, tgtthue, tgtttbso, voucher_no, detail_error
         FROM hddt_purchase_invoice
         WHERE (? = '' OR posting_date >= ?) AND (? = '' OR posting_date <= ?)
         ORDER BY posting_date DESC, khhdon DESC, shdon DESC",
    )
    .bind(&from)
    .bind(&from)
    .bind(&to)
    .bind(&to)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let days: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM hddt_sync_day")
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;

    let summary = PreviewSummary {
        total: rows.len(),
        pending: rows.iter().filter(|r| r.status == "pending").count(),
        imported: rows.iter().filter(|r| r.status == "imported").count(),
        manual: rows.iter().filter(|r| r.status == "manual").count(),
        new_products: rows
            .iter()
            .filter(|r| r.status == "pending")
            .map(|r| r.new_product_count as usize)
            .sum(),
        total_value: rows
            .iter()
            .filter(|r| r.status == "pending")
            .map(|r| r.tgtttbso)
            .sum(),
        days_cached: days.0 as usize,
    };
    Ok((rows, summary))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_pool;

    /// Chèn 1 hóa đơn cache + chi tiết giả, chạy import, kiểm tra chuỗi liên kết.
    async fn seed_invoice(pool: &SqlitePool, portal_id: &str, tthai: i64, lines: &str) -> i64 {
        sqlx::query(
            "INSERT INTO hddt_purchase_invoice
                (portal_id, portal_kind, tdlap, posting_date, nbmst, nbten, nmmst,
                 khmshdon, khhdon, shdon, hthdon, tchat, tgtcthue, tgtthue, ttcktmai,
                 tgtttbso, status, skip_reason, line_count, raw_json, detail_json)
             VALUES (?, 'regular', '2026-09-01T17:00:00Z', '2026-09-02', '0106773786',
                     'Cty TNHH ABC', '001170019085', 1, 'C26ABC', '001', 1, 1,
                     100000, 8000, 0, 108000, ?, '', ?, '{}', ?)",
        )
        .bind(portal_id)
        .bind(if tthai == 1 { "pending" } else { "manual" })
        .bind(
            serde_json::from_str::<Value>(&format!("[{lines}]"))
                .unwrap()
                .as_array()
                .unwrap()
                .len() as i64,
        )
        .bind(format!("{{\"hdhhdvu\":[{lines}]}}"))
        .execute(pool)
        .await
        .unwrap();
        let id: (i64,) = sqlx::query_as("SELECT id FROM hddt_purchase_invoice WHERE portal_id = ?")
            .bind(portal_id)
            .fetch_one(pool)
            .await
            .unwrap();
        id.0
    }

    async fn load(pool: &SqlitePool, id: i64) -> CachedInvoice {
        sqlx::query_as(
            "SELECT id, portal_id, posting_date, nbmst, nbten, khmshdon, khhdon, shdon,
                    status, detail_json
             FROM hddt_purchase_invoice WHERE id = ?",
        )
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn import_creates_voucher_header_and_links_invoice() {
        let pool = test_pool().await;
        let lines = r#"{"ten":"Bình NN Rossi","dvtinh":"Cái","mhhdvu":"puro30SL","sluong":2.0,
                        "dgia":1480000.0,"stckhau":0.0,"tsuat":0.08}"#;
        let id = seed_invoice(&pool, "uuid-1", 1, lines).await;

        let out = import_invoice(&pool, &load(&pool, id).await, "", "HKD", "", "")
            .await
            .expect("import thất bại");
        assert!(out.ok);
        assert_eq!(
            out.products_created, 1,
            "mặt hàng mới phải được tạo tự động"
        );
        assert!(out.voucher_no.starts_with("PN"), "số phiếu = PN…");

        // Đầu phiếu tồn tại và bút toán được gắn inbound_voucher_id.
        let header: (i64, f64, String) =
            sqlx::query_as("SELECT id, total, source FROM inbound_voucher WHERE voucher_no = ?")
                .bind(&out.voucher_no)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(header.2, "manual"); // save_inbound_core ghi source mặc định
        let linked: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM journal_entry WHERE inbound_voucher_id = ?")
                .bind(header.0)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(linked.0 > 0, "phải có bút toán gắn với đầu phiếu");

        // Hóa đơn liên kết chặt với phiếu nhập.
        let inv: (String, i64, i64) = sqlx::query_as(
            "SELECT status, inbound_voucher_id, new_product_count
               FROM hddt_purchase_invoice WHERE id = ?",
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(inv.0, "imported");
        assert_eq!(inv.1, header.0, "FK hóa đơn → phiếu nhập phải khớp");
        assert_eq!(inv.2, 1);

        // NCC tự tạo theo MST.
        let ncc: (String,) = sqlx::query_as("SELECT code FROM supplier WHERE code = ?")
            .bind("0106773786")
            .fetch_one(&pool)
            .await
            .expect("NCC phải được tạo tự động");
        assert_eq!(ncc.0, "0106773786");

        // Mặt hàng lưu khoá danh tính + mã cổng.
        let p: (String, String, String) =
            sqlx::query_as("SELECT code, identity_key, portal_code FROM product WHERE name = ?")
                .bind("Bình NN Rossi")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(p.1, identity_key("Bình NN Rossi", "Cái"));
        assert_eq!(p.2, "puro30SL");
    }

    #[tokio::test]
    async fn import_reuses_product_with_same_name_and_unit() {
        let pool = test_pool().await;
        // Tạo trước mặt hàng "cùng tên, cùng đơn vị" (chữ hoa + thừa space).
        sqlx::query(
            "INSERT INTO product (code, name, unit, identity_key)
             VALUES ('SP0001', '  bình nn rossi  ', 'CÁI', ?)",
        )
        .bind(identity_key("  bình nn rossi  ", "CÁI"))
        .execute(&pool)
        .await
        .unwrap();

        let lines = r#"{"ten":"Bình NN Rossi","dvtinh":"Cái","mhhdvu":"puro30SL","sluong":1.0,
                        "dgia":100.0,"stckhau":0.0,"tsuat":0.08}"#;
        let id = seed_invoice(&pool, "uuid-2", 1, lines).await;
        let out = import_invoice(&pool, &load(&pool, id).await, "", "HKD", "", "")
            .await
            .unwrap();
        assert_eq!(
            out.products_created, 0,
            "phải khớp mặt hàng đã có, không tạo mới"
        );
        let n: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM product")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(n.0, 1);
    }

    #[tokio::test]
    async fn different_unit_creates_separate_product() {
        let pool = test_pool().await;
        let lines_a = r#"{"ten":"Bình NN Rossi","dvtinh":"Cái","mhhdvu":"x","sluong":1.0,
                          "dgia":100.0,"stckhau":0.0,"tsuat":0.08}"#;
        let lines_b = r#"{"ten":"Bình NN Rossi","dvtinh":"Bộ","mhhdvu":"y","sluong":1.0,
                          "dgia":100.0,"stckhau":0.0,"tsuat":0.08}"#;
        let a = seed_invoice(&pool, "uuid-3", 1, lines_a).await;
        let b = seed_invoice(&pool, "uuid-4", 1, lines_b).await;
        import_invoice(&pool, &load(&pool, a).await, "", "HKD", "", "")
            .await
            .unwrap();
        let out = import_invoice(&pool, &load(&pool, b).await, "", "HKD", "", "")
            .await
            .unwrap();
        assert_eq!(out.products_created, 1, "khác đơn vị tính → mặt hàng khác");
        let n: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM product")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(n.0, 2);
    }

    #[tokio::test]
    async fn import_is_idempotent_second_run_creates_nothing() {
        let pool = test_pool().await;
        let lines = r#"{"ten":"Hàng A","dvtinh":"Cái","mhhdvu":"a","sluong":1.0,
                        "dgia":100.0,"stckhau":0.0,"tsuat":0.08}"#;
        let id = seed_invoice(&pool, "uuid-5", 1, lines).await;
        import_invoice(&pool, &load(&pool, id).await, "", "HKD", "", "")
            .await
            .unwrap();
        // Lần 2: hóa đơn đã 'imported' → caller bỏ qua, không tạo phiếu mới.
        let st: (String,) = sqlx::query_as("SELECT status FROM hddt_purchase_invoice WHERE id = ?")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(st.0, "imported");
        let vouchers: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM inbound_voucher")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(vouchers.0, 1, "chỉ 1 phiếu nhập cho 1 hóa đơn");
    }

    #[tokio::test]
    async fn voucher_numbers_do_not_collide() {
        let pool = test_pool().await;
        let a = next_inbound_voucher_no(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO inbound_voucher (voucher_no, created_at) VALUES (?, datetime('now'))",
        )
        .bind(&a)
        .execute(&pool)
        .await
        .unwrap();
        let b = next_inbound_voucher_no(&pool).await.unwrap();
        assert_ne!(a, b, "số phiếu phải tăng, không trùng");
    }

    #[tokio::test]
    async fn cached_days_are_known_per_kind() {
        let pool = test_pool().await;
        mark_days_scanned(
            &pool,
            NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 9, 3).unwrap(),
            InvoiceKind::Regular,
        )
        .await
        .unwrap();
        let reg = cached_days(&pool, InvoiceKind::Regular).await.unwrap();
        let cash = cached_days(&pool, InvoiceKind::CashRegister).await.unwrap();
        assert_eq!(reg.len(), 3, "3 ngày đã đánh dấu cho HĐ điện tử");
        assert!(reg.contains("2026-09-02"));
        assert!(cash.is_empty(), "không đánh dấu nhầm sang máy tính tiền");
    }

    #[tokio::test]
    async fn retry_failed_details_returns_invoice_to_pending() {
        // Lỗi chi tiết phải hồi phục được: trước đây xoá detail_error nhưng
        // status vẫn 'manual' → không bao giờ thử tải lại, không có nút Nhập kho.
        let pool = test_pool().await;
        let id = seed_invoice(&pool, "uuid-retry", 1, "{}").await;
        sqlx::query(
            "UPDATE hddt_purchase_invoice
                SET status = 'manual', detail_error = 'Too Many Requests', skip_reason = 'x'
              WHERE id = ?",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();

        let n = retry_failed_details(&pool).await.unwrap();
        assert_eq!(n, 1);
        let row: (String, String) =
            sqlx::query_as("SELECT status, detail_error FROM hddt_purchase_invoice WHERE id = ?")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row.0, "pending");
        assert_eq!(row.1, "");
    }

    #[tokio::test]
    async fn clear_cache_keeps_imported_invoices() {
        // Xoá cache để quét lại: hóa đơn đã nhập kho phải giữ để không mất liên kết.
        let pool = test_pool().await;
        let imported = seed_invoice(&pool, "uuid-keep", 1, "{}").await;
        let pending = seed_invoice(&pool, "uuid-drop", 1, "{}").await;
        sqlx::query("UPDATE hddt_purchase_invoice SET status = 'imported' WHERE id = ?")
            .bind(imported)
            .execute(&pool)
            .await
            .unwrap();
        mark_days_scanned(
            &pool,
            NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(),
            InvoiceKind::Regular,
        )
        .await
        .unwrap();

        let out = clear_cache(&pool).await.unwrap();
        assert_eq!(out.invoices_deleted, 1, "chỉ xoá hóa đơn chưa nhập kho");
        assert_eq!(out.days_deleted, 1);
        let left: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM hddt_purchase_invoice WHERE id = ?")
                .bind(imported)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(left.0, 1, "hóa đơn đã nhập kho phải còn lại");
        let gone: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM hddt_purchase_invoice WHERE id = ?")
                .bind(pending)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(gone.0, 0);
    }

    #[tokio::test]
    async fn service_line_keeps_full_name_verbatim() {
        // Phương án B (người dùng chọn): tên dòng dịch vụ giữ nguyên, dù dài và
        // có kỳ/tháng → mỗi kỳ là một mặt hàng riêng. Tên lấy thật từ cổng.
        let pool = test_pool().await;
        let long_name = "Phí dịch vụ tiếp thị liên kết PPS 08/2026 bachhoatonghop.phuongvi \
                         (AMS PPS Commission Fee 08/2026 bachhoatonghop.phuongvi)";
        let lines = format!(
            r#"{{"ten":"{long_name}","dvtinh":null,"mhhdvu":null,"sluong":1.0,
                 "dgia":27811.0,"stckhau":null,"tlckhau":null,"tsuat":0.08}}"#
        );
        let id = seed_invoice(&pool, "uuid-svc", 1, &lines).await;
        let out = import_invoice(&pool, &load(&pool, id).await, "", "HKD", "", "")
            .await
            .unwrap();
        assert_eq!(out.products_created, 1);
        // Tên lưu đúng verbatim; đơn vị tính gộp về "Dịch vụ" + đánh dấu dịch vụ.
        let p: (String, String, i64) = sqlx::query_as("SELECT name, unit, is_service FROM product")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(p.0, long_name, "tên dòng dịch vụ phải giữ nguyên");
        assert_eq!(p.1, "Dịch vụ");
        assert_eq!(p.2, 1, "đánh dấu dịch vụ để không tính tồn kho");

        // Kỳ khác → mặt hàng mới (đúng nghĩa phương án B).
        let next_month = long_name.replace("08/2026", "09/2026");
        let lines2 = format!(
            r#"{{"ten":"{next_month}","dvtinh":null,"mhhdvu":null,"sluong":1.0,
                 "dgia":27811.0,"stckhau":null,"tsuat":0.08}}"#
        );
        let id2 = seed_invoice(&pool, "uuid-svc2", 1, &lines2).await;
        let out2 = import_invoice(&pool, &load(&pool, id2).await, "", "HKD", "", "")
            .await
            .unwrap();
        assert_eq!(out2.products_created, 1, "kỳ khác → mặt hàng khác");
        let n: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM product")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(n.0, 2);

        // Cùng tên + cùng đơn vị → khớp mặt hàng đã có.
        let id3 = seed_invoice(&pool, "uuid-svc3", 1, &lines2).await;
        let out3 = import_invoice(&pool, &load(&pool, id3).await, "", "HKD", "", "")
            .await
            .unwrap();
        assert_eq!(out3.products_created, 0, "trùng tên + đơn vị → dùng lại");
    }

    #[tokio::test]
    async fn discount_uses_amount_field_not_percentage() {
        // Dữ liệu thật: tlckhau = 50.0 (%), stckhau = 2.860.000 (tiền CK)
        // trên dòng 2 × 2.860.000. Sổ nhập kho phải trừ đúng SỐ TIỀN.
        let pool = test_pool().await;
        let lines = r#"{"ten":"Aptomat ABN203c","dvtinh":"Cái","mhhdvu":null,"sluong":2.0,
                        "dgia":2860000.0,"stckhau":2860000.0,"tlckhau":50.0,"tsuat":0.08}"#;
        let id = seed_invoice(&pool, "uuid-ck", 1, lines).await;
        import_invoice(&pool, &load(&pool, id).await, "", "HKD", "", "")
            .await
            .unwrap();
        let amount: (f64,) = sqlx::query_as(
            "SELECT amount FROM journal_entry WHERE entry_type = 'PN' AND product_code <> ''",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        // 2 × 2.860.000 − 2.860.000 CK = 2.860.000
        assert!(
            (amount.0 - 2_860_000.0).abs() < 1.0,
            "giá trị nhập = thành tiền − tiền CK, thấy {}",
            amount.0
        );
    }

    #[tokio::test]
    async fn identity_key_normalizes_case_and_whitespace() {
        // Khác đơn vị → khác danh tính.
        assert_ne!(
            identity_key("Bình NN", "Cái"),
            identity_key("Bình NN", "Bộ")
        );
        // Khác chữ hoa/thừa khoảng trắng → CÙNG danh tính.
        assert_eq!(
            identity_key("  Bình NN  ", "CÁI"),
            identity_key("bình nn", "cái")
        );
        // Tiếng Việt có dấu: so khớp phải giống hệt (kể cả THƯỜNG vs thường).
        assert_eq!(
            identity_key("BÌNH NN", "Cái"),
            identity_key("bình nn", "cái")
        );
    }

    #[test]
    fn parse_tdlap_converts_utc_to_vietnam_date() {
        // 2026-09-23T17:00:00Z = 24/09/2026 giờ VN.
        assert_eq!(parse_tdlap("2026-09-23T17:00:00Z"), "2026-09-24");
        assert_eq!(parse_tdlap("2026-09-23"), "2026-09-23");
        assert_eq!(parse_tdlap(""), "");
    }

    #[test]
    fn day_format_roundtrip() {
        assert_eq!(iso_to_portal_day("2026-09-24"), "24/09/2026");
    }
}
