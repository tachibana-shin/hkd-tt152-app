// Đồng bộ hóa đơn mua vào từ cổng HĐĐT → hóa đơn chính thức + phiếu nhập kho.
//
// Luồng: `scan` (gọi cổng, cache theo ngày) → `preview` (đọc cache, không mạng)
//         → `import` (tạo mặt hàng/NCC/phiếu nhập + liên kết hóa đơn).
//
// Nguyên tắc:
//   * Idempotent — `hddt_purchase_invoice.portal_id` UNIQUE (id của cổng) nên
//     quét lại hay import lại 2 lần đều không tạo trùng.
//   * Ngày đã quét thì cache trong `hddt_sync_day` (hóa đơn ngày đã qua không đổi).
//   * Hôm nay vẫn quét nhưng không cache — hóa đơn trong ngày còn phát sinh.

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

/// Nhóm ngành mặc định cho mặt hàng HÀNG HÓA tạo từ hóa đơn: "PPHH" — Phân phối,
/// cung cấp hàng hóa (có sẵn từ migration khởi tạo). Hàng hóa mua vào từ hóa đơn
/// luôn thuộc nhóm này nên không bắt người dùng gán tay trước khi xuất bán.
const GOODS_INDUSTRY_CODE: &str = "PPHH";

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
    let rows = sqlx::query!("SELECT id, name, unit, identity_key FROM product")
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

    // nhóm (key) -> id nhỏ nhất
    let mut winner: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    for r in &rows {
        if r.name.trim().is_empty() {
            continue;
        }
        let key = identity_key(&r.name, &r.unit);
        winner
            .entry(key)
            .and_modify(|min| *min = (*min).min(r.id))
            .or_insert(r.id);
    }

    for r in rows {
        let want = if r.name.trim().is_empty() {
            String::new()
        } else {
            let key = identity_key(&r.name, &r.unit);
            if winner.get(&key) == Some(&r.id) {
                key
            } else {
                String::new()
            }
        };
        if r.identity_key != want {
            sqlx::query!(
                "UPDATE product SET identity_key = ? WHERE id = ?",
                want,
                r.id
            )
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
    /// Hóa đơn lỗi chi tiết lượt trước được thử lại lượt này.
    pub(crate) details_retried: usize,
    /// Số lần đã quét hôm nay (luôn quét lại, không cache).
    pub(crate) days_today: usize,
}

/// Quét một cửa sổ ngày (đã gom ≤ 30 ngày) rồi ghi cache từng hóa đơn. Việc đánh
/// dấu "ngày đã quét" do caller quyết định — hôm nay thì cố ý không đánh dấu.
async fn scan_window(
    pool: &SqlitePool,
    client: &HddtClient,
    token: &str,
    kind: InvoiceKind,
    win_start: NaiveDate,
    win_end: NaiveDate,
    sum: &mut ScanSummary,
) -> Result<(), String> {
    let mut state: Option<String> = None;
    loop {
        let q = InvoiceQuery {
            direction: crate::hddt::InvoiceDirection::Purchase,
            kind,
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
            if upsert_invoice(pool, row, kind).await? {
                sum.invoices_new += 1;
            } else {
                sum.invoices_existing += 1;
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
    Ok(())
}

/// Quét cổng trong khoảng ngày và ghi cache.
///
/// `from`/`to` = yyyy-mm-dd. Mốc thấp nhất lấy từ `hddt_start_date` (khai
/// báo trong popup cấu hình HKD). Mốc cao nhất có thể là **hôm nay**: người dùng
/// được chọn ngày hiện tại, nhưng ngày hôm nay không được đánh dấu cache (hóa đơn
/// trong ngày còn phát sinh) nên mỗi lượt quét đều lấy lại.
pub(crate) async fn scan(
    pool: &SqlitePool,
    client: &HddtClient,
    token: &str,
    from: &str,
    to: &str,
    kinds: &[InvoiceKind],
) -> Result<ScanSummary, String> {
    // Hóa đơn lượt trước lỗi chi tiết (429, mạng chập chờn…) → thử lại ngay
    // trong lượt quét này, không bắt người dùng phải bấm "Quét cổng" hai lần.
    let mut sum = ScanSummary {
        details_retried: retry_failed_details(pool).await?,
        ..Default::default()
    };

    let today = chrono::Local::now().date_naive();
    let start_date = resolve_start(pool, from).await?;
    let end_date = resolve_end(to).await?;
    if start_date > end_date {
        return Err(format!(
            "Khoảng ngày không hợp lệ: {} → {}. Hãy chọn \"Từ ngày lập\" không sau \
             \"Đến ngày lập\".",
            start_date, end_date
        ));
    }
    // Ngày đã qua thì cache (hóa đơn không đổi); hôm nay thì quét nhưng không cache.
    let cache_end = std::cmp::min(end_date, today - Duration::days(1));

    for kind in kinds {
        let cached = cached_days(pool, *kind).await?;
        // `NaiveDate` không có `Iterator::filter` thuận tiện → dựng list ngày trước.
        let mut all_days: Vec<NaiveDate> = Vec::new();
        let mut d = start_date;
        while d <= cache_end {
            all_days.push(d);
            d += Duration::days(1);
        }
        let missing: Vec<NaiveDate> = all_days
            .iter()
            .copied()
            .filter(|d| !cached.contains(&d.format("%Y-%m-%d").to_string()))
            .collect();
        sum.days_cached += all_days.len() - missing.len();

        // Hôm nay: lấy để thấy hóa đơn vừa phát sinh, nhưng KHÔNG đánh dấu cache
        // để lượt quét sau vẫn lấy lại được.
        if end_date >= today {
            scan_window(pool, client, token, *kind, today, today, &mut sum).await?;
            sum.days_today += 1;
        }
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
            scan_window(pool, client, token, *kind, win_start, win_end, &mut sum).await?;
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
            sqlx::query!(
                "UPDATE hddt_purchase_invoice SET detail_error = ? WHERE id = ?",
                msg,
                inv.id
            )
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
                // Bind vào biến trước: macro giữ tham chiếu tới hết `.await`.
                let detail_json = detail.to_string();
                let line_count = lines as i64;
                sqlx::query!(
                    "UPDATE hddt_purchase_invoice
                        SET detail_json = ?, line_count = ?, detail_error = ''
                      WHERE id = ?",
                    detail_json,
                    line_count,
                    inv.id
                )
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
                sum.details_ok += 1;
            }
            Err(msg) => {
                sqlx::query!(
                    "UPDATE hddt_purchase_invoice SET detail_error = ? WHERE id = ?",
                    msg,
                    inv.id
                )
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

/// Ghi 1 hóa đơn vào cache (ON CONFLICT portal_id DO NOTHING — không đè dữ liệu
/// đã có). Trả `true` nếu là dòng mới.
async fn upsert_invoice(pool: &SqlitePool, row: &Value, kind: InvoiceKind) -> Result<bool, String> {
    let portal_id = f(row, "id");
    if portal_id.is_empty() {
        return Ok(false);
    }
    // Đã nhập kho rồi (bảng hóa đơn chính thức) → không đưa lại vào cache.
    let done: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM hddt_imported_invoice WHERE portal_id = ?",
        portal_id
    )
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;
    if done > 0 {
        return Ok(false);
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
    let raw_json = row.to_string();
    // Mọi giá trị bind vào biến trước: macro sqlx giữ tham chiếu tới hết `.await`
    // nên truyền biểu thức tạm (f(...)/int(...)/kind_str(...)) sẽ báo E0716.
    let (nbmst, nbten, nmmst) = (f(row, "nbmst"), f(row, "nbten"), f(row, "nmmst"));
    let (khmshdon, hthdon, tchat) = (int(row, "khmshdon"), int(row, "hthdon"), int(row, "tchat"));
    let (khhdon, shdon) = (f(row, "khhdon"), f(row, "shdon"));
    let (tgtcthue, tgtthue) = (num(row, "tgtcthue"), num(row, "tgtthue"));
    let (ttcktmai, tgtttbso) = (num(row, "ttcktmai"), num(row, "tgtttbso"));
    let portal_kind = kind_str(kind);
    let res = sqlx::query!(
        "INSERT INTO hddt_purchase_invoice
            (portal_id, portal_kind, tdlap, posting_date, nbmst, nbten, nmmst,
             khmshdon, khhdon, shdon, hthdon, tchat,
             tgtcthue, tgtthue, ttcktmai, tgtttbso, status, skip_reason,
             raw_json, created_at)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,datetime('now'))
         ON CONFLICT(portal_id) DO NOTHING",
        portal_id,
        portal_kind,
        tdlap,
        posting_date,
        nbmst,
        nbten,
        nmmst,
        khmshdon,
        khhdon,
        shdon,
        hthdon,
        tchat,
        tgtcthue,
        tgtthue,
        ttcktmai,
        tgtttbso,
        status,
        reason,
        raw_json,
    )
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(res.rows_affected() > 0)
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

async fn mark_days_scanned(
    pool: &SqlitePool,
    from: NaiveDate,
    to: NaiveDate,
    kind: InvoiceKind,
) -> Result<(), String> {
    let mut d = from;
    while d <= to {
        let day = d.format("%Y-%m-%d").to_string();
        let kind = kind_str(kind);
        sqlx::query!(
            "INSERT INTO hddt_sync_day (day, kind, scanned_at) VALUES (?, ?, datetime('now'))
             ON CONFLICT(day, kind) DO NOTHING",
            day,
            kind,
        )
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
        d += Duration::days(1);
    }
    Ok(())
}

/// Hóa đơn chờ xử lý mà chưa có chi tiết (và lần gọi trước không lỗi).
async fn pending_without_detail(pool: &SqlitePool) -> Result<Vec<CachedInvoice>, String> {
    let rows: Vec<CachedInvoice> = sqlx::query_as!(
        CachedInvoice,
        r#"SELECT id as "id!", portal_id, posting_date, nbmst, nbten,
                  khmshdon as "khmshdon!", khhdon, shdon, status, detail_json
             FROM hddt_purchase_invoice
            WHERE status = 'pending' AND detail_json = '' AND detail_error = ''"#
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
    let kind = kind_str(kind);
    let rows: Vec<String> =
        sqlx::query_scalar!("SELECT day FROM hddt_sync_day WHERE kind = ?", kind)
            .fetch_all(pool)
            .await
            .map_err(|e| e.to_string())?;
    Ok(rows.into_iter().collect())
}

/// Xoá cache đồng bộ: toàn bộ dòng cache (chưa nhập kho) + các dấu ngày đã quét.
///
/// Hóa đơn đã nhập kho nằm ở `hddt_imported_invoice` nên không bị đụng tới —
/// liên kết hóa đơn ↔ phiếu nhập luôn giữ nguyên.
pub(crate) async fn clear_cache(pool: &SqlitePool) -> Result<ClearCacheOutcome, String> {
    let deleted = sqlx::query!("DELETE FROM hddt_purchase_invoice")
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?
        .rows_affected();
    let days = sqlx::query!("DELETE FROM hddt_sync_day")
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
    let r = sqlx::query!(
        "UPDATE hddt_purchase_invoice
            SET detail_error = '',
                status = 'pending',
                skip_reason = ''
          WHERE detail_error <> '' AND status = 'manual'"
    )
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    let r2 =
        sqlx::query!("UPDATE hddt_purchase_invoice SET detail_error = '' WHERE detail_error <> ''")
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
    Ok((r.rows_affected() + r2.rows_affected()) as usize)
}

/// Đánh dấu các hóa đơn cần người xử lý: không lấy được dòng hàng, hoặc
/// thuộc loại hóa đơn không tạo phiếu tự động.
async fn mark_manual_cases(pool: &SqlitePool) -> Result<usize, String> {
    let r = sqlx::query!(
        "UPDATE hddt_purchase_invoice
            SET status = 'manual',
                skip_reason = CASE
                    WHEN detail_error <> '' THEN 'Không lấy được chi tiết: ' || detail_error
                    WHEN status = 'manual' AND skip_reason <> '' THEN skip_reason
                    ELSE 'Cần xử lý thủ công' END
          WHERE status = 'pending' AND detail_error <> ''"
    )
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(r.rows_affected() as usize)
}

/// Mốc bắt đầu: max(hddt_start_date của hộ, tham số `from`).
async fn resolve_start(pool: &SqlitePool, from: &str) -> Result<NaiveDate, String> {
    let row = sqlx::query!("SELECT hddt_start_date FROM business WHERE id = 1")
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?;
    let configured = row
        .map(|r| r.hddt_start_date)
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

/// Mốc cuối: người dùng chọn (rỗng → hôm nay). Không chặn hôm nay: phần "không
/// cache hôm nay" do `scan` xử lý.
async fn resolve_end(to: &str) -> Result<NaiveDate, String> {
    let asked = NaiveDate::parse_from_str(to.trim(), "%Y-%m-%d").ok();
    Ok(asked.unwrap_or_else(|| chrono::Local::now().date_naive()))
}

/// Nhóm ngành gán cho mặt hàng hàng hóa tạo từ hóa đơn. Trả về rỗng nếu nhóm
/// `PPHH` không còn trong danh mục (người dùng đã xoá) — để màn Sản phẩm báo
/// cần gán nhóm ngành thay vì ghi mã không tồn tại.
async fn goods_industry_code(pool: &SqlitePool) -> Result<String, String> {
    let found: Option<String> = sqlx::query_scalar!(
        "SELECT code FROM industry_group WHERE code = ?",
        GOODS_INDUSTRY_CODE
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(found.unwrap_or_default())
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
    // Đã có trong bảng hóa đơn chính thức → không tạo phiếu lần nữa (cache có
    // thể bị xoá rồi quét lại cùng ngày đó).
    if let Some(voucher_no) = sqlx::query_scalar!(
        "SELECT iv.voucher_no FROM hddt_imported_invoice hi
           JOIN inbound_voucher iv ON iv.id = hi.inbound_voucher_id
          WHERE hi.portal_id = ?",
        inv.portal_id
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    {
        return Ok(ImportOutcome {
            portal_id: inv.portal_id.clone(),
            ok: true,
            message: format!("Đã nhập kho trước đó — phiếu {voucher_no}"),
            voucher_no,
            products_created: 0,
        });
    }

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
    let goods_industry = goods_industry_code(pool).await?;
    for line in &lines {
        let name = f(line, "ten");
        // Dịch vụ (phí, phí dịch vụ…) không có đơn vị tính → cổng trả null: đơn vị
        // gộp về "Dịch vụ". Tên giữ nguyên verbatim vì có kèm kỳ/tháng
        // ("…PPS 08/2026…") nên mỗi kỳ là một mặt hàng riêng.
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
        let existing: Option<String> =
            sqlx::query_scalar!("SELECT code FROM product WHERE identity_key = ?", key)
                .fetch_optional(pool)
                .await
                .map_err(|e| e.to_string())?;
        let code = match existing {
            Some(c) => c,
            None => {
                let code = next_product_code_core(pool).await?;
                let cost = num(line, "dgia");
                // Dòng không có đơn vị tính = dịch vụ (cổng trả dvtinh = null):
                // đánh dấu is_service để không tính tồn kho cho nó.
                let is_service = i64::from(raw_unit.trim().is_empty());
                // Hàng hóa (không phải dịch vụ) mặc định nhóm "PPHH" — người dùng
                // không phải gán tay trước khi xuất bán. Dịch vụ để rỗng: tỷ lệ
                // thuế của dịch vụ người dùng chọn theo ngành thực tế.
                let industry = if is_service == 0 {
                    goods_industry.as_str()
                } else {
                    ""
                };
                // Biểu thức tạm phải bind trước (macro giữ tham chiếu tới hết await).
                let vat_rate = num(line, "tsuat");
                let portal_code = f(line, "mhhdvu");
                sqlx::query!(
                    "INSERT INTO product (code, name, unit, sale_price, cost_price,
                                          min_stock, vat_rate, import_tax_rate, is_service,
                                          industry_code, identity_key, portal_code)
                     VALUES (?, ?, ?, 0, ?, 0, ?, 0, ?, ?, ?, ?)",
                    code,
                    name,
                    unit,
                    cost,
                    vat_rate,
                    is_service,
                    industry,
                    key,
                    portal_code,
                )
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

    // 5) Hóa đơn chính thức + liên kết chặt với phiếu nhập (FK thật). Dữ liệu
    // này nằm ở bảng riêng `hddt_imported_invoice`, sau đó dòng cache bị xoá.
    let header_id: i64 = sqlx::query_scalar!(
        r#"SELECT id as "id!" FROM inbound_voucher WHERE voucher_no = ?"#,
        voucher_no
    )
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;
    let supplier_id: Option<i64> = sqlx::query_scalar!(
        r#"SELECT id as "id!" FROM supplier WHERE code = ?"#,
        supplier_code
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;
    let new_product_count = created as i64;
    sqlx::query!(
        "INSERT INTO hddt_imported_invoice
            (portal_id, portal_kind, posting_date, nbmst, nbten, nmmst, khmshdon, khhdon, shdon,
             tgtcthue, tgtthue, ttcktmai, tgtttbso, line_count, new_product_count,
             supplier_id, inbound_voucher_id, raw_json, detail_json)
         SELECT portal_id, portal_kind, posting_date, nbmst, nbten, nmmst, khmshdon, khhdon, shdon,
                tgtcthue, tgtthue, ttcktmai, tgtttbso, line_count, ?, ?, ?,
                raw_json, detail_json
           FROM hddt_purchase_invoice WHERE id = ?",
        new_product_count,
        supplier_id,
        header_id,
        inv.id
    )
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    // Cache đã "tiêu" rồi → xoá khỏi cache, dữ liệu chính thức nằm ở bảng trên.
    sqlx::query!("DELETE FROM hddt_purchase_invoice WHERE id = ?", inv.id)
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
    let existing: Option<String> =
        sqlx::query_scalar!("SELECT code FROM supplier WHERE code = ?", mst)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;
    if let Some(c) = existing {
        return Ok(c);
    }
    let tax_code = if name.trim().is_empty() {
        mst
    } else {
        name.trim()
    };
    sqlx::query!(
        "INSERT OR IGNORE INTO supplier (code, name, address, tax_code, phone)
         VALUES (?, ?, '', ?, '')",
        mst,
        tax_code,
        mst,
    )
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(mst.to_string())
}

/// Số phiếu nhập tiếp theo: PN0001… đánh số trên bảng inbound_voucher.
pub(crate) async fn next_inbound_voucher_no(pool: &SqlitePool) -> Result<String, String> {
    let rows: Vec<String> = sqlx::query_scalar!("SELECT voucher_no FROM inbound_voucher")
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
    let max = rows
        .iter()
        .filter_map(|v| v.strip_prefix("PN").and_then(|n| n.parse::<i64>().ok()))
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
    let rows: Vec<String> = sqlx::query_scalar!("SELECT code FROM product")
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
    let mut max_num = start - 1;
    for code in &rows {
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
    let row: Option<String> =
        sqlx::query_scalar!("SELECT value FROM app_setting WHERE key = ?", key)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;
    Ok(row.unwrap_or_default())
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
    /// Tổng tiền chiết khấu thương mại trên hóa đơn (cổng trả về `ttcktmai`).
    pub(crate) ttcktmai: f64,
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
    // Cache (chờ nhập / cần xử lý) + hóa đơn đã nhập kho (bảng riêng) — cùng
    // một danh sách để người dùng thấy trạng thái đầy đủ. Dòng đã nhập kho có
    // `id = 0` (không có hành động) để không lẫn id với dòng cache.
    let rows: Vec<PreviewRow> = sqlx::query_as!(
        PreviewRow,
        "SELECT id, portal_id, posting_date, nbmst, nbten, khhdon, shdon, portal_kind,
                status, skip_reason, line_count, new_product_count,
                tgtcthue, tgtthue, ttcktmai, tgtttbso, voucher_no, detail_error
           FROM hddt_purchase_invoice
          WHERE (? = '' OR posting_date >= ?) AND (? = '' OR posting_date <= ?)
         UNION ALL
         SELECT 0, hi.portal_id, hi.posting_date, hi.nbmst, hi.nbten, hi.khhdon, hi.shdon,
                hi.portal_kind, 'imported', '', hi.line_count, hi.new_product_count,
                hi.tgtcthue, hi.tgtthue, hi.ttcktmai, hi.tgtttbso,
                COALESCE(iv.voucher_no, ''), ''
           FROM hddt_imported_invoice hi
           LEFT JOIN inbound_voucher iv ON iv.id = hi.inbound_voucher_id
          WHERE (? = '' OR hi.posting_date >= ?) AND (? = '' OR hi.posting_date <= ?)
         ORDER BY posting_date DESC, khhdon DESC, shdon DESC",
        from,
        from,
        to,
        to,
        from,
        from,
        to,
        to,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let days: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM hddt_sync_day")
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
        days_cached: days as usize,
    };
    Ok((rows, summary))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_pool;

    /// Chèn 1 hóa đơn cache + chi tiết giả, chạy import, kiểm tra chuỗi liên kết.
    async fn seed_invoice(pool: &SqlitePool, portal_id: &str, tthai: i64, lines: &str) -> i64 {
        let status = if tthai == 1 { "pending" } else { "manual" };
        let line_count = serde_json::from_str::<Value>(&format!("[{lines}]"))
            .unwrap()
            .as_array()
            .unwrap()
            .len() as i64;
        let detail = format!("{{\"hdhhdvu\":[{lines}]}}");
        sqlx::query!(
            "INSERT INTO hddt_purchase_invoice
                (portal_id, portal_kind, tdlap, posting_date, nbmst, nbten, nmmst,
                 khmshdon, khhdon, shdon, hthdon, tchat, tgtcthue, tgtthue, ttcktmai,
                 tgtttbso, status, skip_reason, line_count, raw_json, detail_json)
             VALUES (?, 'regular', '2026-09-01T17:00:00Z', '2026-09-02', '0106773786',
                     'Cty TNHH ABC', '001170019085', 1, 'C26ABC', '001', 1, 1,
                     100000, 8000, 0, 108000, ?, '', ?, '{}', ?)",
            portal_id,
            status,
            line_count,
            detail
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query_scalar!(
            r#"SELECT id as "id!" FROM hddt_purchase_invoice WHERE portal_id = ?"#,
            portal_id
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    async fn load(pool: &SqlitePool, id: i64) -> CachedInvoice {
        sqlx::query_as!(
            CachedInvoice,
            r#"SELECT id as "id!", portal_id, posting_date, nbmst, nbten,
                      khmshdon as "khmshdon!", khhdon, shdon, status, detail_json
                 FROM hddt_purchase_invoice WHERE id = ?"#,
            id
        )
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
        let header = sqlx::query!(
            r#"SELECT id as "id!", total, source FROM inbound_voucher WHERE voucher_no = ?"#,
            out.voucher_no
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(header.source, "manual"); // save_inbound_core ghi source mặc định
        let linked: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM journal_entry WHERE inbound_voucher_id = ?",
            header.id
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(linked > 0, "phải có bút toán gắn với đầu phiếu");

        // Hóa đơn chính thức nằm ở bảng riêng, liên kết chặt với phiếu nhập;
        // dòng cache tương ứng bị xoá.
        let inv = sqlx::query!(
            "SELECT inbound_voucher_id, new_product_count
               FROM hddt_imported_invoice WHERE portal_id = 'uuid-1'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            inv.inbound_voucher_id, header.id,
            "FK hóa đơn → phiếu nhập phải khớp"
        );
        assert_eq!(inv.new_product_count, 1);
        let cache_left: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM hddt_purchase_invoice WHERE id = ?",
            id
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(cache_left, 0, "hóa đơn đã nhập không nằm lại trong cache");

        // NCC tự tạo theo MST.
        let ncc: String =
            sqlx::query_scalar!(r#"SELECT code FROM supplier WHERE code = '0106773786'"#)
                .fetch_one(&pool)
                .await
                .expect("NCC phải được tạo tự động");
        assert_eq!(ncc, "0106773786");

        // Mặt hàng lưu khoá danh tính + mã cổng.
        let p = sqlx::query!(
            "SELECT code, identity_key, portal_code FROM product WHERE name = 'Bình NN Rossi'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(p.identity_key, identity_key("Bình NN Rossi", "Cái"));
        assert_eq!(p.portal_code, "puro30SL");
    }

    #[tokio::test]
    async fn import_reuses_product_with_same_name_and_unit() {
        let pool = test_pool().await;
        // Tạo trước mặt hàng "cùng tên, cùng đơn vị" (chữ hoa + thừa space).
        let key = identity_key("  bình nn rossi  ", "CÁI");
        sqlx::query!(
            "INSERT INTO product (code, name, unit, identity_key)
             VALUES ('SP0001', '  bình nn rossi  ', 'CÁI', ?)",
            key
        )
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
        let n: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM product")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(n, 1);
    }

    /// Hàng hóa tạo từ hóa đơn mặc định nhóm ngành PPHH (để xuất bán không phải
    /// gán tay); dòng dịch vụ (cổng trả dvtinh = null) để trống vì tỷ lệ thuế
    /// phụ thuộc ngành thực tế.
    #[tokio::test]
    async fn goods_product_gets_pphh_industry_service_stays_empty() {
        let pool = test_pool().await;
        let lines = r#"{"ten":"Bình NN Rossi","dvtinh":"Cái","mhhdvu":"puro30","sluong":1.0,
                        "dgia":100.0,"stckhau":0.0,"tsuat":0.08},
                       {"ten":"Phí vận chuyển","dvtinh":null,"mhhdvu":"phi","sluong":1.0,
                        "dgia":50.0,"stckhau":0.0,"tsuat":0.1}"#;
        let id = seed_invoice(&pool, "uuid-industry", 1, lines).await;
        let out = import_invoice(&pool, &load(&pool, id).await, "", "HKD", "", "")
            .await
            .unwrap();
        assert_eq!(out.products_created, 2);

        let goods = sqlx::query!(
            "SELECT industry_code, unit, is_service FROM product WHERE name = 'Bình NN Rossi'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            goods.industry_code, "PPHH",
            "hàng hóa phải mặc định nhóm PPHH"
        );
        assert_eq!(goods.unit, "Cái");
        assert_eq!(goods.is_service, 0);

        let service = sqlx::query!(
            "SELECT industry_code, is_service FROM product WHERE name = 'Phí vận chuyển'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            service.industry_code, "",
            "dịch vụ không tự gán nhóm ngành hàng hóa"
        );
        assert_eq!(service.is_service, 1);
    }

    /// Người dùng xoá nhóm PPHH thì mặt hàng mới để trống, không ghi mã nhóm
    /// không tồn tại (màn Sản phẩm sẽ báo cần gán nhóm ngành).
    #[tokio::test]
    async fn missing_pphh_group_leaves_industry_empty() {
        let pool = test_pool().await;
        sqlx::query!("DELETE FROM industry_group WHERE code = 'PPHH'")
            .execute(&pool)
            .await
            .unwrap();
        let lines = r#"{"ten":"Hàng B","dvtinh":"Cái","mhhdvu":"b","sluong":1.0,
                        "dgia":100.0,"stckhau":0.0,"tsuat":0.08}"#;
        let id = seed_invoice(&pool, "uuid-no-industry", 1, lines).await;
        import_invoice(&pool, &load(&pool, id).await, "", "HKD", "", "")
            .await
            .unwrap();
        let p: String =
            sqlx::query_scalar!("SELECT industry_code FROM product WHERE name = 'Hàng B'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(p, "");
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
        let n: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM product")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(n, 2);
    }

    #[tokio::test]
    async fn import_is_idempotent_second_run_creates_nothing() {
        let pool = test_pool().await;
        let lines = r#"{"ten":"Hàng A","dvtinh":"Cái","mhhdvu":"a","sluong":1.0,
                        "dgia":100.0,"stckhau":0.0,"tsuat":0.08}"#;
        let id = seed_invoice(&pool, "uuid-5", 1, lines).await;
        let inv = load(&pool, id).await;
        let first = import_invoice(&pool, &inv, "", "HKD", "", "")
            .await
            .unwrap();
        assert!(first.voucher_no.starts_with("PN"));

        // Lần 2 (ví dụ cache bị xoá rồi quét lại đúng ngày đó): hóa đơn đã có
        // trong bảng chính thức → báo đã nhập, không tạo phiếu thứ hai.
        let again = import_invoice(&pool, &inv, "", "HKD", "", "")
            .await
            .unwrap();
        assert_eq!(again.voucher_no, first.voucher_no);
        assert!(again.message.contains("Đã nhập kho trước đó"));
        let vouchers: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM inbound_voucher")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(vouchers, 1, "chỉ 1 phiếu nhập cho 1 hóa đơn");
    }

    #[tokio::test]
    async fn voucher_numbers_do_not_collide() {
        let pool = test_pool().await;
        let a = next_inbound_voucher_no(&pool).await.unwrap();
        sqlx::query!(
            "INSERT INTO inbound_voucher (voucher_no, created_at) VALUES (?, datetime('now'))",
            a
        )
        .execute(&pool)
        .await
        .unwrap();
        let b = next_inbound_voucher_no(&pool).await.unwrap();
        assert_ne!(a, b, "số phiếu phải tăng, không trùng");
    }

    // ─── Test offline với mock portal ───

    #[tokio::test]
    async fn scan_then_import_round_trip_against_mock_portal() {
        use crate::hddt::mock_portal::MockPortal;
        let portal = MockPortal::start().await;
        let client = HddtClient::for_test(&portal.base);
        let pool = test_pool().await;
        // Hôm nay là 25/09/2026 trong test dữ liệu → quét 01/09 → 24/09.
        let from = "2026-09-01";
        let to = "2026-09-24";

        let sum = scan(
            &pool,
            &client,
            "mock-token",
            from,
            to,
            &[InvoiceKind::Regular],
        )
        .await
        .expect("quét mock thất bại");
        assert_eq!(sum.days_scanned, 24, "phải quét hết 24 ngày chưa cache");
        assert_eq!(sum.days_cached, 0);
        assert_eq!(sum.invoices_new, 1);
        assert_eq!(
            sum.details_ok, 1,
            "mock trả khmshdon số → lấy chi tiết được"
        );
        assert_eq!(sum.details_failed, 0);

        // Lần 2: mọi ngày đã cache → không gọi lại endpoint danh sách.
        portal.reset();
        let sum2 = scan(
            &pool,
            &client,
            "mock-token",
            from,
            to,
            &[InvoiceKind::Regular],
        )
        .await
        .expect("quét lại thất bại");
        assert_eq!(sum2.days_scanned, 0);
        assert_eq!(sum2.days_cached, 24);
        assert_eq!(
            portal.count("/invoices/purchase"),
            0,
            "không được gọi lại cổng cho ngày đã cache"
        );

        // Nhập kho → tạo phiếu + hóa đơn chính thức, dòng cache biến mất.
        let cached = pending_without_detail(&pool).await.unwrap();
        assert!(
            cached.is_empty(),
            "đã lấy chi tiết nên không còn chờ lấy nữa"
        );
        let row: CachedInvoice = sqlx::query_as!(
            CachedInvoice,
            r#"SELECT id as "id!", portal_id, posting_date, nbmst, nbten,
                      khmshdon as "khmshdon!", khhdon, shdon, status, detail_json
                 FROM hddt_purchase_invoice LIMIT 1"#
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let out = import_invoice(&pool, &row, "", "HKD", "", "")
            .await
            .unwrap();
        assert!(out.ok);
        let imported: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM hddt_imported_invoice")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(imported, 1);
        let cache_left: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM hddt_purchase_invoice")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(cache_left, 0, "cache phải sạch sau khi nhập kho");

        // Quét lại cùng khoảng ngày (xoá dấu ngày) → hóa đơn đã nhập không quay
        // lại cache nữa, nên không thể tạo phiếu lần hai.
        sqlx::query!("DELETE FROM hddt_sync_day")
            .execute(&pool)
            .await
            .unwrap();
        let sum3 = scan(
            &pool,
            &client,
            "mock-token",
            from,
            to,
            &[InvoiceKind::Regular],
        )
        .await
        .unwrap();
        assert_eq!(
            sum3.invoices_new, 0,
            "hóa đơn đã nhập không được ghi lại cache"
        );
        let again: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM hddt_purchase_invoice")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(again, 0);
        let vouchers: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM inbound_voucher")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(vouchers, 1, "vẫn chỉ 1 phiếu nhập");
    }

    #[tokio::test]
    async fn scan_today_is_fetched_but_never_cached() {
        use crate::hddt::mock_portal::MockPortal;
        let portal = MockPortal::start().await;
        let client = HddtClient::for_test(&portal.base);
        let pool = test_pool().await;
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();

        let sum = scan(
            &pool,
            &client,
            "mock-token",
            &today,
            &today,
            &[InvoiceKind::Regular],
        )
        .await
        .expect("quét hôm nay phải chạy được");
        assert_eq!(sum.days_today, 1, "phải có 1 lượt quét hôm nay");
        assert_eq!(sum.days_scanned, 0, "hôm nay không tính vào ngày đã cache");
        assert_eq!(sum.invoices_new, 1, "hóa đơn hôm nay phải vào cache");
        let cached: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM hddt_sync_day")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(cached, 0, "hôm nay không được đánh dấu cache");
        assert_eq!(portal.count("/invoices/purchase"), 1, "phải gọi cổng 1 lần");

        // Lượt sau vẫn gọi lại hôm nay (không cache) nhưng không nhân bản hóa đơn.
        portal.reset();
        let sum2 = scan(
            &pool,
            &client,
            "mock-token",
            &today,
            &today,
            &[InvoiceKind::Regular],
        )
        .await
        .expect("quét lại hôm nay phải chạy được");
        assert_eq!(sum2.days_today, 1);
        assert_eq!(sum2.invoices_new, 0, "không tạo bản ghi trùng");
        assert_eq!(sum2.invoices_existing, 1, "hóa đơn đã cache thì bỏ qua");
        assert_eq!(
            portal.count("/invoices/purchase"),
            1,
            "vẫn phải gọi lại cổng"
        );
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
        sqlx::query!(
            "UPDATE hddt_purchase_invoice
                SET status = 'manual', detail_error = 'Too Many Requests', skip_reason = 'x'
              WHERE id = ?",
            id
        )
        .execute(&pool)
        .await
        .unwrap();

        let n = retry_failed_details(&pool).await.unwrap();
        assert_eq!(n, 1);
        let row = sqlx::query!(
            "SELECT status, detail_error FROM hddt_purchase_invoice WHERE id = ?",
            id
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.status, "pending");
        assert_eq!(row.detail_error, "");
    }

    /// Một lượt quét phải tự thử lại HĐ lỗi chi tiết của lượt trước (trước đây
    /// phải bấm "Quét cổng" hai lần mới lấy lại được dòng hàng).
    #[tokio::test]
    async fn scan_retries_previously_failed_details_in_one_pass() {
        use crate::hddt::mock_portal::MockPortal;
        let portal = MockPortal::start().await;
        let client = HddtClient::for_test(&portal.base);
        let pool = test_pool().await;
        let id = seed_invoice(&pool, "uuid-retry-scan", 1, "{}").await;
        sqlx::query!(
            "UPDATE hddt_purchase_invoice
                SET status = 'manual', detail_error = 'Too Many Requests', detail_json = ''
              WHERE id = ?",
            id
        )
        .execute(&pool)
        .await
        .unwrap();
        // Ngày đã có cache → lượt quét này không gọi danh sách, chỉ thử lại chi tiết.
        mark_days_scanned(
            &pool,
            NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 9, 24).unwrap(),
            InvoiceKind::Regular,
        )
        .await
        .unwrap();

        let sum = scan(
            &pool,
            &client,
            "mock-token",
            "2026-09-01",
            "2026-09-24",
            &[InvoiceKind::Regular],
        )
        .await
        .expect("quét mock thất bại");
        assert_eq!(sum.details_retried, 1, "phải đưa HĐ lỗi về hàng chờ ngay");
        assert_eq!(sum.details_ok, 1, "và lấy được dòng hàng ngay lượt này");
        let row = sqlx::query!(
            "SELECT status, detail_error, line_count FROM hddt_purchase_invoice WHERE id = ?",
            id
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.status, "pending");
        assert_eq!(row.detail_error, "");
        assert_eq!(row.line_count, 1, "phải có dòng hàng để nhập kho được");
    }

    #[tokio::test]
    async fn clear_cache_keeps_imported_invoices() {
        // Xoá cache để quét lại: chỉ dòng cache bị xoá; hóa đơn đã nhập kho nằm
        // ở bảng riêng nên giữ nguyên liên kết với phiếu.
        let pool = test_pool().await;
        let lines = r#"{"ten":"Hàng A","dvtinh":"Cái","mhhdvu":"a","sluong":1.0,
                        "dgia":100.0,"stckhau":0.0,"tsuat":0.08}"#;
        let done = seed_invoice(&pool, "uuid-keep", 1, lines).await;
        import_invoice(&pool, &load(&pool, done).await, "", "HKD", "", "")
            .await
            .unwrap();
        let pending = seed_invoice(&pool, "uuid-drop", 1, "{}").await;
        mark_days_scanned(
            &pool,
            NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(),
            InvoiceKind::Regular,
        )
        .await
        .unwrap();

        let out = clear_cache(&pool).await.unwrap();
        assert_eq!(out.invoices_deleted, 1, "chỉ xoá dòng cache");
        assert_eq!(out.days_deleted, 1);
        let left: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM hddt_imported_invoice WHERE portal_id = 'uuid-keep'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(left, 1, "hóa đơn đã nhập kho phải còn lại");
        let linked: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM hddt_imported_invoice hi
               JOIN inbound_voucher iv ON iv.id = hi.inbound_voucher_id
              WHERE hi.portal_id = 'uuid-keep'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(linked, 1, "liên kết hóa đơn ↔ phiếu phải nguyên vẹn");
        let gone: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM hddt_purchase_invoice WHERE id = ?",
            pending
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(gone, 0);
    }

    #[tokio::test]
    async fn service_line_keeps_full_name_verbatim() {
        // Tên dòng dịch vụ giữ nguyên (kể cả kỳ/tháng) → mỗi kỳ là một mặt hàng riêng.
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
        let p = sqlx::query!("SELECT name, unit, is_service FROM product")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(p.name, long_name, "tên dòng dịch vụ phải giữ nguyên");
        assert_eq!(p.unit, "Dịch vụ");
        assert_eq!(p.is_service, 1, "đánh dấu dịch vụ để không tính tồn kho");

        // Kỳ khác → mặt hàng mới.
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
        let n: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM product")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(n, 2);

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
        let amount: f64 = sqlx::query_scalar!(
            "SELECT amount FROM journal_entry WHERE entry_type = 'PN' AND product_code <> ''"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        // 2 × 2.860.000 − 2.860.000 CK = 2.860.000
        assert!(
            (amount - 2_860_000.0).abs() < 1.0,
            "giá trị nhập = thành tiền − tiền CK, thấy {}",
            amount
        );
        // Cột `discount` phải được GHI LẠI (không chỉ trừ vào amount): nếu mất ở
        // đây thì màn xem phiếu chỉ hiện thành tiền sau CK, không hiện chiết khấu.
        let discount: f64 = sqlx::query_scalar(
            "SELECT discount FROM journal_entry WHERE entry_type = 'PN' AND product_code <> ''",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            (discount - 2_860_000.0).abs() < 1.0,
            "journal_entry.discount phải là tiền CK, thấy {}",
            discount
        );
    }

    #[tokio::test]
    async fn backfill_restores_discount_lost_before_column_existed() {
        // Hóa đơn import trước 30/09/2026 được trừ CK vào `amount` nhưng chưa có
        // cột `discount` → màn xem phiếu hiện "Tiền CK —". Migration
        // 20261002001 phải dựng lại được số tiền CK từ detail_json.
        let pool = test_pool().await;
        let lines = r#"{"ten":"Aptomat ABN203c","dvtinh":"Cái","mhhdvu":null,"sluong":2.0,
                        "dgia":2860000.0,"stckhau":2860000.0,"tlckhau":50.0,"tsuat":0.08}"#;
        let id = seed_invoice(&pool, "uuid-ck-legacy", 1, lines).await;
        let out = import_invoice(&pool, &load(&pool, id).await, "", "HKD", "", "")
            .await
            .unwrap();

        // Mô phỏng đúng dữ liệu cũ: amount đã là SAU chiết khấu, discount = 0.
        sqlx::query("UPDATE journal_entry SET discount = 0 WHERE voucher_no = ?")
            .bind(&out.voucher_no)
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query(include_str!(
            "../../migrations/20261002001_backfill_hddt_discount.sql"
        ))
        .execute(&pool)
        .await
        .expect("migration backfill phải chạy được");

        let discount: f64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(discount), 0) FROM journal_entry
              WHERE voucher_no = ? AND entry_type = 'PN' AND product_code <> ''",
        )
        .bind(&out.voucher_no)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            (discount - 2_860_000.0).abs() < 1.0,
            "backfill phải dựng lại tiền CK 2.860.000, thấy {}",
            discount
        );

        // amount KHÔNG được đổi — backfill chỉ ghi thêm cột chiết khấu.
        let amount: f64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount), 0) FROM journal_entry
              WHERE voucher_no = ? AND entry_type = 'PN' AND product_code <> ''",
        )
        .bind(&out.voucher_no)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            (amount - 2_860_000.0).abs() < 1.0,
            "amount phải giữ nguyên là giá trị sau CK, thấy {}",
            amount
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
