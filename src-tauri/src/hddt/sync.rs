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

use std::time::Instant;

use chrono::{Datelike, Duration, NaiveDate};
use serde::Serialize;
use serde_json::Value;
use sqlx::{FromRow, SqlitePool};

use crate::commands::progress::ProgressJob;
use crate::hddt::{HddtClient, InvoiceKind, InvoiceQuery};
use crate::helpers::round2;
use crate::models::InboundItemInput;

/// Ngày cổng chấp nhận trong 1 request (HTTP 400 nếu vượt: "Khoảng thời gian
/// tìm kiếm không được lớn hơn 1 tháng").
const MAX_WINDOW_DAYS: i64 = 30;

/// Nhãn loại hóa đơn cho nhật ký quét cổng.
fn kind_label(kind: InvoiceKind) -> &'static str {
    match kind {
        InvoiceKind::Regular => "HĐ điện tử",
        InvoiceKind::CashRegister => "Máy tính tiền",
    }
}

/// Thời gian một request đã chạy — để nhìn ra hóa đơn/trang nào lag.
fn elapsed_label(started: Instant) -> String {
    let ms = started.elapsed().as_millis();
    if ms < 1_000 {
        format!("{ms} ms")
    } else {
        format!("{:.1} s", ms as f64 / 1_000.0)
    }
}

/// Lỗi cổng có thể rất dài (HTML/JSON); nhật ký chỉ giữ phần đầu đủ đọc.
fn brief_error(msg: &str) -> String {
    let one_line = msg.replace('\n', " ");
    let mut out: String = one_line.chars().take(180).collect();
    if one_line.chars().count() > 180 {
        out.push('…');
    }
    out
}

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
    /// `regular` | `cash-register` — chọn đúng tiền tố endpoint chi tiết.
    pub(crate) portal_kind: String,
    pub(crate) posting_date: String,
    pub(crate) nbmst: String,
    pub(crate) nbten: String,
    /// Mã mẫu số dạng số (1..9) — endpoint chi tiết BẮT BUỘC đúng field này.
    pub(crate) khmshdon: i64,
    pub(crate) khhdon: String,
    pub(crate) shdon: String,
    pub(crate) status: String,
    pub(crate) detail_json: String,
    /// Tổng tiền hàng trước thuế (theo hóa đơn) — dùng đối chiếu với tổng dòng.
    pub(crate) tgtcthue: f64,
    /// Tổng thuế GTGT của hóa đơn.
    pub(crate) tgtthue: f64,
    /// Tổng thanh toán cuối cùng — chuẩn để phân bổ ngược về từng dòng.
    pub(crate) tgtttbso: f64,
}

impl CachedInvoice {
    /// Tải chi tiết của **riêng** hóa đơn này từ cổng rồi ghi đè vào cache.
    ///
    /// Dùng cho thao tác từng hóa đơn (Xem PDF, Nhập kho) khi lượt quét trước
    /// chưa lấy được dòng hàng: chỉ gọi đúng 1 request thay vì bắt người dùng
    /// quét lại cả ngày. `khmshdon` phải là số — endpoint chi tiết của cổng trả
    /// HTTP 500 nếu gửi ký hiệu hóa đơn.
    pub(crate) async fn fetch_detail(
        &mut self,
        pool: &SqlitePool,
        client: &HddtClient,
        token: &str,
    ) -> Result<String, String> {
        if self.khmshdon <= 0 {
            return Err(
                "Thiếu mã mẫu số (khmshdon) — không tải được chi tiết, hãy quét lại hóa đơn từ cổng."
                    .into(),
            );
        }
        let detail = client
            .invoice_detail(
                token,
                InvoiceKind::parse(&self.portal_kind).unwrap_or_default(),
                &self.nbmst,
                &self.khmshdon.to_string(),
                &self.khhdon,
                &self.shdon,
            )
            .await?;
        let line_count = detail
            .get("hdhhdvu")
            .and_then(Value::as_array)
            .map(|a| a.len())
            .unwrap_or(0) as i64;
        // Hóa đơn đang 'manual' chỉ vì lỗi chi tiết thì nay đã có dòng hàng →
        // trả về 'pending' để nút Nhập kho dùng được. Hóa đơn manual vì lý do
        // khác (tthai != 1) có detail_error rỗng nên giữ nguyên.
        let reset = sqlx::query(
            "UPDATE hddt_purchase_invoice
                SET status = 'pending', skip_reason = ''
              WHERE id = ? AND status = 'manual' AND detail_error <> ''",
        )
        .bind(self.id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?
        .rows_affected()
            > 0;
        if reset {
            self.status = "pending".into();
        }
        let detail_json = detail.to_string();
        sqlx::query!(
            "UPDATE hddt_purchase_invoice
                SET detail_json = ?, line_count = ?, detail_error = ''
              WHERE id = ?",
            detail_json,
            line_count,
            self.id
        )
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
        Ok(detail_json)
    }
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
    /// Lỗi của từng loại/khung ngày. Lượt quét vẫn tiếp tục các phần khác —
    /// một endpoint (thường là `sco-query` máy tính tiền) chết không được làm
    /// mất toàn bộ hóa đơn điện tử đã lấy được.
    pub(crate) errors: Vec<String>,
}

/// Quét một cửa sổ ngày (đã gom ≤ 30 ngày) rồi ghi cache từng hóa đơn. Việc đánh
/// dấu "ngày đã quét" do caller quyết định — hôm nay thì cố ý không đánh dấu.
#[allow(clippy::too_many_arguments)] // đủ ngữ cảnh 1 request; gom struct chỉ để vừa lint
async fn scan_window(
    pool: &SqlitePool,
    client: &HddtClient,
    token: &str,
    kind: InvoiceKind,
    win_start: NaiveDate,
    win_end: NaiveDate,
    sum: &mut ScanSummary,
    job: Option<&ProgressJob>,
) -> Result<(), String> {
    let day_range = format!(
        "{}–{}",
        win_start.format("%d/%m/%Y"),
        win_end.format("%d/%m/%Y")
    );
    let mut state: Option<String> = None;
    let mut page = 0usize;
    loop {
        page += 1;
        let q = InvoiceQuery {
            direction: crate::hddt::InvoiceDirection::Purchase,
            kind,
            // HĐ điện tử: cổng từ chối size > 50 → lấy tối đa 50/lần cho ít
            // request. Máy tính tiền (`sco-query`): dùng đúng 15 như trang tra
            // cứu của cổng, tránh backend treo với trang lớn.
            size: match kind {
                InvoiceKind::Regular => crate::hddt::MAX_PAGE_SIZE,
                InvoiceKind::CashRegister => crate::hddt::CASH_REGISTER_PAGE_SIZE,
            },
            state: state.clone(),
            from: Some(iso_to_portal_day(&win_start.to_string())),
            to: Some(iso_to_portal_day(&win_end.to_string())),
            // Tất cả kết quả kiểm tra: bỏ lọc để không bỏ sót HĐ.
            ttxly: Some("-1".into()),
            ..Default::default()
        };
        if let Some(j) = job {
            j.step(format!(
                "Đang lấy danh sách {} {} (trang {page})…",
                kind_label(kind),
                day_range
            ));
        }
        let started = Instant::now();
        let list = match client.query_invoices(token, &q).await {
            Ok(v) => v,
            Err(e) => {
                if let Some(j) = job {
                    j.log(format!(
                        "✗ Danh sách {} {} trang {page} lỗi sau {}: {}",
                        kind_label(kind),
                        day_range,
                        elapsed_label(started),
                        brief_error(&e)
                    ));
                }
                return Err(e);
            }
        };
        let rows = list.datas.len();
        for row in &list.datas {
            if upsert_invoice(pool, row, kind).await? {
                sum.invoices_new += 1;
            } else {
                sum.invoices_existing += 1;
            }
        }
        if let Some(j) = job {
            j.log(format!(
                "Danh sách {} {} trang {page}: {rows} HĐ — {}",
                kind_label(kind),
                day_range,
                elapsed_label(started)
            ));
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
    scan_with_progress(pool, client, token, from, to, kinds, None).await
}

/// Như [`scan`] nhưng ghi tiến độ/nhật ký vào hộp thoại của `job` (nếu có).
///
/// Việc chạy nền không có hộp thoại → `job = None`, không đụng tới ô tiến độ
/// chung. Nhật ký ghi cả thời gian từng request để biết trang/hóa đơn nào lag.
pub(crate) async fn scan_with_progress(
    pool: &SqlitePool,
    client: &HddtClient,
    token: &str,
    from: &str,
    to: &str,
    kinds: &[InvoiceKind],
    job: Option<&ProgressJob>,
) -> Result<ScanSummary, String> {
    // LƯU Ý: việc thử lại hóa đơn lỗi chi tiết được làm Ở DƯỚI (ngay trước khi
    // tải chi tiết), KHÔNG phải trước khi gọi danh sách. Nếu reset sớm mà lượt
    // quét chết vì lỗi mạng, hóa đơn bị bỏ trạng thái 'manual' + xoá detail_error
    // nhưng detail_json vẫn rỗng → hiện "Chờ nhập kho" và bấm Nhập kho sẽ nổ
    // "Dữ liệu chi tiết hóa đơn hỏng: EOF while parsing…".
    let mut sum = ScanSummary::default();

    let today = chrono::Local::now().date_naive();
    let start_date = resolve_start(pool, from).await?;
    let end_date = resolve_end(to).await?;
    if let Some(j) = job {
        j.log(format!(
            "Bắt đầu quét cổng HĐĐT: {} → {}",
            start_date.format("%d/%m/%Y"),
            end_date.format("%d/%m/%Y")
        ));
    }
    if start_date > end_date {
        // Mốc bắt đầu dùng HĐĐT được phép nằm TRONG TƯƠNG LAI (Cài đặt thông tin
        // HKD nhận bất kỳ ngày nào) — tới mốc đó mới có gì để quét, nên im lặng
        // trả về không ngày nào thay vì báo lỗi khoảng ngày.
        if start_date > today {
            if let Some(j) = job {
                j.log(format!(
                    "Mốc bắt đầu {} ở tương lai — chưa có hóa đơn nào để quét",
                    start_date.format("%d/%m/%Y")
                ));
            }
            return Ok(sum);
        }
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
        if let Some(j) = job {
            j.log(format!(
                "{}: {} ngày cần quét · {} ngày đã có cache",
                kind_label(*kind),
                missing.len(),
                all_days.len() - missing.len()
            ));
        }

        // Hôm nay: lấy để thấy hóa đơn vừa phát sinh, nhưng KHÔNG đánh dấu cache
        // để lượt quét sau vẫn lấy lại được.
        let mut kind_failed = false;
        if end_date >= today {
            if let Some(j) = job {
                j.plan(1);
                j.log(format!("Quét {} hôm nay (không cache)…", kind_label(*kind)));
            }
            let started = Instant::now();
            match scan_window(pool, client, token, *kind, today, today, &mut sum, job).await {
                Ok(()) => {
                    sum.days_today += 1;
                    if let Some(j) = job {
                        j.tick(format!(
                            "Xong {} hôm nay — {}",
                            kind_label(*kind),
                            elapsed_label(started)
                        ));
                    }
                }
                Err(e) => {
                    // Cổng có thể chết riêng một endpoint (thường `sco-query`
                    // máy tính tiền). Đừng thử lại endpoint đó cho từng khung
                    // ngày — mỗi lần 4 lượt × 20 s sẽ treo cả buổi.
                    let msg = format!(
                        "{} hôm nay {}: {}",
                        kind_label(*kind),
                        today.format("%d/%m/%Y"),
                        brief_error(&e)
                    );
                    if let Some(j) = job {
                        j.log(format!("✗ {msg} — bỏ qua phần còn lại của loại này"));
                    }
                    sum.errors.push(msg);
                    kind_failed = true;
                }
            }
        }
        if kind_failed || missing.is_empty() {
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
            if let Some(p) = job {
                p.plan(1);
                p.log(format!(
                    "Quét {} {}–{} ({} ngày)…",
                    kind_label(*kind),
                    win_start.format("%d/%m/%Y"),
                    win_end.format("%d/%m/%Y"),
                    days
                ));
            }
            let started = Instant::now();
            match scan_window(
                pool, client, token, *kind, win_start, win_end, &mut sum, job,
            )
            .await
            {
                Ok(()) => {
                    // Đánh dấu ngày đã quét cho loại này (kể cả 0 hóa đơn).
                    sum.days_scanned += days as usize;
                    mark_days_scanned(pool, win_start, win_end, *kind).await?;
                    if let Some(p) = job {
                        p.tick(format!(
                            "Xong {} {}–{} ({} ngày) — {}",
                            kind_label(*kind),
                            win_start.format("%d/%m/%Y"),
                            win_end.format("%d/%m/%Y"),
                            days,
                            elapsed_label(started)
                        ));
                    }
                }
                Err(e) => {
                    let msg = format!(
                        "{} {}–{}: {}",
                        kind_label(*kind),
                        win_start.format("%d/%m/%Y"),
                        win_end.format("%d/%m/%Y"),
                        brief_error(&e)
                    );
                    if let Some(p) = job {
                        p.log(format!("✗ {msg} — bỏ qua phần còn lại của loại này"));
                    }
                    sum.errors.push(msg);
                    break;
                }
            }
            i = j + 1;
        }
    }

    // Không lấy được bất kỳ danh sách nào (toàn bộ endpoint đều lỗi) → trả lỗi
    // để UI báo đúng. Quan trọng: thoát TRƯỚC khi thử lại chi tiết, vì lượt quét
    // chết vì mạng không được reset trạng thái lỗi chi tiết của lượt trước.
    if sum.days_scanned == 0 && sum.days_today == 0 && !sum.errors.is_empty() {
        return Err(format!(
            "Không quét được cổng HĐĐT: {}",
            sum.errors.join(" · ")
        ));
    }

    // Hóa đơn lượt trước lỗi chi tiết (429, mạng chập chờn…) → thử lại ngay
    // trong lượt quét này, không bắt người dùng phải bấm "Quét cổng" hai lần.
    // Đặt Ở ĐÂY (sau khi danh sách đã gọi xong): lượt quét chết vì mạng ở trên
    // không được phép reset trạng thái hóa đơn.
    sum.details_retried = retry_failed_details(pool).await?;
    if let Some(j) = job {
        if sum.details_retried > 0 {
            j.log(format!(
                "Thử lại chi tiết {} hóa đơn lỗi từ lượt trước",
                sum.details_retried
            ));
        }
    }

    // Lấy chi tiết cho các hóa đơn chưa có (đủ dòng hàng mới tạo được phiếu).
    let pending = pending_without_detail(pool).await?;
    let n = pending.len();
    if let Some(j) = job {
        j.plan(n);
        if n == 0 {
            j.log("Không có hóa đơn nào cần tải chi tiết");
        } else {
            j.log(format!("Cần tải chi tiết {n} hóa đơn"));
        }
    }
    for (idx, inv) in pending.into_iter().enumerate() {
        let inv_label = format!("{} {}", inv.khhdon, inv.shdon);
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
            if let Some(j) = job {
                j.tick(format!(
                    "✗ Chi tiết {}/{} {inv_label}: thiếu mã mẫu số",
                    idx + 1,
                    n
                ));
            }
            continue;
        }
        if let Some(j) = job {
            j.step(format!(
                "Đang tải chi tiết {}/{} — {inv_label}…",
                idx + 1,
                n
            ));
        }
        let started = Instant::now();
        match client
            .invoice_detail(
                token,
                InvoiceKind::parse(&inv.portal_kind).unwrap_or_default(),
                &inv.nbmst,
                &inv.khmshdon.to_string(),
                &inv.khhdon,
                &inv.shdon,
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
                if let Some(j) = job {
                    j.tick(format!(
                        "Chi tiết {}/{} {inv_label}: {lines} dòng — {}",
                        idx + 1,
                        n,
                        elapsed_label(started)
                    ));
                }
            }
            Err(msg) => {
                let note = brief_error(&msg);
                sqlx::query!(
                    "UPDATE hddt_purchase_invoice SET detail_error = ? WHERE id = ?",
                    msg,
                    inv.id
                )
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
                sum.details_failed += 1;
                if let Some(j) = job {
                    j.tick(format!(
                        "✗ Chi tiết {}/{} {inv_label}: {note} — {}",
                        idx + 1,
                        n,
                        elapsed_label(started)
                    ));
                }
            }
        }
    }

    // Gán cờ cần người xử lý sau khi đã biết chi tiết.
    sum.need_manual = mark_manual_cases(pool).await?;
    if let Some(j) = job {
        j.log(format!(
            "Quét xong: {} HĐ mới · {} chi tiết OK · {} lỗi · {} cần xử lý",
            sum.invoices_new, sum.details_ok, sum.details_failed, sum.need_manual
        ));
    }
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

/// Đọc 1 hóa đơn cache theo `portal_id` — dùng cho thao tác từng hóa đơn
/// (Xem PDF, Nhập kho) khi frontend chỉ có id của cổng.
pub(crate) async fn cached_by_portal_id(
    pool: &SqlitePool,
    portal_id: &str,
) -> Result<Option<CachedInvoice>, String> {
    sqlx::query_as::<_, CachedInvoice>(
        r#"SELECT id, portal_id, portal_kind, posting_date, nbmst, nbten,
                  COALESCE(khmshdon, 0) AS khmshdon, khhdon, shdon, status, detail_json,
                  tgtcthue, tgtthue, tgtttbso
             FROM hddt_purchase_invoice
            WHERE portal_id = ?"#,
    )
    .bind(portal_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())
}

/// Hóa đơn chờ xử lý mà chưa có chi tiết (và lần gọi trước không lỗi).
async fn pending_without_detail(pool: &SqlitePool) -> Result<Vec<CachedInvoice>, String> {
    let rows: Vec<CachedInvoice> = sqlx::query_as!(
        CachedInvoice,
        r#"SELECT id as "id!", portal_id, portal_kind, posting_date, nbmst, nbten,
                  khmshdon as "khmshdon!", khhdon, shdon, status, detail_json,
                  tgtcthue, tgtthue, tgtttbso
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

/// Bỏ dấu "ngày đã quét" + xoá cache hóa đơn CỦA NGÀY ĐÓ để lần quét tới tải
/// lại từ cổng — hóa đơn xuất hiện SAU khi ngày đó đã quét (đã ký trễ, cổng
/// cập nhật chậm) trước đây bị bỏ sót vĩnh viễn vì cache theo ngày. Hóa đơn
/// đã nhập kho nằm ở `hddt_imported_invoice` nên không bị đụng tới.
pub(crate) async fn uncache_day(pool: &SqlitePool, day: &str) -> Result<(usize, usize), String> {
    let invoices = sqlx::query!(
        "DELETE FROM hddt_purchase_invoice WHERE posting_date = ?",
        day
    )
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?
    .rows_affected();
    let days = sqlx::query!("DELETE FROM hddt_sync_day WHERE day = ?", day)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?
        .rows_affected();
    Ok((invoices as usize, days as usize))
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
    /// true = ghi đè lên phiếu nhập tay đã khớp, false = tạo phiếu mới.
    pub(crate) updated: bool,
    pub(crate) message: String,
    pub(crate) voucher_no: String,
    pub(crate) products_created: usize,
}

/// Phân bổ `total` theo trọng số `weights`, làm tròn 2 chữ số, dồn phần lệch
/// vào dòng có trọng số lớn nhất để tổng khớp tuyệt đối (tránh lệch 1–2 đồng
/// khi cộng từng dòng).
fn allocate_amount(total: f64, weights: &[f64]) -> Vec<f64> {
    let n = weights.len();
    if n == 0 {
        return Vec::new();
    }
    let abs_sum: f64 = weights.iter().map(|w| w.abs()).sum();
    if abs_sum < 1e-12 {
        let each = round2(total / n as f64);
        let mut out = vec![each; n];
        let diff = round2(total - out.iter().sum::<f64>());
        if diff.abs() >= 0.005 {
            out[0] = round2(out[0] + diff);
        }
        return out;
    }
    let mut out: Vec<f64> = weights
        .iter()
        .map(|w| round2(total * w.abs() / abs_sum))
        .collect();
    let diff = round2(total - out.iter().sum::<f64>());
    if diff.abs() >= 0.005 {
        let idx = weights
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.abs().total_cmp(&b.abs()))
            .map(|(i, _)| i)
            .unwrap_or(0);
        out[idx] = round2(out[idx] + diff);
    }
    out
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
            updated: false,
            message: format!("Đã nhập kho trước đó — phiếu {voucher_no}"),
            voucher_no,
            products_created: 0,
        });
    }

    // Chưa tải được dòng hàng (lượt quét trước lỗi mạng giữa chừng) → báo câu
    // người dùng hiểu được kèm ngày để quét lại. Tuyệt đối không để lộ lỗi serde
    // "EOF while parsing a value at line 1 column 0".
    if inv.detail_json.trim().is_empty() {
        return Err(format!(
            "Hóa đơn {} {} chưa tải được dòng hàng (ngày {}). Bấm \"Quét cổng\" hoặc \
             \"Quét lại ngày này\" để tải lại chi tiết rồi nhập kho.",
            inv.khhdon, inv.shdon, inv.posting_date
        ));
    }
    let detail: Value = serde_json::from_str(&inv.detail_json).map_err(|e| {
        format!(
            "Dữ liệu chi tiết hóa đơn {} {} hỏng: {e}. Bấm \"Quét lại ngày này\" để \
             tải lại chi tiết rồi nhập kho.",
            inv.khhdon, inv.shdon
        )
    })?;
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
        // dgia của HĐĐT là đơn giá CHƯA thuế; stckhau là tiền chiết khấu trên
        // dòng. Tính đúng như save_inbound_core (CK không vượt quá thành tiền).
        let quantity = num(line, "sluong");
        let unit_price = num(line, "dgia");
        let discount = num(line, "stckhau");
        // Thuế suất của RIÊNG dòng hàng — hóa đơn có thể trộn 8%/10%/KCT.
        let line_vat_rate = num(line, "tsuat");
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
                let portal_code = f(line, "mhhdvu");
                sqlx::query!(
                    "INSERT INTO product (code, name, unit, sale_price, cost_price,
                                          min_stock, vat_rate, import_tax_rate, is_service,
                                          industry_code, identity_key, portal_code)
                     VALUES (?, ?, ?, 0, ?, 0, ?, ?, ?, ?, ?, ?)",
                    code,
                    name,
                    unit,
                    cost,
                    line_vat_rate,
                    line_vat_rate,
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
            quantity,
            unit_price,
            // Chiết khấu: `stckhau` là SỐ TIỀN CK trên dòng (đã kiểm chứng
            // live 24/09/2026: tlckhau = 50.0 là TỶ LỆ %, stckhau = 2.860.000
            // là tiền CK, dùng cho dòng 2 × 2.860.000). Sổ của app cần số tiền.
            discount,
            vat_rate: Some(line_vat_rate),
            vat_amount: None,
        });
    }
    if items.is_empty() {
        return Err("Hóa đơn không có dòng hàng hợp lệ".into());
    }

    // ─── Đối chiếu tổng hóa đơn → phân bổ xuống từng dòng ───
    // Giá trị hàng trước thuế theo hóa đơn = tổng thanh toán − tổng thuế. Nếu
    // hóa đơn có chiết khấu cấp hóa đơn / làm tròn, phần lệch được phân bổ
    // xuống dòng để Σ giá trị hàng khớp tuyệt đối với hóa đơn.
    let net_of = |it: &InboundItemInput| -> f64 {
        let gross = round2(it.quantity * it.unit_price);
        let discount = if gross >= 0.0 {
            it.discount.clamp(0.0, gross)
        } else {
            it.discount
        };
        round2(gross - round2(discount))
    };
    let line_net_total = round2(items.iter().map(net_of).sum());
    let base_total = if inv.tgtttbso.abs() > 0.0 {
        round2(inv.tgtttbso - inv.tgtthue)
    } else if inv.tgtcthue.abs() > 0.0 {
        round2(inv.tgtcthue)
    } else {
        line_net_total
    };
    if base_total > 0.0 && line_net_total > 0.0 {
        let extra = round2(line_net_total - base_total);
        if extra.abs() >= 0.005 {
            if extra > 0.0 {
                // Giảm giá cấp hóa đơn (ttcktmai) / làm tròn: phân bổ theo giá
                // trị từng dòng (sau CK dòng) để dòng lớn chịu nhiều hơn.
                let weights: Vec<f64> = items.iter().map(|it| net_of(it).max(0.0)).collect();
                let alloc = allocate_amount(extra, &weights);
                for (it, add) in items.iter_mut().zip(alloc) {
                    let gross = round2(it.quantity * it.unit_price).max(0.0);
                    it.discount = round2((it.discount + add).clamp(0.0, gross));
                }
            } else {
                // Hóa đơn cao hơn tổng dòng (làm tròn / phụ phí): giảm chiết
                // khấu trước, còn thiếu thì cộng vào đơn giá dòng lớn nhất.
                let need = -extra;
                let disc_total: f64 = items.iter().map(|it| it.discount.max(0.0)).sum();
                if disc_total > 0.0 {
                    let weights: Vec<f64> = items.iter().map(|it| it.discount.max(0.0)).collect();
                    let alloc = allocate_amount(need.min(disc_total), &weights);
                    for (it, sub) in items.iter_mut().zip(alloc) {
                        it.discount = round2((it.discount - sub).max(0.0));
                    }
                }
            }
            // Dồn phần lệch còn lại vào chiết khấu (hoặc đơn giá) dòng lớn nhất.
            let now = round2(items.iter().map(net_of).sum());
            let diff = round2(base_total - now);
            if diff.abs() >= 0.005 {
                if let Some(it) = items
                    .iter_mut()
                    .filter(|it| it.quantity.abs() > 1e-9)
                    .max_by(|a, b| net_of(a).abs().total_cmp(&net_of(b).abs()))
                {
                    let gross = round2(it.quantity * it.unit_price).max(0.0);
                    let new_discount = round2(it.discount - diff);
                    if new_discount >= -0.005 && new_discount <= gross + 0.005 {
                        it.discount = new_discount.clamp(0.0, gross);
                    } else {
                        it.unit_price = round2(it.unit_price + diff / it.quantity);
                    }
                }
            }
        }
    }

    // Phân bổ tổng thuế của hóa đơn xuống từng dòng theo giá trị hàng. Dùng số
    // tuyệt đối (`vat_amount`) để tổng 133 khớp `tgtthue` dù làm tròn từng dòng.
    let net_after: Vec<f64> = items.iter().map(net_of).collect();
    let line_vat_calc = round2(
        items
            .iter()
            .map(|it| round2(net_of(it) * it.vat_rate.unwrap_or(0.0)))
            .sum(),
    );
    let target_vat = if inv.tgtttbso.abs() > 0.0 {
        round2(inv.tgtthue)
    } else {
        line_vat_calc
    };
    // Phân bổ theo SỐ THUẾ TÍNH ĐƯỢC CỦA TỪNG DÒNG (net × thuế suất), không
    // theo giá trị hàng — hóa đơn trộn 8%/10% phải giữ đúng thuế từng dòng.
    let vat_weights: Vec<f64> = items
        .iter()
        .map(|it| round2(net_of(it) * it.vat_rate.unwrap_or(0.0)))
        .collect();
    let vat_alloc = if vat_weights.iter().any(|w| w.abs() > 0.005) {
        allocate_amount(target_vat, &vat_weights)
    } else {
        allocate_amount(target_vat, &net_after)
    };
    for (it, vat) in items.iter_mut().zip(vat_alloc) {
        it.vat_amount = Some(vat);
    }

    // 2) Nhà cung cấp: khớp theo MST, thiếu thì tạo (mã = MST để đối chiếu).
    let supplier_code = ensure_supplier(pool, &inv.nbmst, &inv.nbten).await?;

    // 3) Khớp phiếu nhập tay đã có (cùng ký hiệu + số + ngày) để ghi đè.
    // Không so MST người bán — số hiệu + ngày là đủ theo yêu cầu.
    let matched = crate::commands::stock::find_matching_inbound(
        pool,
        &inv.khhdon,
        &inv.shdon,
        &inv.posting_date,
    )
    .await?;
    let overwrite = matched.is_some();
    let voucher_no = match &matched {
        Some(m) => m.voucher_no.clone(),
        None => next_inbound_voucher_no(pool).await?,
    };
    let effective_warehouse = match &matched {
        Some(m) if warehouse_code.trim().is_empty() => m.warehouse_code.clone(),
        _ => warehouse_code.to_string(),
    };

    // 4) Ghi phiếu trong CÙNG transaction với liên kết hóa đơn + xoá cache.
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
    let extra = crate::commands::stock::InboundExtra {
        source: "hddt",
        invoice_serial: &inv.khhdon,
        invoice_no: &inv.shdon,
        invoice_date: &inv.posting_date,
        overwrite,
        exclude_id: matched.as_ref().map(|m| m.id),
    };
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    if let Some(m) = &matched {
        crate::commands::stock::clear_inbound_for_overwrite(&mut tx, m).await?;
    }
    let result = crate::commands::stock::save_inbound_core_tx(
        &mut tx,
        &inv.posting_date,
        &voucher_no,
        &description,
        &supplier_code,
        &effective_warehouse,
        if unit_code.is_empty() {
            "HKD"
        } else {
            unit_code
        },
        &items,
        "",
        "purchase",
        &reference_no,
        0.0, // thuế lấy theo từng dòng / vat_amount
        debit_account,
        credit_account,
        false, // không tự tạo phiếu chi — thanh toán do người dùng quyết định
        "",
        false, // không tự xuất NVL theo định mức — HĐĐT nhập là mua ngoài
        &extra,
    )
    .await?;

    // 4b) Phiếu chi "trả tiền ngay" cũ (nếu có) phải theo tổng mới.
    if overwrite {
        let pay = round2(result.total + result.vat);
        let pc_note = format!("Theo chứng từ: {}", voucher_no);
        sqlx::query!(
            "UPDATE journal_entry SET unit_price = ?, amount = ?
              WHERE entry_type = 'PC' AND note = ?",
            pay,
            pay,
            pc_note
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    }

    // 5) Hóa đơn chính thức + liên kết chặt với phiếu nhập (FK thật). Dữ liệu
    // này nằm ở bảng riêng `hddt_imported_invoice`, sau đó dòng cache bị xoá.
    let header_id = result.inbound_voucher_id;
    let supplier_id: Option<i64> = sqlx::query_scalar!(
        r#"SELECT id as "id!" FROM supplier WHERE code = ?"#,
        supplier_code
    )
    .fetch_optional(&mut *tx)
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
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    // Cache đã "tiêu" rồi → xoá khỏi cache, dữ liệu chính thức nằm ở bảng trên.
    sqlx::query!("DELETE FROM hddt_purchase_invoice WHERE id = ?", inv.id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;

    Ok(ImportOutcome {
        portal_id: inv.portal_id.clone(),
        ok: true,
        updated: overwrite,
        message: if overwrite {
            format!(
                "Đã cập nhật phiếu {} theo hóa đơn · {} dòng · tổng {}",
                voucher_no, result.entries, result.total
            )
        } else {
            format!(
                "Đã tạo phiếu {} · {} dòng · {} hàng mới · tổng {}",
                voucher_no, result.entries, created, result.total
            )
        },
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
    /// Mã mẫu số — cần cho nút "Tải XML" từng dòng (endpoint export-xml).
    pub(crate) khmshdon: i64,
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
        "SELECT id, portal_id, posting_date, nbmst, nbten, khhdon, shdon,
                COALESCE(khmshdon, 0) AS khmshdon, portal_kind,
                status, skip_reason, line_count, new_product_count,
                tgtcthue, tgtthue, ttcktmai, tgtttbso, voucher_no, detail_error
           FROM hddt_purchase_invoice
          WHERE (? = '' OR posting_date >= ?) AND (? = '' OR posting_date <= ?)
         UNION ALL
         SELECT 0, hi.portal_id, hi.posting_date, hi.nbmst, hi.nbten, hi.khhdon, hi.shdon,
                COALESCE(hi.khmshdon, 0), hi.portal_kind, 'imported', '', hi.line_count,
                hi.new_product_count, hi.tgtcthue, hi.tgtthue, hi.ttcktmai, hi.tgtttbso,
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
    use crate::commands::stock::{save_inbound_core_with, InboundExtra};
    use crate::test_support::{seed_product, set_vat_deduct, test_pool};
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Số hóa đơn tăng dần: mỗi test có thể seed nhiều hóa đơn, không được trùng
    /// bộ ký hiệu + số + ngày (trùng là cùng 1 hóa đơn, import sẽ chặn).
    static SEED_SEQ: AtomicU64 = AtomicU64::new(1);

    /// Chèn 1 hóa đơn cache + chi tiết giả, chạy import, kiểm tra chuỗi liên kết.
    async fn seed_invoice(pool: &SqlitePool, portal_id: &str, tthai: i64, lines: &str) -> i64 {
        let status = if tthai == 1 { "pending" } else { "manual" };
        let parsed: Value = serde_json::from_str(&format!("[{lines}]")).unwrap();
        let arr = parsed.as_array().unwrap();
        let line_count = arr.len() as i64;
        // Tổng hóa đơn phải khớp chính các dòng — import đối chiếu `tgtttbso`
        // với tổng dòng và phân bổ phần lệch xuống từng dòng.
        let mut tgtcthue = 0.0;
        let mut tgtthue = 0.0;
        for line in arr {
            let quantity = num(line, "sluong");
            let unit_price = num(line, "dgia");
            let discount = num(line, "stckhau");
            let gross = quantity * unit_price;
            let net = gross - discount.clamp(0.0, gross);
            tgtcthue += net;
            tgtthue += net * num(line, "tsuat");
        }
        let tgtcthue = round2(tgtcthue);
        let tgtthue = round2(tgtthue);
        let tgtttbso = round2(tgtcthue + tgtthue);
        let shdon = format!("{:03}", SEED_SEQ.fetch_add(1, Ordering::Relaxed));
        let detail = format!("{{\"hdhhdvu\":[{lines}]}}");
        sqlx::query!(
            "INSERT INTO hddt_purchase_invoice
                (portal_id, portal_kind, tdlap, posting_date, nbmst, nbten, nmmst,
                 khmshdon, khhdon, shdon, hthdon, tchat, tgtcthue, tgtthue, ttcktmai,
                 tgtttbso, status, skip_reason, line_count, raw_json, detail_json)
             VALUES (?, 'regular', '2026-09-01T17:00:00Z', '2026-09-02', '0106773786',
                     'Cty TNHH ABC', '001170019085', 1, 'C26ABC', ?, 1, 1,
                     ?, ?, 0, ?, ?, '', ?, '{}', ?)",
            portal_id,
            shdon,
            tgtcthue,
            tgtthue,
            tgtttbso,
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
            r#"SELECT id as "id!", portal_id, portal_kind, posting_date, nbmst, nbten,
                      khmshdon as "khmshdon!", khhdon, shdon, status, detail_json,
                  tgtcthue, tgtthue, tgtttbso
                 FROM hddt_purchase_invoice WHERE id = ?"#,
            id
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    /// Quét lại ngày: xoá cache hóa đơn + dấu "ngày đã quét" CỦA NGÀY ĐÓ, giữ
    /// nguyên ngày khác (hóa đơn ngày kế không bị đụng tới).
    #[tokio::test]
    async fn uncache_day_xoa_cache_va_dau_ngay_cua_ngay_do() {
        let pool = test_pool().await;
        sqlx::query!(
            "INSERT INTO hddt_sync_day (day, kind, scanned_at, invoice_count)
             VALUES ('2026-09-02', 'purchase', '2026-09-02T10:00:00Z', 2),
                    ('2026-09-03', 'purchase', '2026-09-03T10:00:00Z', 1)"
        )
        .execute(&pool)
        .await
        .unwrap();
        let line = r#"{"ten":"Hàng test","dvtinh":"Cái","mhhdvu":"t1","sluong":1.0,
                       "dgia":1000.0,"stckhau":0.0,"tsuat":0.01}"#;
        // 2 hóa đơn ngày 02 (seed_invoice ghi cứng posting_date '2026-09-02').
        let _a = seed_invoice(&pool, "uuid-a", 1, line).await;
        let _b = seed_invoice(&pool, "uuid-b", 1, line).await;
        // 1 hóa đơn ngày 03.
        let c = seed_invoice(&pool, "uuid-c", 1, line).await;
        sqlx::query!(
            "UPDATE hddt_purchase_invoice SET posting_date = '2026-09-03' WHERE id = ?",
            c
        )
        .execute(&pool)
        .await
        .unwrap();

        let (invoices, days) = uncache_day(&pool, "2026-09-02").await.unwrap();
        assert_eq!((invoices, days), (2, 1), "xoá 2 hóa đơn cache + 1 dấu ngày");

        let left: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM hddt_purchase_invoice WHERE posting_date = '2026-09-02'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(left, 0, "cache ngày đã quét bị xoá hết");
        let day_mark: i64 =
            sqlx::query_scalar!("SELECT COUNT(*) FROM hddt_sync_day WHERE day = '2026-09-02'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(day_mark, 0, "dấu ngày đã quét bị bỏ");
        let other: i64 =
            sqlx::query_scalar!("SELECT COUNT(*) FROM hddt_sync_day WHERE day = '2026-09-03'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(other, 1, "ngày khác vẫn giữ dấu đã quét");
        let kept: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM hddt_purchase_invoice WHERE posting_date = '2026-09-03'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(kept, 1, "hóa đơn ngày khác không bị đụng");
        // Lần quét sau sẽ thử tải lại ngày 02 từ cổng.
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
        assert_eq!(header.source, "hddt"); // phiếu tạo từ đồng bộ cổng
        assert!(!out.updated, "hóa đơn mới → tạo phiếu, không ghi đè");

        // Hộ mặc định KHÔNG khấu trừ: đơn giá HĐĐT là giá chưa thuế, thuế 8%
        // phải được cộng vào giá nhập kho (2 × 1.480.000 × 1,08).
        assert_eq!(
            header.total, 3_196_800.0,
            "giá nhập kho phải gồm thuế khi hộ không khấu trừ"
        );
        let vat133: f64 = sqlx::query_scalar(
            "SELECT CAST(COALESCE(SUM(amount), 0) AS REAL) FROM journal_entry
             WHERE voucher_no = ? AND debit_account = '133'",
        )
        .bind(&out.voucher_no)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(vat133, 0.0, "không khấu trừ thì không sinh bút toán 133");
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

    /// Hộ BẬT khấu trừ GTGT: thuế trên HĐĐT được tách vào 133, giá nhập kho
    /// (và giá vốn FIFO) là giá chưa thuế.
    #[tokio::test]
    async fn import_tach_thue_133_khi_bat_khau_tru() {
        let pool = test_pool().await;
        set_vat_deduct(&pool, true).await;

        let lines = r#"{"ten":"Bình NN Rossi","dvtinh":"Cái","mhhdvu":"puro30SL","sluong":2.0,
                        "dgia":1480000.0,"stckhau":0.0,"tsuat":0.08}"#;
        let id = seed_invoice(&pool, "uuid-vat", 1, lines).await;
        let out = import_invoice(&pool, &load(&pool, id).await, "", "HKD", "", "")
            .await
            .expect("import thất bại");
        assert!(out.ok);

        let (total, vat_amount): (f64, f64) =
            sqlx::query_as("SELECT total, vat_amount FROM inbound_voucher WHERE voucher_no = ?")
                .bind(&out.voucher_no)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(total, 2_960_000.0, "giá nhập kho chưa thuế khi khấu trừ");
        assert_eq!(vat_amount, 236_800.0, "thuế tách riêng vào 133");

        let vat133: f64 = sqlx::query_scalar(
            "SELECT CAST(COALESCE(SUM(amount), 0) AS REAL) FROM journal_entry
             WHERE voucher_no = ? AND debit_account = '133'",
        )
        .bind(&out.voucher_no)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(vat133, 236_800.0);

        // Giá vốn FIFO + giá vốn mặc định không gồm thuế.
        let unit_cost: f64 = sqlx::query_scalar("SELECT unit_cost FROM stock_lot")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(unit_cost, 1_480_000.0);
        let cost_price: f64 = sqlx::query_scalar("SELECT cost_price FROM product")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(cost_price, 1_480_000.0);
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
            r#"SELECT id as "id!", portal_id, portal_kind, posting_date, nbmst, nbten,
                      khmshdon as "khmshdon!", khhdon, shdon, status, detail_json,
                  tgtcthue, tgtthue, tgtttbso
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

    /// Lượt quét chết vì lỗi mạng KHÔNG được reset hóa đơn lỗi chi tiết về
    /// "Chờ nhập kho": detail_json vẫn rỗng nên bấm Nhập kho sẽ nổ EOF parse.
    #[tokio::test]
    async fn scan_network_error_keeps_failed_details_intact() {
        use crate::hddt::mock_portal::MockPortal;
        let portal = MockPortal::start().await;
        // Cắt kết nối mọi request → client hết lượt thử và trả lỗi.
        *portal.transport_fail_first.lock().unwrap() = 99;
        let client = HddtClient::for_test(&portal.base);
        let pool = test_pool().await;
        let id = seed_invoice(&pool, "uuid-net-fail", 1, "{}").await;
        sqlx::query!(
            "UPDATE hddt_purchase_invoice
                SET status = 'manual', detail_error = 'Too Many Requests', detail_json = ''
              WHERE id = ?",
            id
        )
        .execute(&pool)
        .await
        .unwrap();

        let err = scan(
            &pool,
            &client,
            "mock-token",
            "2026-09-01",
            "2026-09-24",
            &[InvoiceKind::Regular],
        )
        .await
        .expect_err("cổng đứt kết nối thì quét phải trả lỗi");
        assert!(err.contains("Lỗi kết nối"), "lỗi sai: {err}");

        let row = sqlx::query!(
            "SELECT status, detail_error FROM hddt_purchase_invoice WHERE id = ?",
            id
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            row.status, "manual",
            "lỗi mạng không được biến HĐ thành 'Chờ nhập kho'"
        );
        assert_eq!(
            row.detail_error, "Too Many Requests",
            "phải giữ lỗi cũ để lượt sau còn thử lại"
        );
    }

    /// Trang tra cứu của cổng xin 15 dòng/lần; tab máy tính tiền (`sco-query`)
    /// không được thử thách với `size=50` (đúng URL lỗi của issue #8).
    #[tokio::test]
    async fn scan_uses_portal_default_page_size_for_cash_register() {
        use crate::hddt::mock_portal::MockPortal;
        let portal = MockPortal::start().await;
        let client = HddtClient::for_test(&portal.base);
        let pool = test_pool().await;

        scan(
            &pool,
            &client,
            "mock-token",
            "2026-09-01",
            "2026-09-24",
            &[InvoiceKind::Regular, InvoiceKind::CashRegister],
        )
        .await
        .expect("quét mock thất bại");

        let reqs = portal.requests();
        let regular = reqs
            .iter()
            .find(|r| r.path.contains("/query/invoices/purchase"))
            .expect("phải gọi endpoint HĐ điện tử");
        assert_eq!(regular.param("size").as_deref(), Some("50"));
        let cash = reqs
            .iter()
            .find(|r| r.path.contains("/sco-query/invoices/purchase"))
            .expect("phải gọi endpoint máy tính tiền");
        assert_eq!(
            cash.param("size").as_deref(),
            Some("15"),
            "sco-query phải dùng đúng size mặc định của cổng: {}",
            cash.path
        );
    }

    /// `sco-query` (máy tính tiền) chết nhưng `query` (HĐ điện tử) sống → vẫn
    /// quét/nhập được HĐ thường, chỉ ghi cảnh báo cho phần lỗi (issue #8).
    #[tokio::test]
    async fn scan_keeps_regular_invoices_when_cash_register_endpoint_dies() {
        use crate::hddt::mock_portal::MockPortal;
        let portal = MockPortal::start().await;
        *portal.fail_path_contains.lock().unwrap() = vec!["sco-query".into()];
        let client = HddtClient::for_test(&portal.base);
        let pool = test_pool().await;

        let sum = scan(
            &pool,
            &client,
            "mock-token",
            "2026-09-01",
            "2026-09-24",
            &[InvoiceKind::Regular, InvoiceKind::CashRegister],
        )
        .await
        .expect("HĐ điện tử vẫn phải quét được khi sco-query chết");

        assert!(
            sum.errors.iter().any(|e| e.contains("Máy tính tiền")),
            "phải ghi lỗi phần máy tính tiền: {:?}",
            sum.errors
        );
        assert!(sum.days_scanned > 0, "phải quét xong phần HĐ thường");
        assert!(sum.invoices_new > 0, "phải giữ HĐ thường đã lấy được");
        assert!(sum.details_ok > 0, "phải lấy được dòng hàng HĐ thường");
    }

    /// Hóa đơn chưa có dòng hàng: báo câu hướng dẫn quét lại, không lộ lỗi serde
    /// "EOF while parsing a value at line 1 column 0" (ảnh lỗi issue #7).
    #[tokio::test]
    async fn import_without_detail_asks_to_rescan_instead_of_eof() {
        let pool = test_pool().await;
        let id = seed_invoice(&pool, "uuid-empty-detail", 1, "{}").await;
        sqlx::query!(
            "UPDATE hddt_purchase_invoice SET detail_json = '' WHERE id = ?",
            id
        )
        .execute(&pool)
        .await
        .unwrap();

        let err = import_invoice(&pool, &load(&pool, id).await, "", "HKD", "", "")
            .await
            .expect_err("thiếu dòng hàng thì không được tạo phiếu");
        assert!(
            err.contains("chưa tải được dòng hàng"),
            "lỗi phải hướng dẫn quét lại: {err}"
        );
        assert!(!err.contains("EOF"), "không được lộ lỗi serde: {err}");
    }

    /// Issue #8 mục 2: bấm Xem PDF/Nhập kho khi cache thiếu chi tiết phải tải
    /// đúng 1 hóa đơn từ cổng rồi lưu lại — không bắt quét lại cả ngày.
    #[tokio::test]
    async fn fetch_detail_gets_one_invoice_and_stores_it() {
        use crate::hddt::mock_portal::MockPortal;
        let portal = MockPortal::start().await;
        let client = HddtClient::for_test(&portal.base);
        let pool = test_pool().await;
        let id = seed_invoice(&pool, "uuid-on-demand", 1, "{}").await;
        sqlx::query!(
            "UPDATE hddt_purchase_invoice
                SET detail_json = '', line_count = 0, detail_error = ''
              WHERE id = ?",
            id
        )
        .execute(&pool)
        .await
        .unwrap();

        let mut inv = load(&pool, id).await;
        let detail_json = inv
            .fetch_detail(&pool, &client, "mock-token")
            .await
            .expect("phải tải được chi tiết của đúng hóa đơn này");

        let detail: Value = serde_json::from_str(&detail_json).unwrap();
        assert_eq!(
            detail["hdhhdvu"].as_array().map(Vec::len),
            Some(1),
            "mock trả 1 dòng hàng: {detail_json}"
        );
        let row = sqlx::query!(
            "SELECT detail_json, line_count, detail_error
               FROM hddt_purchase_invoice WHERE id = ?",
            id
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.detail_json, detail_json);
        assert_eq!(row.line_count, 1);
        assert_eq!(row.detail_error, "");
        assert_eq!(
            portal.count("/invoices/detail"),
            1,
            "chỉ gọi chi tiết 1 lần, không quét danh sách"
        );
    }

    /// Hóa đơn từng bị đánh 'manual' vì lỗi chi tiết: tải lại thành công thì
    /// phải trở về 'pending' để nút Nhập kho dùng được.
    #[tokio::test]
    async fn fetch_detail_returns_manual_detail_error_to_pending() {
        use crate::hddt::mock_portal::MockPortal;
        let portal = MockPortal::start().await;
        let client = HddtClient::for_test(&portal.base);
        let pool = test_pool().await;
        let id = seed_invoice(&pool, "uuid-on-demand-manual", 1, "{}").await;
        sqlx::query!(
            "UPDATE hddt_purchase_invoice
                SET status = 'manual', skip_reason = 'Không lấy được chi tiết: lỗi mạng',
                    detail_error = 'lỗi mạng', detail_json = '', line_count = 0
              WHERE id = ?",
            id
        )
        .execute(&pool)
        .await
        .unwrap();

        let mut inv = load(&pool, id).await;
        inv.fetch_detail(&pool, &client, "mock-token")
            .await
            .expect("tải chi tiết");

        let row = sqlx::query!(
            "SELECT status, skip_reason, detail_error
               FROM hddt_purchase_invoice WHERE id = ?",
            id
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.status, "pending");
        assert_eq!(row.skip_reason, "");
        assert_eq!(row.detail_error, "");
    }

    /// Hóa đơn 'manual' vì lý do khác (tthai != 1) không được tự chuyển pending
    /// chỉ vì tải được chi tiết.
    #[tokio::test]
    async fn fetch_detail_keeps_manual_invoice_for_other_reasons() {
        use crate::hddt::mock_portal::MockPortal;
        let portal = MockPortal::start().await;
        let client = HddtClient::for_test(&portal.base);
        let pool = test_pool().await;
        let id = seed_invoice(&pool, "uuid-on-demand-tthai", 0, "{}").await;
        sqlx::query!(
            "UPDATE hddt_purchase_invoice
                SET detail_json = '', line_count = 0, detail_error = '',
                    skip_reason = 'Hóa đơn tthai=2 — không tự tạo phiếu nhập'
              WHERE id = ?",
            id
        )
        .execute(&pool)
        .await
        .unwrap();

        let mut inv = load(&pool, id).await;
        assert_eq!(inv.status, "manual");
        inv.fetch_detail(&pool, &client, "mock-token")
            .await
            .expect("tải chi tiết");

        let row = sqlx::query!(
            "SELECT status, skip_reason FROM hddt_purchase_invoice WHERE id = ?",
            id
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.status, "manual");
        assert!(
            row.skip_reason.contains("tthai"),
            "giữ lý do cần xử lý: {}",
            row.skip_reason
        );
    }

    /// Thiếu mã mẫu số thì báo rõ, không gọi endpoint chi tiết (cổng trả 500).
    #[tokio::test]
    async fn fetch_detail_without_form_code_does_not_call_portal() {
        use crate::hddt::mock_portal::MockPortal;
        let portal = MockPortal::start().await;
        let client = HddtClient::for_test(&portal.base);
        let pool = test_pool().await;
        let id = seed_invoice(&pool, "uuid-on-demand-nokhmshdon", 1, "{}").await;
        sqlx::query!(
            "UPDATE hddt_purchase_invoice
                SET detail_json = '', line_count = 0, detail_error = '', khmshdon = 0
              WHERE id = ?",
            id
        )
        .execute(&pool)
        .await
        .unwrap();

        let mut inv = load(&pool, id).await;
        let err = inv
            .fetch_detail(&pool, &client, "mock-token")
            .await
            .expect_err("thiếu khmshdon phải báo lỗi");
        assert!(err.contains("mã mẫu số"), "lỗi sai: {err}");
        assert_eq!(portal.count("/invoices/detail"), 0);
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
        // 2 × 2.860.000 − 2.860.000 CK = 2.860.000 (chưa thuế). Hộ mặc định
        // không khấu trừ → VAT 8% cộng vào giá nhập: 2.860.000 × 1,08.
        assert!(
            (amount - 3_088_800.0).abs() < 1.0,
            "giá trị nhập = (thành tiền − tiền CK) × (1 + thuế), thấy {}",
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
        //
        // Import thời đó luôn ghi vat_rate = 0 (amount = giá chưa thuế), nên
        // tái tạo dữ liệu legacy bằng dòng tsuat = 0 — nếu để 8% thì amount
        // mới đã gồm thuế (hộ không khấu trừ) và không khớp công thức backfill.
        let pool = test_pool().await;
        let lines = r#"{"ten":"Aptomat ABN203c","dvtinh":"Cái","mhhdvu":null,"sluong":2.0,
                        "dgia":2860000.0,"stckhau":2860000.0,"tlckhau":50.0,"tsuat":0.0}"#;
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

    /// Cài đặt thông tin HKD nhận **bất kỳ ngày nào** làm mốc bắt đầu HĐĐT, kể cả
    /// tương lai — khi đó chưa có gì để quét, nên vòng quét phải trả về im lặng
    /// thay vì lỗi "Khoảng ngày không hợp lệ" (quan trọng khi quét tự động lúc mở app).
    #[tokio::test]
    async fn scan_voi_moc_bat_dau_tuong_lai_tra_ve_rong_khong_loi() {
        use crate::hddt::mock_portal::MockPortal;
        let portal = MockPortal::start().await;
        let client = HddtClient::for_test(&portal.base);
        let pool = test_pool().await;
        sqlx::query!(
            "INSERT INTO business (id, hddt_start_date) VALUES (1, '2076-01-01')
             ON CONFLICT(id) DO UPDATE SET hddt_start_date = excluded.hddt_start_date"
        )
        .execute(&pool)
        .await
        .expect("khai mốc bắt đầu tương lai");

        let sum = scan(
            &pool,
            &client,
            "mock-token",
            "",
            "",
            &[InvoiceKind::Regular],
        )
        .await
        .expect("mốc tương lai không được làm quét thất bại");
        assert_eq!(sum.days_scanned, 0, "chưa tới mốc → không quét ngày nào");
        assert_eq!(sum.days_today, 0);
        assert_eq!(sum.invoices_new, 0);
        assert_eq!(
            portal.count("/invoices/purchase"),
            0,
            "không được gọi cổng khi chưa tới mốc bắt đầu"
        );
    }

    /// Hóa đơn trộn 8% + 10%: lưu đúng thuế suất từng dòng, tổng 133 khớp
    /// `tgtthue` (kể cả khi hóa đơn làm tròn lệch 1 đồng so với tích từng dòng).
    #[tokio::test]
    async fn import_luu_thue_tung_dong_va_khop_tong_hoa_don() {
        let pool = test_pool().await;
        set_vat_deduct(&pool, true).await;
        let lines = r#"{"ten":"Hàng 8%","dvtinh":"Cái","mhhdvu":"a","sluong":2.0,
                         "dgia":100000.0,"stckhau":0.0,"tsuat":0.08},
                       {"ten":"Hàng 10%","dvtinh":"Cái","mhhdvu":"b","sluong":1.0,
                         "dgia":200000.0,"stckhau":0.0,"tsuat":0.10}"#;
        let id = seed_invoice(&pool, "uuid-multi-vat", 1, lines).await;
        // Tổng dòng: 200.000 + 200.000 = 400.000; thuế 16.000 + 20.000 = 36.000.
        // Giả lập hóa đơn làm tròn thuế lên 1 đồng.
        sqlx::query!(
            "UPDATE hddt_purchase_invoice SET tgtthue = 36001, tgtttbso = 436001 WHERE id = ?",
            id
        )
        .execute(&pool)
        .await
        .unwrap();
        let out = import_invoice(&pool, &load(&pool, id).await, "", "HKD", "", "")
            .await
            .unwrap();
        assert!(!out.updated);

        let (total, vat_amount, vat_rate): (f64, f64, f64) = sqlx::query_as(
            "SELECT total, vat_amount, vat_rate FROM inbound_voucher WHERE voucher_no = ?",
        )
        .bind(&out.voucher_no)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(total, 400_000.0, "tổng hàng chưa thuế");
        assert_eq!(vat_amount, 36_001.0, "thuế đầu vào khớp hóa đơn");
        assert!(
            (vat_rate - 0.09).abs() < 1e-6,
            "tỷ lệ bình quân đầu phiếu = 36.001/400.000"
        );

        // Bút toán 133 tách theo từng dòng, giữ đúng thuế suất 8% / 10%.
        let vat_rows: Vec<(f64, f64)> = sqlx::query_as(
            "SELECT vat_rate, amount FROM journal_entry
              WHERE voucher_no = ? AND debit_account = '133' ORDER BY id",
        )
        .bind(&out.voucher_no)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(vat_rows.len(), 2);
        assert!((vat_rows[0].0 - 0.08).abs() < 1e-9);
        assert!((vat_rows[1].0 - 0.10).abs() < 1e-9);
        let vat_sum: f64 = vat_rows.iter().map(|r| r.1).sum();
        assert!(
            (vat_sum - 36_001.0).abs() < 0.01,
            "tổng 133 phải khớp tgtthue, thấy {vat_sum}"
        );

        // Tổng phiếu nhập + thuế = tổng thanh toán của hóa đơn.
        let invoice_total: f64 = sqlx::query_scalar!(
            r#"SELECT tgtttbso as "tgtttbso!" FROM hddt_imported_invoice WHERE portal_id = ?"#,
            "uuid-multi-vat"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!((total + vat_amount - invoice_total).abs() < 0.01);

        // Mặt hàng mới lưu thuế nhập mặc định đúng thuế suất dòng.
        let import_rates: Vec<f64> = sqlx::query_scalar(
            "SELECT import_tax_rate FROM product WHERE name IN ('Hàng 8%', 'Hàng 10%')
              ORDER BY name",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(import_rates, vec![0.10, 0.08]);
    }

    /// Chiết khấu cấp hóa đơn (ttcktmai) phải được phân bổ xuống dòng để
    /// Σ giá trị hàng = `tgtttbso − tgtthue`, không chỉ lấy tổng dòng.
    #[tokio::test]
    async fn import_phan_bo_chiet_khau_cap_hoa_don_va_khop_tong() {
        let pool = test_pool().await;
        set_vat_deduct(&pool, true).await;
        let lines = r#"{"ten":"Hàng CK hóa đơn","dvtinh":"Cái","mhhdvu":"c","sluong":10.0,
                         "dgia":100000.0,"stckhau":0.0,"tsuat":0.08}"#;
        let id = seed_invoice(&pool, "uuid-inv-discount", 1, lines).await;
        // Giá gốc 1.000.000 + VAT 80.000 = 1.080.000; chiết khấu hóa đơn 100.000
        // → khách trả 980.000, thuế giữ nguyên 80.000.
        sqlx::query!(
            "UPDATE hddt_purchase_invoice SET ttcktmai = 100000, tgtttbso = 980000 WHERE id = ?",
            id
        )
        .execute(&pool)
        .await
        .unwrap();
        let out = import_invoice(&pool, &load(&pool, id).await, "", "HKD", "", "")
            .await
            .unwrap();

        let (total, vat_amount): (f64, f64) =
            sqlx::query_as("SELECT total, vat_amount FROM inbound_voucher WHERE voucher_no = ?")
                .bind(&out.voucher_no)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(total, 900_000.0, "giá trị hàng sau chiết khấu hóa đơn");
        assert_eq!(vat_amount, 80_000.0);
        assert!((total + vat_amount - 980_000.0).abs() < 0.01);

        let (amount, discount): (f64, f64) = sqlx::query_as(
            "SELECT amount, discount FROM journal_entry
              WHERE voucher_no = ? AND entry_type = 'PN' AND product_code <> ''",
        )
        .bind(&out.voucher_no)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(amount, 900_000.0);
        assert_eq!(discount, 100_000.0, "chiết khấu hóa đơn dồn xuống dòng");
    }

    /// Phiếu nhập tay trùng ký hiệu + số + ngày hóa đơn phải được CẬP NHẬT
    /// (giữ số phiếu), không tạo phiếu mới; lô cũ còn nguyên mới được ghi đè.
    #[tokio::test]
    async fn import_ghi_de_phieu_nhap_tay_cung_hoa_don() {
        let pool = test_pool().await;
        set_vat_deduct(&pool, true).await;
        seed_product(&pool, "SP-TAY", "Hàng nhập tay", 0.08).await;
        let lines = r#"{"ten":"Hàng hóa đơn","dvtinh":"Cái","mhhdvu":"h1","sluong":2.0,
                         "dgia":100000.0,"stckhau":0.0,"tsuat":0.08}"#;
        let id = seed_invoice(&pool, "uuid-overwrite", 1, lines).await;
        let inv_row = sqlx::query!(
            "SELECT khhdon, shdon, posting_date FROM hddt_purchase_invoice WHERE id = ?",
            id
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        let manual_items = [InboundItemInput {
            product_code: "SP-TAY".into(),
            quantity: 3.0,
            unit_price: 50_000.0,
            discount: 0.0,
            vat_rate: Some(0.08),
            vat_amount: None,
        }];
        let extra = InboundExtra {
            source: "manual",
            invoice_serial: &inv_row.khhdon,
            invoice_no: &inv_row.shdon,
            invoice_date: &inv_row.posting_date,
            overwrite: false,
            exclude_id: None,
        };
        let manual = save_inbound_core_with(
            &pool,
            &inv_row.posting_date,
            "PN-TAY-1",
            "Phiếu nhập tay",
            "",
            "KHO-CHINH",
            "HKD",
            &manual_items,
            "",
            "purchase",
            "TAY-REF",
            0.0,
            "152",
            "331",
            false,
            "",
            false,
            &extra,
        )
        .await
        .expect("lưu phiếu tay");
        assert_eq!(manual.total, 150_000.0);

        let out = import_invoice(&pool, &load(&pool, id).await, "", "HKD", "", "")
            .await
            .expect("import phải ghi đè phiếu tay");
        assert!(out.updated, "phải báo đã cập nhật");
        assert_eq!(out.voucher_no, "PN-TAY-1", "giữ nguyên số phiếu nội bộ");

        // Chỉ còn 1 đầu phiếu, đã chuyển source=hddt và cập nhật tổng/hóa đơn.
        let header = sqlx::query!(
            r#"SELECT id as "id!", source, total, vat_amount, warehouse_code,
                      invoice_serial, invoice_no, invoice_date
                 FROM inbound_voucher WHERE voucher_no = 'PN-TAY-1'"#
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(header.id, manual.inbound_voucher_id, "giữ nguyên id");
        assert_eq!(header.source, "hddt");
        assert_eq!(header.total, 200_000.0);
        assert_eq!(header.vat_amount, 16_000.0);
        assert_eq!(header.warehouse_code, "KHO-CHINH");
        assert_eq!(header.invoice_serial, inv_row.khhdon);
        assert_eq!(header.invoice_no, inv_row.shdon);
        assert_eq!(header.invoice_date, inv_row.posting_date);
        let count: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM inbound_voucher")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1, "không được tạo thêm phiếu");

        // Lô + bút toán của mặt hàng nhập tay cũ đã bị thay bằng hàng hóa đơn.
        let old_lots: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM stock_lot sl JOIN product p ON p.id = sl.product_id
              WHERE p.code = 'SP-TAY'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(old_lots, 0, "lô phiếu tay phải bị xoá");
        let old_entries: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM journal_entry
              WHERE voucher_no = 'PN-TAY-1' AND product_code = 'SP-TAY'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(old_entries, 0, "bút toán phiếu tay phải bị xoá");

        let new_lot: f64 = sqlx::query_scalar!(
            r#"SELECT COALESCE(SUM(sl.quantity), 0.0) as "q!: f64"
                 FROM stock_lot sl JOIN product p ON p.id = sl.product_id
                WHERE p.name = 'Hàng hóa đơn'"#
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(new_lot, 2.0, "lô hàng hóa đơn phải được tạo đúng SL");

        let linked: i64 = sqlx::query_scalar!(
            "SELECT inbound_voucher_id FROM hddt_imported_invoice WHERE portal_id = ?",
            "uuid-overwrite"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(linked, manual.inbound_voucher_id);
    }

    /// Lô của phiếu tay đã bị xuất một phần → từ chối ghi đè, không nhân đôi
    /// tồn kho hoặc làm lệch giá vốn đã bán.
    #[tokio::test]
    async fn import_tu_choi_ghi_de_khi_lo_da_xuat() {
        let pool = test_pool().await;
        set_vat_deduct(&pool, true).await;
        seed_product(&pool, "SP-TAY-2", "Hàng nhập tay 2", 0.08).await;
        let lines = r#"{"ten":"Hàng hóa đơn 2","dvtinh":"Cái","mhhdvu":"h2","sluong":2.0,
                         "dgia":100000.0,"stckhau":0.0,"tsuat":0.08}"#;
        let id = seed_invoice(&pool, "uuid-overwrite-used", 1, lines).await;
        let inv_row = sqlx::query!(
            "SELECT khhdon, shdon, posting_date FROM hddt_purchase_invoice WHERE id = ?",
            id
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let manual_items = [InboundItemInput {
            product_code: "SP-TAY-2".into(),
            quantity: 3.0,
            unit_price: 50_000.0,
            discount: 0.0,
            vat_rate: Some(0.08),
            vat_amount: None,
        }];
        let extra = InboundExtra {
            source: "manual",
            invoice_serial: &inv_row.khhdon,
            invoice_no: &inv_row.shdon,
            invoice_date: &inv_row.posting_date,
            overwrite: false,
            exclude_id: None,
        };
        let manual = save_inbound_core_with(
            &pool,
            &inv_row.posting_date,
            "PN-TAY-2",
            "Phiếu nhập tay 2",
            "",
            "KHO-CHINH",
            "HKD",
            &manual_items,
            "",
            "purchase",
            "TAY-REF-2",
            0.0,
            "152",
            "331",
            false,
            "",
            false,
            &extra,
        )
        .await
        .expect("lưu phiếu tay");
        // Giả lập đã xuất 2/3: lô chỉ còn 1.
        sqlx::query!(
            "UPDATE stock_lot SET quantity = 1 WHERE inbound_voucher_id = ?",
            manual.inbound_voucher_id
        )
        .execute(&pool)
        .await
        .unwrap();

        let err = import_invoice(&pool, &load(&pool, id).await, "", "HKD", "", "")
            .await
            .expect_err("lô đã xuất thì phải từ chối");
        assert!(
            err.contains("đã bị xuất/điều chỉnh"),
            "thông báo phải nói rõ lô đã dùng: {err}"
        );

        // Rollback: phiếu tay + lô + cache còn nguyên, chưa có liên kết hóa đơn.
        let source: String = sqlx::query_scalar!(
            "SELECT source FROM inbound_voucher WHERE id = ?",
            manual.inbound_voucher_id
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(source, "manual");
        let old_lot: f64 = sqlx::query_scalar!(
            r#"SELECT COALESCE(SUM(quantity), 0.0) as "q!: f64"
                 FROM stock_lot WHERE inbound_voucher_id = ?"#,
            manual.inbound_voucher_id
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(old_lot, 1.0);
        let cache: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM hddt_purchase_invoice WHERE id = ?",
            id
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(cache, 1);
        let linked: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM hddt_imported_invoice WHERE inbound_voucher_id = ?",
            manual.inbound_voucher_id
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(linked, 0);
    }

    /// Không cho lưu 2 phiếu nhập tay cùng ký hiệu + số + ngày hóa đơn.
    #[tokio::test]
    async fn save_inbound_chan_trung_hoa_don() {
        let pool = test_pool().await;
        seed_product(&pool, "SP-TRUNG", "Hàng trùng hóa đơn", 0.08).await;
        let items = [InboundItemInput {
            product_code: "SP-TRUNG".into(),
            quantity: 1.0,
            unit_price: 100_000.0,
            discount: 0.0,
            vat_rate: Some(0.08),
            vat_amount: None,
        }];
        let extra = InboundExtra {
            source: "manual",
            invoice_serial: "C26XYZ",
            invoice_no: "999",
            invoice_date: "2026-09-02",
            overwrite: false,
            exclude_id: None,
        };
        save_inbound_core_with(
            &pool,
            "2026-09-02",
            "PN-TRUNG-1",
            "Phiếu 1",
            "",
            "KHO-CHINH",
            "HKD",
            &items,
            "",
            "purchase",
            "",
            0.0,
            "152",
            "331",
            false,
            "",
            false,
            &extra,
        )
        .await
        .expect("phiếu đầu lưu được");

        let err = save_inbound_core_with(
            &pool,
            "2026-09-02",
            "PN-TRUNG-2",
            "Phiếu 2",
            "",
            "KHO-CHINH",
            "HKD",
            &items,
            "",
            "purchase",
            "",
            0.0,
            "152",
            "331",
            false,
            "",
            false,
            &extra,
        )
        .await
        .expect_err("trùng hóa đơn phải bị chặn");
        assert!(
            err.contains("đã có trong phiếu nhập PN-TRUNG-1"),
            "thông báo phải chỉ phiếu cũ: {err}"
        );
    }
}
