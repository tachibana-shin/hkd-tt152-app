#![allow(unused_imports)]

use crate::helpers::*;
use crate::models::*;
use serde_json::json;
use sqlx::sqlite::{SqlitePoolOptions, SqliteTransaction};
use sqlx::SqlitePool;
use tauri::{AppHandle, Manager, State};

/// Sổ nhật ký theo kỳ — nguồn cho S2a-HKD, S2b-HKD, in ấn, xuất Excel
#[tauri::command]
pub(crate) async fn get_ledger(
    state: State<'_, AppState>,
    from_date: String,
    to_date: String,
) -> Result<String, String> {
    let rows: Vec<LedgerRow> = sqlx::query_as!(
        LedgerRow,
        "SELECT posting_date, voucher_no, entry_type, description, product_code,
                debit_account, credit_account, quantity, unit_price, amount
         FROM journal_entry
         WHERE posting_date >= ? AND posting_date <= ?
         ORDER BY posting_date ASC, id ASC
         LIMIT 5000",
        from_date,
        to_date
    )
    .fetch_all(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    Ok(serde_json::to_string(&rows).unwrap_or_default())
}

/// Sổ nhật ký chung — phân trang server-side trong khoảng ngày đã chọn.
///
/// Cả năm một hộ nhập liệu dày đặc có thể vài nghìn bút toán; kéo hết về
/// client rồi cắt trang khiến mở màn đơ rất lâu.
#[tauri::command]
pub(crate) async fn get_ledger_page(
    state: State<'_, AppState>,
    lazy_event: String,
    from_date: String,
    to_date: String,
    entry_type: String,
) -> Result<String, String> {
    let ev: crate::commands::page::PageEvent =
        serde_json::from_str(&lazy_event).map_err(|e| format!("lazy_event lỗi: {}", e))?;
    let page = ev.page();
    // Khoảng ngày là bộ lọc riêng của màn Sổ nhật ký, cộng thêm ô tìm kiếm
    // toàn cục của bảng (mô tả, số phiếu, mã hàng, tài khoản).
    let mut params: Vec<crate::commands::page::BindVal> = vec![
        crate::commands::page::BindVal(entry_type.clone()),
        crate::commands::page::BindVal(entry_type),
        crate::commands::page::BindVal(from_date),
        crate::commands::page::BindVal(to_date),
    ];
    let mut where_sql = String::from(
        " WHERE (? = '' OR entry_type = ?) AND posting_date >= ? AND posting_date <= ?",
    );
    if let Some(kw) = ev.global_keyword() {
        let like = format!("%{}%", kw.replace('%', "\\%").replace('_', "\\_"));
        where_sql.push_str(
            " AND (voucher_no LIKE ? ESCAPE '\\' OR description LIKE ? ESCAPE '\\' OR product_code LIKE ? ESCAPE '\\' OR debit_account LIKE ? ESCAPE '\\' OR credit_account LIKE ? ESCAPE '\\')",
        );
        for _ in 0..5 {
            params.push(crate::commands::page::BindVal(like.clone()));
        }
    }
    let order = crate::commands::page::order_by(
        &[
            ("id", "id"),
            ("posting_date", "posting_date"),
            ("voucher_no", "voucher_no"),
            ("entry_type", "entry_type"),
            ("debit_account", "debit_account"),
            ("credit_account", "credit_account"),
        ],
        &ev,
        "posting_date ASC, id ASC",
    );
    let sql = format!(
        "SELECT posting_date, voucher_no, entry_type, description, product_code,
                debit_account, credit_account, quantity, unit_price, amount
         FROM journal_entry{} ORDER BY {} LIMIT ? OFFSET ?",
        where_sql, order
    );
    let count_sql = format!("SELECT COUNT(*) FROM journal_entry{where_sql}");
    let pool = state.pool.read().await;
    let (rows, total): (Vec<LedgerRow>, i64) =
        crate::commands::page::fetch_page(&pool, &sql, &count_sql, &params, &page).await?;
    // CAST sang REAL: khi kỳ lọc không có dòng nào, COALESCE trả số nguyên 0 và
    // sqlx từ chối decode INTEGER vào f64.
    let sum_sql =
        format!("SELECT CAST(COALESCE(SUM(amount), 0) AS REAL) FROM journal_entry{where_sql}");
    let mut sum_q = sqlx::query_scalar::<_, f64>(&sum_sql);
    for p in &params {
        sum_q = sum_q.bind(p.0.clone());
    }
    let total_amount: f64 = sum_q.fetch_one(&*pool).await.map_err(|e| e.to_string())?;
    Ok(serde_json::to_string(&json!({
        "rows": rows,
        "total": total,
        "total_amount": total_amount,
    }))
    .unwrap_or_default())
}

/// Bảng cân đối số phát sinh: số dư đầu kỳ (từ DMTK) + phát sinh trong kỳ
/// (từ journal_entry) + số dư cuối kỳ, nhóm theo từng tài khoản.
#[tauri::command]
pub(crate) async fn get_trial_balance(
    state: State<'_, AppState>,
    from_date: String,
    to_date: String,
) -> Result<String, String> {
    let pool = state.pool.read().await;
    let accounts: Vec<(String, String, f64, f64)> =
        sqlx::query!("SELECT code, name, opening_debit, opening_credit FROM account ORDER BY code")
            .fetch_all(&*pool)
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|r| (r.code, r.name, r.opening_debit, r.opening_credit))
            .collect();

    // Tổng phát sinh Nợ / Có theo từng tài khoản trong kỳ (SUM có ít nhất 1 dòng
    // trong mỗi nhóm nên ép NOT NULL).
    let mvmt: Vec<(String, f64, f64)> = sqlx::query!(
        r#"SELECT code, SUM(d) AS "debit_mvmt!: f64", SUM(c) AS "credit_mvmt!: f64" FROM (
               SELECT debit_account AS code, amount AS d, 0.0 AS c FROM journal_entry
               WHERE posting_date >= ? AND posting_date <= ?
               UNION ALL
               SELECT credit_account, 0.0, amount FROM journal_entry
               WHERE posting_date >= ? AND posting_date <= ?
            ) GROUP BY code"#,
        from_date,
        to_date,
        from_date,
        to_date
    )
    .fetch_all(&*pool)
    .await
    .map_err(|e| e.to_string())?
    .into_iter()
    .map(|r| (r.code, r.debit_mvmt, r.credit_mvmt))
    .collect();

    let mv: std::collections::HashMap<String, (f64, f64)> = mvmt
        .into_iter()
        .map(|(code, d, c)| (code, (d, c)))
        .collect();
    let rows: Vec<TrialBalanceRow> = accounts
        .into_iter()
        .map(|(code, name, od, oc)| {
            let (d, c) = mv.get(&code).copied().unwrap_or((0.0, 0.0));
            let net = od + d - oc - c;
            let (cd, cc) = if net > 0.0 { (net, 0.0) } else { (0.0, -net) };
            TrialBalanceRow {
                code,
                name,
                opening_debit: od,
                opening_credit: oc,
                debit_mvmt: round2(d),
                credit_mvmt: round2(c),
                closing_debit: round2(cd),
                closing_credit: round2(cc),
            }
        })
        .collect();
    Ok(serde_json::to_string(&rows).unwrap_or_default())
}

/// Doanh thu + thuế theo nhóm ngành nghề, trong khoảng ngày người dùng chọn.
///
/// Sửa so với bản cũ: bản cũ **không xét nhóm hộ lẫn phương pháp TNCN** nên
/// hiện thuế phải nộp cho cả hộ nhóm 1 (được miễn) và hiện TNCN theo doanh thu
/// cho hộ đã chọn tính theo lợi nhuận — lệch với thẻ tổng hợp ngay trên màn.
/// Giờ dùng chung `build_tax_declaration` với tờ khai theo kỳ nên hai thẻ luôn
/// khớp nhau.
/// Tờ khai thuế theo khoảng ngày đang chọn, chia theo nhóm ngành nghề.
///
/// Trước đây có hai lệnh gần như trùng nhau: một lệnh theo khoảng ngày tùy ý và
/// một lệnh theo kỳ khai (năm/quý/tháng). Người dùng chỉ cần một bảng — theo đúng
/// khoảng thời gian đang chọn trên thanh công cụ — nên đã gộp còn lệnh này.
///
/// Mức trừ ngưỡng TNCN vẫn tính theo doanh thu lũy kế **từ đầu năm** tới trước
/// ngày bắt đầu của khoảng, nên xem một khoảng bất kỳ trong năm vẫn không bị tính
/// trùng phần mức trừ.
#[tauri::command]
pub(crate) async fn get_tax_declaration(
    state: State<'_, AppState>,
    from_date: String,
    to_date: String,
    unit_code: String,
) -> Result<String, String> {
    let pool = state.pool.read().await;
    let year = from_date
        .get(0..4)
        .and_then(|y| y.parse::<i64>().ok())
        .ok_or("Ngày bắt đầu không hợp lệ")?;
    let ctx = load_tax_context(&pool, year).await?;
    let rows = load_tax_agg_in_range(&pool, &from_date, &to_date, &unit_code).await?;

    // Mức trừ ngưỡng 01 tỷ là số tiền của cả năm: dùng lũy kế doanh thu từ đầu
    // năm tới hết ngày trước kỳ đang xem để biết kỳ này còn trừ được bao nhiêu.
    let cum = load_cum_revenue_until(&pool, year, &prev_day(&from_date)).await?;
    let period: std::collections::HashMap<String, f64> = rows
        .iter()
        .map(|r| (r.industry_code.clone(), net_revenue(r)))
        .collect();
    let split = split_by_period(&cum, &period, &ctx.alloc);
    let taxable: std::collections::HashMap<String, f64> =
        split.iter().map(|(c, v, _)| (c.clone(), *v)).collect();
    let deducted: std::collections::HashMap<String, f64> =
        split.iter().map(|(c, _, d)| (c.clone(), *d)).collect();
    let taxable_period = period_is_taxable(
        cum.values().sum(),
        period.values().sum(),
        ctx.settings.thresholds.exempt,
        ctx.taxed_from_start,
    );
    let out = build_tax_declaration(
        rows,
        &ctx.settings.method,
        ctx.group,
        &taxable,
        taxable_period,
        &deducted,
    );
    Ok(serde_json::to_string(&out).unwrap_or_default())
}

/// Doanh thu ghi trên phiếu xuất trong khoảng ngày, gộp theo nhóm ngành nghề.
async fn load_tax_agg_in_range(
    pool: &SqlitePool,
    from_date: &str,
    to_date: &str,
    unit_code: &str,
) -> Result<Vec<TaxAgg>, String> {
    sqlx::query_as!(
        TaxAgg,
        "SELECT
            ig.code AS industry_code,
            ig.name AS industry_name,
            ig.vat_rate,
            ig.pit_rate,
            COALESCE(SUM(CASE WHEN je.adjust_code != 'GiamDT' THEN je.amount ELSE 0.0 END), 0.0) AS revenue_up,
            COALESCE(SUM(CASE WHEN je.adjust_code = 'GiamDT' THEN je.amount ELSE 0.0 END), 0.0) AS revenue_down
         FROM industry_group ig
         LEFT JOIN journal_entry je ON je.industry_code = ig.code
            AND je.entry_type = 'PX'
            AND je.posting_date >= ?
            AND je.posting_date <= ?
            AND je.unit_code = ?
         GROUP BY ig.code
         ORDER BY ig.code",
        from_date,
        to_date,
        unit_code
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())
}

/// Doanh thu lũy kế từ 01/01 của `year` tới hết ngày `to_date` (ISO), theo nhóm ngành.
async fn load_cum_revenue_until(
    pool: &SqlitePool,
    year: i64,
    to_date: &str,
) -> Result<std::collections::HashMap<String, f64>, String> {
    let year_start = format!("{year:04}-01-01");
    let rows = sqlx::query!(
        r#"SELECT je.industry_code AS "industry_code!", je.adjust_code AS "adjust_code!",
                  je.amount
             FROM journal_entry je
            WHERE je.entry_type = 'PX'
              AND je.posting_date >= ? AND je.posting_date <= ?
              AND je.industry_code <> ''"#,
        year_start,
        to_date
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    let mut out: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    for r in rows {
        let sign = if r.adjust_code == "GiamDT" { -1.0 } else { 1.0 };
        *out.entry(r.industry_code).or_insert(0.0) += sign * r.amount;
    }
    Ok(out)
}

/// Ngày liền trước `date` (ISO `yyyy-mm-dd`) — lấy doanh thu lũy kế tới hết ngày
/// trước kỳ đang xem. Ngày không hợp lệ thì trả chính nó (không trừ nhầm kỳ).
fn prev_day(date: &str) -> String {
    let parts: Vec<i64> = date.split('-').filter_map(|p| p.parse().ok()).collect();
    if parts.len() != 3 {
        return date.to_string();
    }
    let (mut y, mut m, mut d) = (parts[0], parts[1], parts[2]);
    d -= 1;
    if d == 0 {
        m -= 1;
        if m == 0 {
            m = 12;
            y -= 1;
        }
        d = last_day_of_month(y, m).0;
    }
    format!("{y:04}-{m:02}-{d:02}")
}

/// Nạp dữ liệu gộp thô cho tờ khai thuế (chưa tính số phải nộp) cho 1 kỳ.
/// month = 0 → cả năm; ngược lại tháng 1..12.
pub(crate) async fn load_tax_agg(
    pool: &SqlitePool,
    year: i64,
    month: i64,
) -> Result<Vec<TaxAgg>, String> {
    if (1..=12).contains(&month) {
        load_tax_agg_range(pool, year, month, month).await
    } else {
        load_tax_agg_range(pool, year, 1, 12).await
    }
}

/// Nạp dữ liệu gộp thô cho khoảng tháng from_month..=to_month (hỗ trợ kỳ khai theo quý).
pub(crate) async fn load_tax_agg_range(
    pool: &SqlitePool,
    year: i64,
    from_month: i64,
    to_month: i64,
) -> Result<Vec<TaxAgg>, String> {
    let from = format!("{:04}-{:02}-01", year, from_month);
    let to = format!("{:04}-{:02}-31", year, to_month);
    sqlx::query_as::<_, TaxAgg>(
        "SELECT
            ig.code AS industry_code,
            ig.name AS industry_name,
            ig.vat_rate,
            ig.pit_rate,
            COALESCE(SUM(CASE WHEN je.adjust_code <> 'GiamDT' THEN je.amount ELSE 0.0 END), 0.0) AS revenue_up,
            COALESCE(SUM(CASE WHEN je.adjust_code = 'GiamDT' THEN je.amount ELSE 0.0 END), 0.0) AS revenue_down
         FROM industry_group ig
         LEFT JOIN journal_entry je
            ON je.industry_code = ig.code
           AND je.entry_type = 'PX'
           AND je.posting_date >= ?
           AND je.posting_date <= ?
         GROUP BY ig.code
         ORDER BY ig.code",
    )
    .bind(&from)
    .bind(&to)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())
}

// ─── THUẾ HKD 2026 — HÀM THUẦN (test được không cần DB) ───
//
// Mọi quy tắc tính thuế nằm ở đây để test bằng unit test; phần đọc DB chỉ lo
// lấy số liệu rồi đưa vào. Căn cứ:
//   • Luật Thuế TNCN 109/2025/QH15 — Điều 7 (phương pháp + thuế suất theo quy mô)
//   • NĐ 68/2026/NĐ-CP — Điều 3 (GTGT), Điều 4 (TNCN + cách phân bổ mức trừ),
//     Điều 5 (doanh thu tính thuế), Điều 6 (chi được/không được trừ),
//     Điều 8 (nguyên tắc kê khai, vượt ngưỡng giữa năm), Điều 10 (kỳ khai, hạn nộp)
//   • NĐ 141/2026/NĐ-CP — nâng ngưỡng 500 triệu → 01 tỷ đồng, áp dụng từ 01/01/2026
//   • Luật Thuế GTGT 48/2024/QH15 — Điều 12 (tỷ lệ % trên doanh thu)
//   • TT 152/2025/TT-BTC — mẫu sổ S1a/S2a/S2b–S2e/S3a-HKD

/// Khoảng ngày có đúng bằng một kỳ khai không (năm / quý / tháng đủ tháng).
///
/// Tờ khai nộp cho cơ quan thuế theo kỳ, nhưng người dùng xem tờ khai theo khoảng
/// ngày tùy ý. Hạn nộp chỉ có nghĩa khi khoảng đó trùng một kỳ khai — trùng thì trả
/// `(loại kỳ, số kỳ)`, không trùng thì `None` (UI ẩn hạn nộp thay vì hiện sai).
pub(crate) fn period_of_range(from_date: &str, to_date: &str) -> Option<(String, i64)> {
    let (fy, fm, fd) = date_parts(from_date)?;
    let (ty, tm, td) = date_parts(to_date)?;
    if fy != ty {
        return None;
    }
    let year = fy;
    // Cả năm: 01/01 → 31/12 (ngày cuối tháng 12 có thể 29/30/31).
    if fm == 1 && fd == 1 && tm == 12 && td == last_day_of_month(year, 12).0 {
        return Some(("year".to_string(), 0));
    }
    let q_start = (fm - 1) / 3 * 3 + 1;
    let q_end = q_start + 2;
    if fm == q_start && tm == q_end && fd == 1 && td == last_day_of_month(year, q_end).0 {
        return Some(("quarter".to_string(), (q_start - 1) / 3 + 1));
    }
    // Một tháng đủ: cùng tháng, từ ngày 01 tới ngày cuối tháng.
    if fm == tm && fd == 1 && td == last_day_of_month(year, tm).0 {
        return Some(("month".to_string(), tm));
    }
    None
}

/// Tách ngày ISO `yyyy-mm-dd` thành (năm, tháng, ngày).
fn date_parts(d: &str) -> Option<(i64, i64, i64)> {
    let (y, rest) = d.split_once('-')?;
    let (m, day) = rest.split_once('-')?;
    Some((y.parse().ok()?, m.parse().ok()?, day.parse().ok()?))
}

/// Ngưỡng doanh thu năm không phải nộp thuế GTGT và TNCN: 01 tỷ đồng
/// (NĐ 141/2026/NĐ-CP sửa NĐ 68/2026/NĐ-CP).
pub(crate) const THRESHOLD_EXEMPT: f64 = 1_000_000_000.0;
/// Mốc doanh thu năm: trên ngưỡng → tới 3 tỷ (thuế suất TNCN 15%).
pub(crate) const THRESHOLD_GROUP3: f64 = 3_000_000_000.0;
/// Mốc doanh thu năm: trên 3 tỷ → tới 50 tỷ (thuế suất TNCN 17%).
pub(crate) const THRESHOLD_GROUP4: f64 = 50_000_000_000.0;
/// Mức thanh toán từng lần bắt buộc có chứng từ thanh toán không dùng tiền mặt
/// thì mới được trừ khi tính thu nhập tính thuế (NĐ 68/2026 Điều 6 khoản 1).
pub(crate) const NON_CASH_PAYMENT_LIMIT: f64 = 5_000_000.0;

/// Ba mốc doanh thu quyết định nhóm hộ, cho phép sửa ở màn Cài đặt.
///
/// Ngưỡng đã nhảy nhiều lần qua các văn bản (100 triệu → 500 triệu → 01 tỷ) và
/// còn có thể đổi tiếp, nên không gắn cứng trong code: người dùng sửa được ở
/// Cài đặt, app lấy theo khi tính thuế.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TaxThresholds {
    /// Doanh thu năm từ mức này trở xuống: không phải nộp GTGT và TNCN.
    pub(crate) exempt: f64,
    /// Mốc nhóm 3 (thuế suất TNCN theo thu nhập 17%).
    pub(crate) group3: f64,
    /// Mốc nhóm 4 (thuế suất TNCN theo thu nhập 20%).
    pub(crate) group4: f64,
}

impl Default for TaxThresholds {
    fn default() -> Self {
        Self {
            exempt: THRESHOLD_EXEMPT,
            group3: THRESHOLD_GROUP3,
            group4: THRESHOLD_GROUP4,
        }
    }
}

impl TaxThresholds {
    /// Đọc từ `app_setting`, bỏ qua giá trị rác (0, âm hoặc không đọc được) và
    /// giữ mốc tăng dần để nhóm xếp không bị lộn xộn.
    pub(crate) fn from_settings(raw: &std::collections::HashMap<String, String>) -> Self {
        let num = |key: &str, default: f64| match raw.get(key).map(|v| v.trim().parse::<f64>()) {
            Some(Ok(v)) if v > 0.0 => v,
            _ => default,
        };
        let exempt = num("tax_threshold_exempt", THRESHOLD_EXEMPT);
        let mut group3 = num("tax_threshold_group3", THRESHOLD_GROUP3).max(exempt);
        let mut group4 = num("tax_threshold_group4", THRESHOLD_GROUP4).max(group3);
        // Các mốc phải tăng dần: group3 ≥ ngưỡng, group4 ≥ group3.
        group3 = group3.max(exempt);
        group4 = group4.max(group3);
        Self {
            exempt,
            group3,
            group4,
        }
    }
}

/// Xếp nhóm hộ theo tổng doanh thu cả năm (Điều 7 khoản 2 Luật TNCN + Điều 10 khoản 1).
pub(crate) fn tax_group_with(total_revenue_year: f64, th: &TaxThresholds) -> i64 {
    if total_revenue_year <= th.exempt {
        1
    } else if total_revenue_year <= th.group3 {
        2
    } else if total_revenue_year <= th.group4 {
        3
    } else {
        4
    }
}

/// Nhóm đã chốt trong hồ sơ có khác nhóm app xếp từ doanh thu không.
///
/// Trả true khi hồ sơ ghi một nhóm khác nhóm suy ra từ doanh thu — trường hợp
/// này làm số thuế trên tờ khai khó hiểu (GTGT vẫn tính vì không thuộc diện miễn,
/// TNCN theo doanh thu bằng 0 vì chưa vượt ngưỡng 01 tỷ) nên UI cần cảnh báo.
pub(crate) fn group_mismatch(stored: Option<i64>, auto_group: i64) -> bool {
    stored
        .filter(|g| (1..=4).contains(g))
        .is_some_and(|g| g != auto_group)
}

/// Nhóm hộ dùng cho tờ khai, theo thứ tự ưu tiên:
///   1. nhóm đã chốt trong hồ sơ HKD (cao nhất — người dùng/cơ quan thuế chỉ định),
///   2. nhóm app đã ghim cho năm tính thuế này (đóng băng theo năm),
///   3. nhóm xếp từ doanh thu cả năm khi hộ chưa chốt nhóm nào.
pub(crate) fn effective_tax_group(stored: Option<i64>, frozen: Option<i64>, auto: i64) -> i64 {
    stored
        .filter(|g| (1..=4).contains(g))
        .or(frozen.filter(|g| (1..=4).contains(g)))
        .unwrap_or(auto)
}

/// Thuế suất TNCN theo thu nhập (Điều 7 khoản 2 Luật TNCN 109/2025).
pub(crate) fn profit_tncn_rate(group: i64) -> f64 {
    match group {
        2 => 0.15,
        3 => 0.17,
        _ => 0.20,
    }
}

/// Số thuế TNCN quyết toán cả năm.
///
/// * Hộ nộp TNCN theo tỷ lệ % trên doanh thu (`by_revenue`) thì số quyết toán
///   chính là **tổng số đã tạm nộp** trong năm (Điều 10 NĐ 68/2026) — không
///   tính lại theo thu nhập tính thuế, nếu không hai ô trong card quyết toán
///   mâu thuẫn với nhau (số quyết toán ≠ 0 nhưng chênh lệch ép bằng 0).
/// * Phương pháp thu nhập: thu nhập tính thuế × thuế suất, nhưng chỉ khi hộ
///   thuộc diện nộp thuế cả năm (Điều 8 khoản 1a); không thì bằng 0.
pub(crate) fn settlement_tax(
    by_revenue: bool,
    provisional_total: f64,
    year_taxable: bool,
    income: f64,
    rate: f64,
) -> f64 {
    if by_revenue {
        round2(provisional_total)
    } else if year_taxable {
        round2(income.max(0.0) * rate)
    } else {
        0.0
    }
}

/// Doanh thu thuần của 1 nhóm ngành trong kỳ (đã trừ khoản giảm doanh thu).
pub(crate) fn net_revenue(r: &TaxAgg) -> f64 {
    r.revenue_up - r.revenue_down
}

/// Tổng doanh thu cả năm — cơ sở xếp nhóm hộ và so với ngưỡng 01 tỷ.
pub(crate) fn year_revenue(rows: &[TaxAgg]) -> f64 {
    rows.iter().map(net_revenue).sum()
}

// ─── Phân bổ mức trừ ngưỡng 01 tỷ cho các nhóm ngành ───
//
// Luật TNCN Điều 7 khoản 3 điểm a: "doanh thu tính thuế được xác định bằng
// **phần doanh thu vượt trên** mức không phải nộp thuế" → phải trừ ngưỡng 01
// tỷ/năm trước, rồi mới nhân tỷ lệ % của từng nhóm ngành.
// NĐ 68/2026 Điều 4 khoản 3: khi có nhiều nhóm ngành áp mức thuế suất khác
// nhau, hộ được **tự chọn** nhóm nào áp dụng mức trừ, theo phương án có lợi
// nhất, tổng mức trừ không vượt quá ngưỡng/năm.

/// Mức trừ ngưỡng còn được dùng trong năm, tính theo doanh thu của từng nhóm.
///
/// `override` là lựa chọn của hộ (JSON `{"PPHH": 800000000}`); rỗng thì chia
/// theo tỷ lệ doanh thu của từng nhóm — chia đều là cách có lợi nhất khi các
/// nhóm cùng tỷ lệ % và là mức mặc định hợp lý khi hộ chưa chốt.
pub(crate) fn exempt_alloc(
    rows: &[TaxAgg],
    threshold: f64,
    chosen: &TaxableExemptChoice,
) -> Vec<(String, f64)> {
    let total: f64 = rows.iter().map(net_revenue).filter(|v| *v > 0.0).sum();
    let target = threshold.min(total.max(0.0));
    match chosen {
        TaxableExemptChoice::Manual(map) if !map.is_empty() => rows
            .iter()
            .filter_map(|r| {
                let v = map.get(&r.industry_code).copied().unwrap_or(0.0);
                (v > 0.0).then(|| (r.industry_code.clone(), round2(v)))
            })
            .collect(),
        _ => {
            if total <= 0.0 {
                return Vec::new();
            }
            rows.iter()
                .filter_map(|r| {
                    let net = net_revenue(r);
                    (net > 0.0).then(|| (r.industry_code.clone(), round2(target * net / total)))
                })
                .collect()
        }
    }
}

/// Cách hộ chọn phân bổ mức trừ ngưỡng cho các nhóm ngành.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) enum TaxableExemptChoice {
    /// Chưa chốt → chia theo tỷ lệ doanh thu.
    #[default]
    Auto,
    /// Hộ tự chọn mức trừ cho từng nhóm ngành (Điều 4 khoản 3).
    Manual(std::collections::HashMap<String, f64>),
}

impl TaxableExemptChoice {
    /// Đọc từ JSON trong `app_setting` (`pit_exempt_alloc`). JSON sai → coi như
    /// chưa chốt, không làm hỏng cả tờ khai.
    pub(crate) fn from_json(raw: &str) -> Self {
        let raw = raw.trim();
        if raw.is_empty() || raw == "{}" {
            return Self::Auto;
        }
        match serde_json::from_str::<std::collections::HashMap<String, f64>>(raw) {
            Ok(map) if !map.is_empty() => {
                Self::Manual(map.into_iter().map(|(k, v)| (k, v.max(0.0))).collect())
            }
            _ => Self::Auto,
        }
    }
}

/// Kỳ tính thuế đầu tiên phải nộp thuế trong năm: hộ vượt ngưỡng giữa năm thì
/// "khai, nộp thuế kể từ **quý phát sinh** doanh thu trên [ngưỡng] trong năm"
/// (NĐ 68/2026 Điều 8 khoản 1 điểm a).
///
/// Nhận doanh thu lũy kế theo từng tháng (1..=12) → trả về (tháng đầu, tháng
/// cuối) của kỳ tính thuế, hoặc `None` khi cả năm không vượt ngưỡng (chỉ thông
/// báo doanh thu). `months` = số tháng trong 1 kỳ khai (3 = quý, 1 = tháng).
pub(crate) fn first_taxable_period(
    cumulative: &[f64; 13],
    threshold: f64,
    months_per_period: i64,
) -> Option<(i64, i64)> {
    let cross = (1i64..=12).find(|m| cumulative[*m as usize] > threshold)?;
    let from = ((cross - 1) / months_per_period) * months_per_period + 1;
    let to = (from + months_per_period - 1).min(12);
    Some((from, to))
}

/// Hạn nộp hồ sơ khai thuế (NĐ 68/2026 khoản 3 Điều 10):
///   • theo quý  → chậm nhất ngày cuối cùng của tháng đầu tiên của quý tiếp theo
///   • theo tháng → chậm nhất ngày thứ 20 của tháng tiếp theo
///   • nhóm 1    → thông báo doanh thu chậm nhất 31/01 của năm sau (Điều 8 khoản 1a)
///   • quyết toán thu nhập tính thuế theo năm → 31/03 của năm sau
/// Trả về chuỗi `dd/mm/yyyy`; kỳ không hợp lệ → `None`.
pub(crate) fn tax_deadline(period: &str, no: i64, year: i64) -> Option<String> {
    match period {
        "month" if (1..=12).contains(&no) => {
            let (y, m) = if no == 12 {
                (year + 1, 1)
            } else {
                (year, no + 1)
            };
            Some(format!("20/{:02}/{:04}", m, y))
        }
        "quarter" if (1..=4).contains(&no) => {
            let (m, y) = if no == 4 {
                (1, year + 1)
            } else {
                (no * 3 + 1, year)
            };
            let last = last_day_of_month(y, m);
            Some(format!("{:02}/{:02}/{:04}", last.0, last.1, last.2))
        }
        // Nhóm 1: thông báo doanh thu thực tế trong năm chậm nhất 31/01 năm sau.
        "year" | "per_occurrence" | "occurrence" => Some(format!("31/01/{}", year + 1)),
        _ => None,
    }
}

/// Ngày cuối tháng `(year, month)` → `(ngày, tháng, năm)`.
fn last_day_of_month(year: i64, month: i64) -> (i64, i64, i64) {
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        // Tháng 2: năm nhuận là năm chia hết 4 và (không chia hết 100 hoặc chia hết 400).
        2 => {
            if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 {
                29
            } else {
                28
            }
        }
        _ => 30,
    };
    (days, month, year)
}

// ─── GIÁ VỐN FIFO từ phiếu nhập ───

/// Một lượt nhập kho (từ phiếu PN) để tính giá vốn FIFO: `(mã hàng, số lượng, đơn giá)`.
pub(crate) type Receipt = (String, f64, f64);
/// Một lượt xuất kho (từ phiếu PX) cần tính giá vốn: `(mã hàng, số lượng)`.
pub(crate) type Issue = (String, f64);

/// Giá vốn FIFO của các lượt xuất trong `issues`, khớp với `receipts` đã sắp
/// xếp theo thời gian (lô nhập trước xuất trước) — cùng cách `allocate_fifo`
/// dùng lúc lập phiếu xuất.
///
/// Nhập trước rồi xuất sau trong cùng một mã hàng: tồn lô dư = tổng nhập − tổng
/// xuất đã tính. Xuất vượt tồn (dữ liệu lệch) → lấy hết tồn còn lại, phần
/// thiếu tính bằng 0 thay vì làm hỏng cả tờ khai.
/// Giá vốn FIFO của **từng** lượt xuất trong `issues`, giữ nguyên thứ tự của
/// `issues` (chưa làm tròn từng lượt — cộng lại rồi làm tròn một lần nên khớp
/// chẵn với [`fifo_cogs`]).
///
/// Dùng khi ghi bổ sung bút toán Nợ 632 / Có 152 cho phiếu xuất lịch sử: mỗi
/// lượt xuất cần một dòng giá vốn riêng, gộp lại thì không ghi được.
pub(crate) fn fifo_cogs_per_issue(receipts: &[Receipt], issues: &[Issue]) -> Vec<f64> {
    // Tồn lô theo mã hàng: Vec<(số lượng còn, đơn giá)> theo thứ tự nhập.
    let mut lots: std::collections::HashMap<&str, Vec<(f64, f64)>> =
        std::collections::HashMap::new();
    for (code, qty, price) in receipts {
        if *qty > 0.0 {
            lots.entry(code.as_str()).or_default().push((*qty, *price));
        }
    }
    let mut costs = Vec::with_capacity(issues.len());
    for (code, qty) in issues {
        let mut cost = 0.0;
        // Xuất âm / bằng 0 (điều chỉnh giảm) không có giá vốn — vẫn giữ một ô
        // trong kết quả để chỉ số khớp với `issues`.
        if *qty > 0.0 {
            let mut remaining = *qty;
            if let Some(queue) = lots.get_mut(code.as_str()) {
                for lot in queue.iter_mut() {
                    if remaining <= 1e-9 {
                        break;
                    }
                    let take = remaining.min(lot.0);
                    cost += take * lot.1;
                    lot.0 -= take;
                    remaining -= take;
                }
            }
        }
        costs.push(cost);
    }
    costs
}

/// Giá vốn FIFO của **tất cả** lượt xuất trong `issues` (tổng một lần rồi mới
/// làm tròn) — dùng cho tờ khai và card quyết toán.
pub(crate) fn fifo_cogs(receipts: &[Receipt], issues: &[Issue]) -> f64 {
    round2(fifo_cogs_per_issue(receipts, issues).iter().sum())
}

// ─── CHI PHÍ ĐƯỢC TRỪ (NĐ 68/2026 Điều 6) ───

/// Một khoản chi để quyết định có được trừ hay không:
///   • `deductible` — người dùng đánh dấu "không được trừ" (Điều 6 khoản 2)
///   • `amount` — số tiền
///   • `has_bank_doc` — có chứng từ thanh toán không dùng tiền mặt hay không
pub(crate) struct CostItem {
    pub(crate) deductible: bool,
    pub(crate) amount: f64,
    pub(crate) has_bank_doc: bool,
}

/// Tách chi phí thành (được trừ, không được trừ + lý do) theo Điều 6.
///
/// Không được trừ khi: người dùng đã đánh dấu, hoặc khoản chi từ 5 triệu trở
/// lên thanh toán bằng tiền mặt mà không có chứng từ thanh toán không dùng tiền
/// mặt (khoản 1 điều 6 yêu cầu chứng từ này).
pub(crate) fn split_deductible_cost(items: &[CostItem]) -> (f64, f64) {
    items.iter().fold((0.0, 0.0), |(ok, no), it| {
        let amount = it.amount.max(0.0);
        // Hai trường hợp không được trừ (đã đánh dấu bởi người dùng, hoặc từ 5 triệu
        // trở lên trả tiền mặt không có chứng từ thanh toán không dùng tiền mặt) gộp
        // chung để khớp với danh sách cảnh báo hiển thị trên tờ khai.
        if !it.deductible || (amount >= NON_CASH_PAYMENT_LIMIT && !it.has_bank_doc) {
            (ok, no + amount)
        } else {
            (ok + amount, no)
        }
    })
}

/// Nhóm hộ đã chốt trong hồ sơ HKD (NULL = chưa chốt → tự xếp theo doanh thu).
pub(crate) async fn load_stored_tax_group(pool: &SqlitePool) -> Result<Option<i64>, String> {
    sqlx::query_scalar!("SELECT tax_group FROM business WHERE id = 1")
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())
}

/// Cấu hình kê khai thuế của hộ đọc từ `app_setting`.
#[derive(Debug, Clone)]
pub(crate) struct TaxSettings {
    /// Kỳ khai: "quarter" | "month" | "year" | "per_occurrence"
    pub(crate) period: String,
    /// Phương pháp tính TNCN: "revenue" (tỷ lệ % doanh thu) | "profit" (thu nhập)
    pub(crate) method: String,
    /// Hộ tự chọn cách phân bổ mức trừ ngưỡng 01 tỷ cho các nhóm ngành
    /// (NĐ 68/2026 Điều 4 khoản 3).
    pub(crate) exempt_choice: TaxableExemptChoice,
    /// Ba mốc doanh thu (sửa được ở màn Cài đặt).
    pub(crate) thresholds: TaxThresholds,
    /// Nhóm hộ đã chốt riêng cho từng năm — xem `FrozenGroups`.
    pub(crate) group_by_year: FrozenGroups,
}

/// Nhóm hộ đã "chốt" cho từng năm tính thuế, lưu dạng JSON `{"2026": 2}`.
///
/// Nguyên tắc ổn định: khi doanh thu giữa năm vượt sang nhóm kế tiếp (ví dụ đang
/// Nhóm 2 mà vượt 03 tỷ) thì kỳ tính thuế của năm đó **vẫn giữ nhóm cũ**, từ năm
/// sau mới chuyển — đúng như ý kiến chính thức của Thuế cơ sở Tây Ninh trả lời
/// báo Chính phủ về trường hợp vượt 03 tỷ trong năm (dẫn điểm d khoản 5 Điều 4
/// NĐ 68/2026). Nhờ vậy nhóm tính thuế không bị đổi giữa chừng khi doanh thu tăng
/// vượt mốc.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct FrozenGroups(pub(crate) std::collections::HashMap<i64, i64>);

impl FrozenGroups {
    /// JSON sai kiểu thì coi như chưa chốt gì — không làm hỏng tờ khai.
    pub(crate) fn from_json(raw: Option<&String>) -> Self {
        let Some(raw) = raw.map(|v| v.trim().to_string()) else {
            return Self::default();
        };
        if raw.is_empty() || raw == "{}" {
            return Self::default();
        }
        let parsed: std::collections::HashMap<String, f64> =
            serde_json::from_str(&raw).unwrap_or_default();
        let map = parsed
            .into_iter()
            .filter_map(|(k, v)| k.trim().parse::<i64>().ok().map(|y| (y, v as i64)))
            .filter(|(_, g)| (1..=4).contains(g))
            .collect();
        Self(map)
    }

    pub(crate) fn get(&self, year: i64) -> Option<i64> {
        self.0.get(&year).copied()
    }

    /// Thêm nhóm đã chốt của một năm (bỏ qua năm đã có để không ghi đè).
    pub(crate) fn set_if_absent(&mut self, year: i64, group: i64) -> bool {
        if self.0.contains_key(&year) || !(1..=4).contains(&group) {
            return false;
        }
        self.0.insert(year, group);
        true
    }

    pub(crate) fn to_json(&self) -> String {
        serde_json::to_string(&self.0).unwrap_or_else(|_| "{}".to_string())
    }
}

/// Đọc cấu hình kê khai thuế: (kỳ khai, phương pháp TNCN, cách phân bổ mức trừ).
/// Mặc định: khai theo quý + TNCN theo doanh thu + chia mức trừ theo tỷ lệ DT.
pub(crate) async fn load_tax_settings(pool: &SqlitePool) -> Result<TaxSettings, String> {
    let rows: Vec<(String, String)> = sqlx::query!(
        r#"SELECT key as "key!", value FROM app_setting
            WHERE key IN ('tax_period', 'tax_method', 'pit_exempt_alloc',
                          'tax_threshold_exempt', 'tax_threshold_group3',
                          'tax_threshold_group4', 'tax_group_by_year')"#
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?
    .into_iter()
    .map(|r| (r.key, r.value))
    .collect();
    let mut period = "quarter".to_string();
    let mut method = "revenue".to_string();
    let mut alloc = String::new();
    let mut raw_settings: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for (k, v) in rows {
        match k.as_str() {
            "tax_period" => period = v,
            "tax_method" => method = v,
            "pit_exempt_alloc" => alloc = v,
            other => {
                raw_settings.insert(other.to_string(), v);
            }
        }
    }
    Ok(TaxSettings {
        period,
        method,
        exempt_choice: TaxableExemptChoice::from_json(&alloc),
        thresholds: TaxThresholds::from_settings(&raw_settings),
        group_by_year: FrozenGroups::from_json(raw_settings.get("tax_group_by_year")),
    })
}

/// Số tháng trong 1 kỳ khai — quý = 3, tháng = 1, năm = 12.
pub(crate) fn months_per_period(period: &str) -> i64 {
    match period {
        "month" => 1,
        "year" | "per_occurrence" | "occurrence" => 12,
        _ => 3,
    }
}

/// Chuyển kỳ khai (year/quarter/month/occurrence) + số thứ tự kỳ thành khoảng tháng.
fn period_range(period: &str, no: i64) -> Result<(i64, i64), String> {
    match period {
        "year" | "occurrence" | "per_occurrence" => Ok((1, 12)),
        "month" => {
            if (1..=12).contains(&no) {
                Ok((no, no))
            } else {
                Err("Tháng kê khai không hợp lệ".into())
            }
        }
        "quarter" => {
            if (1..=4).contains(&no) {
                Ok((no * 3 - 2, no * 3))
            } else {
                Err("Quý kê khai không hợp lệ".into())
            }
        }
        _ => Err("Kỳ khai thuế không hợp lệ".into()),
    }
}

/// Doanh thu lũy kế của từng nhóm ngành từ đầu năm đến hết tháng `to_month`
/// (không gồm kỳ đang xét) — cơ sở trừ ngưỡng theo phương pháp lũy kế.
pub(crate) async fn load_cum_revenue_before(
    pool: &SqlitePool,
    year: i64,
    from_month: i64,
) -> Result<std::collections::HashMap<String, f64>, String> {
    let rows = load_tax_agg_range(pool, year, 1, from_month - 1).await?;
    Ok(rows
        .iter()
        .map(|r| (r.industry_code.clone(), net_revenue(r)))
        .collect())
}

/// Giá vốn FIFO trong khoảng `from..=to` (ngày ISO), tính từ phiếu nhập (PN)
/// và phiếu xuất (PX) trong `journal_entry`.
///
/// Nhập lấy **từ trước `from`** để kỳ tính thuế sau vẫn dùng được tồn đầu kỳ;
/// xuất chỉ lấy trong khoảng `from..=to` vì đó là giá vốn của kỳ đang tính.
/// Hàng dịch vụ không theo dõi tồn nên không có giá vốn (khớp lúc lập phiếu xuất).
/// Phiếu điều chỉnh giảm (khách trả lại) loại khỏi lượt xuất: app đã nhập lại hàng
/// (bút Nợ 152 / Có 632) chứ không tiêu kho, coi là lượt xuất thì lô FIFO hụt đi
/// so với sổ kho và giá vốn của các lần bán sau bị đẩy lên.
pub(crate) async fn load_fifo_cogs(pool: &SqlitePool, from: &str, to: &str) -> Result<f64, String> {
    let receipts: Vec<Receipt> = sqlx::query!(
        r#"SELECT je.product_code AS "product_code!", je.quantity, je.unit_price
             FROM journal_entry je
             JOIN product p ON p.code = je.product_code
            WHERE je.entry_type = 'PN'
              AND je.posting_date <= ?
              AND je.quantity > 0
              AND COALESCE(p.is_service, 0) = 0
            ORDER BY je.posting_date, je.id"#,
        to
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?
    .into_iter()
    .map(|r| (r.product_code, r.quantity, r.unit_price))
    .collect();
    let issues: Vec<Issue> = sqlx::query!(
        r#"SELECT je.product_code AS "product_code!", je.quantity
             FROM journal_entry je
             JOIN product p ON p.code = je.product_code
            WHERE je.entry_type = 'PX'
              AND je.posting_date >= ? AND je.posting_date <= ?
              AND je.quantity > 0
              AND COALESCE(je.adjust_code, '') NOT IN ('GiamDT', 'XuatNVL')
              AND COALESCE(p.is_service, 0) = 0
            ORDER BY je.posting_date, je.id"#,
        from,
        to
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?
    .into_iter()
    .map(|r| (r.product_code, r.quantity))
    .collect();
    Ok(fifo_cogs(&receipts, &issues))
}

/// Một lượt xuất kho kèm đủ thông tin để ghi bút toán giá vốn lịch sử.
#[derive(sqlx::FromRow)]
pub(crate) struct IssueRow {
    pub(crate) voucher_no: String,
    pub(crate) posting_date: String,
    pub(crate) description: String,
    pub(crate) product_code: String,
    pub(crate) customer_code: String,
    pub(crate) unit_code: String,
    pub(crate) industry_code: String,
    pub(crate) quantity: f64,
    pub(crate) note: String,
}

/// Số bút toán giá vốn (Nợ 632 / Có 152) còn thiếu cho phiếu xuất lịch sử.
#[derive(serde::Serialize, Default, Clone, Copy)]
pub(crate) struct CogsBackfillPending {
    /// Số lượt xuất chưa có bút toán giá vốn.
    pub(crate) count: u32,
    /// Tổng giá vốn FIFO của các lượt đó (đồng, làm tròn 2 số).
    pub(crate) amount: f64,
}

/// Tải toàn bộ lượt nhập (đầu vào của FIFO) và lượt xuất (cần ghi bút toán).
///
/// Không giới hạn khoảng ngày: bút toán giá vốn là **lịch sử** — kỳ nào cũng
/// phải được ghi một lần, kể cả khi card quyết toán đang xem một kỳ khác.
async fn load_fifo_rows(pool: &SqlitePool) -> Result<(Vec<Receipt>, Vec<IssueRow>), String> {
    let receipts: Vec<Receipt> = sqlx::query!(
        r#"SELECT je.product_code AS "product_code!", je.quantity, je.unit_price
             FROM journal_entry je
             JOIN product p ON p.code = je.product_code
            WHERE je.entry_type = 'PN'
              AND je.quantity > 0
              AND COALESCE(p.is_service, 0) = 0
            ORDER BY je.posting_date, je.id"#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?
    .into_iter()
    .map(|r| (r.product_code, r.quantity, r.unit_price))
    .collect();
    let issues: Vec<IssueRow> = sqlx::query_as!(
        IssueRow,
        r#"SELECT je.voucher_no AS "voucher_no!",
                  je.posting_date AS "posting_date!",
                  je.description AS "description!",
                  je.product_code AS "product_code!",
                  je.customer_code AS "customer_code!",
                  je.unit_code AS "unit_code!",
                  je.industry_code AS "industry_code!",
                  je.quantity,
                  je.note AS "note!"
             FROM journal_entry je
             JOIN product p ON p.code = je.product_code
            WHERE je.entry_type = 'PX'
              AND je.quantity > 0
              -- Khách trả lại app đã ghi bút Nợ 152 / Có 632 rồi, không có dòng
              -- 632/152 nào để bổ sung (ghi thêm thì sẽ sai chiều).
              -- 'XuatNVL' = phiếu xuất NVL tự sinh theo định mức (F4): đã ghi Nợ
              -- 154 / Có 152 rồi — bổ sung thêm Nợ 632 / Có 152 là credit 152 lần 2.
              AND COALESCE(je.adjust_code, '') NOT IN ('GiamDT', 'XuatNVL')
              AND COALESCE(p.is_service, 0) = 0
            ORDER BY je.posting_date, je.id"#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok((receipts, issues))
}

/// Số bút toán `CP` đã ghi cho từng cặp (số phiếu, mã hàng).
async fn load_cogs_entry_counts(
    pool: &SqlitePool,
) -> Result<std::collections::HashMap<(String, String), u32>, String> {
    let rows = sqlx::query!(
        r#"SELECT voucher_no AS "voucher_no!", product_code AS "product_code!", COUNT(*) AS "count!: i64"
             FROM journal_entry
            WHERE entry_type = 'CP'
            GROUP BY voucher_no, product_code"#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|r| ((r.voucher_no, r.product_code), r.count as u32))
        .collect())
}

/// Các lượt xuất chưa có bút toán giá vốn, theo thứ tự ngày — tức là kế hoạch
/// ghi bổ sung. Cặp (số phiếu, mã hàng) đã có bút toán thì lượt xuất kế nó coi
/// như **đã ghi** nên không viết lại → chạy bao nhiêu lần cũng không trùng.
///
/// Trả về rỗng khi `costs` không khớp `issues` để không bao giờ ghi nhầm số.
fn cogs_backfill_plan(
    issues: &[IssueRow],
    costs: &[f64],
    existing: &std::collections::HashMap<(String, String), u32>,
) -> Vec<usize> {
    if costs.len() != issues.len() {
        return Vec::new();
    }
    let mut remaining = existing.clone();
    let mut pending = Vec::new();
    for (i, issue) in issues.iter().enumerate() {
        // Không có giá vốn (hàng giá 0, xuất vượt tồn) → không có gì để ghi;
        // loại khỏi kế hoạch để ô cảnh báo không treo hoài với bút toán vô nghĩa.
        if costs[i] <= 0.0 {
            continue;
        }
        let key = (issue.voucher_no.clone(), issue.product_code.clone());
        match remaining.get_mut(&key) {
            // Đã có ít nhất một bút toán CP cho cặp này → tiêu một suất.
            Some(n) if *n > 0 => *n -= 1,
            _ => pending.push(i),
        }
    }
    pending
}

/// Giá vốn FIFO cho một danh sách lượt xuất (chỉ số khớp `issues`).
fn issue_costs(receipts: &[Receipt], issues: &[IssueRow]) -> Vec<f64> {
    let issues_arg: Vec<Issue> = issues
        .iter()
        .map(|i| (i.product_code.clone(), i.quantity))
        .collect();
    fifo_cogs_per_issue(receipts, &issues_arg)
}

/// (số lượt xuất còn thiếu bút toán giá vốn, tổng giá vốn của chúng).
async fn cogs_backfill_summary(pool: &SqlitePool) -> Result<CogsBackfillPending, String> {
    let (receipts, issues) = load_fifo_rows(pool).await?;
    let existing = load_cogs_entry_counts(pool).await?;
    let costs = issue_costs(&receipts, &issues);
    let plan = cogs_backfill_plan(&issues, &costs, &existing);
    Ok(CogsBackfillPending {
        count: plan.len() as u32,
        amount: round2(plan.iter().map(|&i| costs[i]).sum()),
    })
}

/// Số lượt xuất còn thiếu bút toán Nợ 632 / Có 152 (dùng để hiện nút ghi bổ
/// sung ở card Tổng hợp doanh thu - chi phí).
#[tauri::command]
pub(crate) async fn cogs_backfill_pending(state: State<'_, AppState>) -> Result<String, String> {
    let pool = state.pool.read().await;
    let pending = cogs_backfill_summary(&pool).await?;
    serde_json::to_string(&pending).map_err(|e| e.to_string())
}

/// Ghi bút toán Nợ 632 / Có 152 cho các phiếu xuất lịch sử chưa có giá vốn.
///
/// Giá vốn tính lại đúng bằng `fifo_cogs_per_issue` (cùng logic FIFO với lúc lập
/// phiếu xuất) nên sổ cái khớp với card quyết toán. Lặp lại được: lần hai không
/// còn lượt nào thiếu.
#[tauri::command]
pub(crate) async fn backfill_cogs_entries(state: State<'_, AppState>) -> Result<String, String> {
    let pool = state.pool.read().await;
    let (receipts, issues) = load_fifo_rows(&pool).await?;
    let existing = load_cogs_entry_counts(&pool).await?;
    let costs = issue_costs(&receipts, &issues);
    let plan = cogs_backfill_plan(&issues, &costs, &existing);
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let mut written = 0_u32;
    let mut amount = 0.0_f64;
    for &idx in &plan {
        let issue = &issues[idx];
        let cost = round2(costs[idx]);
        // Xuất vượt tồn (dữ liệu lệch) không có giá vốn → bỏ qua thay vì tạo
        // bút toán 632/152 bằng 0 làm lệch bảng cân đối.
        if cost <= 0.0 {
            continue;
        }
        insert_journal_entry(
            &mut tx,
            &issue.posting_date,
            &issue.voucher_no,
            "CP",
            &issue.description,
            &issue.product_code,
            "",
            &issue.customer_code,
            issue.quantity,
            cost / issue.quantity,
            cost,
            "632",
            "152",
            &issue.industry_code,
            0.0,
            0.0,
            &issue.unit_code,
            "",
            &issue.note,
            0.0,
        )
        .await?;
        written += 1;
        amount += cost;
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    serde_json::to_string(&CogsBackfillPending {
        count: written,
        amount: round2(amount),
    })
    .map_err(|e| e.to_string())
}

/// Chi phí trong kỳ: (được trừ, không được trừ, danh sách vi phạm).
///
/// Quy tắc Điều 6 NĐ 68/2026:
///   • khoản chi người dùng đánh dấu "không được trừ" → không trừ (khoản 2)
///   • khoản chi từ 05 triệu trở lên trả bằng tiền mặt mà không có chứng từ
///     thanh toán không dùng tiền mặt → không trừ (khoản 1, `bank_code` rỗng)
pub(crate) async fn load_period_costs(
    pool: &SqlitePool,
    from: &str,
    to: &str,
) -> Result<(f64, f64, Vec<TaxCostWarning>), String> {
    let rows = sqlx::query!(
        r#"SELECT je.posting_date AS "posting_date!", je.voucher_no AS "voucher_no!",
                  je.description AS "description!", je.amount, je.bank_code AS "bank_code!",
                  je.deductible,
                  je.non_deductible_reason AS "non_deductible_reason!"
             FROM journal_entry je
            WHERE je.entry_type = 'PC'
              AND je.posting_date >= ? AND je.posting_date <= ?
              AND COALESCE(je.adjust_code, '') != 'GiamCP'
              -- PC trả nhà cung cấp (Nợ 331 / Có 111) là **thanh toán phải trả**,
              -- không phải chi phí: tiền hàng đã nằm ở giá vốn khi xuất kho. Cộng
              -- vào đây là tính trùng hàng mua (vd. PC001 = đúng tổng PN0131).
              AND COALESCE(je.debit_account, '') != '331'
            ORDER BY je.posting_date, je.id"#,
        from,
        to
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    let mut warnings: Vec<TaxCostWarning> = Vec::new();
    let items: Vec<CostItem> = rows
        .iter()
        .map(|r| {
            let has_bank_doc = !r.bank_code.trim().is_empty();
            let reason = if r.deductible == 0 {
                let why = r.non_deductible_reason.trim();
                if why.is_empty() {
                    "Người dùng đánh dấu không được trừ".to_string()
                } else {
                    why.to_string()
                }
            } else if r.amount >= NON_CASH_PAYMENT_LIMIT && !has_bank_doc {
                format!(
                    "Từ {} triệu đồng thanh toán tiền mặt, không có chứng từ thanh toán không dùng tiền mặt",
                    NON_CASH_PAYMENT_LIMIT / 1_000_000.0
                )
            } else {
                String::new()
            };
            if !reason.is_empty() {
                warnings.push(TaxCostWarning {
                    posting_date: r.posting_date.clone(),
                    voucher_no: r.voucher_no.clone(),
                    description: r.description.clone(),
                    amount: round2(r.amount),
                    reason: reason.clone(),
                });
            }
            CostItem {
                deductible: reason.is_empty(),
                amount: r.amount,
                has_bank_doc,
            }
        })
        .collect();
    let (ok, no) = split_deductible_cost(&items);
    Ok((round2(ok), round2(no), warnings))
}

/// Kỳ này đã phát sinh nghĩa vụ thuế chưa.
///
/// NĐ 68/2026 Điều 8 khoản 1 điểm a: hộ có doanh thu năm từ 01 tỷ trở xuống thì
/// chỉ thông báo; **"phát sinh doanh thu thực tế trên [ngưỡng] trong năm thì khai,
/// nộp thuế kể từ quý phát sinh"**. Nghĩa là kỳ mà doanh thu lũy kế tới cuối kỳ
/// vẫn chưa vượt ngưỡng thì chưa phát sinh thuế GTGT lẫn TNCN — kể cả khi hộ đã
/// thuộc nhóm 2, vì nhóm 2 được xếp từ tổng doanh thu cả năm.
///
/// `cum_before` = doanh thu lũy kế từ đầu năm tới trước kỳ, `in_period` = doanh
/// thu của kỳ đang xem.
///
/// `taxed_from_start` = hộ đã chốt Nhóm 2/3/4 trong hồ sơ HKD (do cơ quan thuế
/// xác định): hộ đã thuộc diện nộp thuế cả năm nên kỳ nào có doanh thu là phải
/// nộp, không chờ vượt ngưỡng. Ngược lại (hồ sơ để Nhóm 1 hoặc chưa chốt) thì
/// áp dụng đúng Điều 8 khoản 1a: chỉ nộp từ kỳ doanh thu lũy kế vượt ngưỡng.
///
/// Lưu ý hàm này chỉ quyết định **có phát sinh nghĩa vụ khai hay không** (cổng
/// GTGT). Mức trừ ngưỡng của TNCN là chuyện khác, do `exempt_alloc` +
/// `taxable_by_period` xử lý theo doanh thu lũy kế của các kỳ trước + kỳ này.
pub(crate) fn period_is_taxable(
    cum_before: f64,
    in_period: f64,
    threshold: f64,
    taxed_from_start: bool,
) -> bool {
    if taxed_from_start {
        return in_period > 0.0;
    }
    cum_before + in_period > threshold
}

/// Tách kỳ của một nhóm ngành thành `(doanh thu tính thuế, mức trừ đã dùng)`.
///
/// NĐ 68/2026 Điều 8 khoản 1 điểm a: hộ vượt ngưỡng giữa năm thì "khai, nộp thuế
/// kể từ quý phát sinh" → mức trừ 01 tỷ được **dùng dần theo doanh thu lũy kế**:
/// mỗi kỳ khấu trừ tiếp phần mức trừ chưa dùng của các kỳ trước, nên tổng các kỳ
/// trong năm luôn bằng đúng mức trừ một lần (và không bao giờ trừ quá doanh thu
/// thật). Trả về cả `deducted` để tờ khai ghi rõ kỳ này đã trừ được bao nhiêu.
pub(crate) fn split_by_period(
    cum_before: &std::collections::HashMap<String, f64>,
    period: &std::collections::HashMap<String, f64>,
    alloc: &[(String, f64)],
) -> Vec<(String, f64, f64)> {
    period
        .iter()
        .map(|(code, in_period)| {
            let exempt = alloc
                .iter()
                .find(|(c, _)| c == code)
                .map(|(_, v)| *v)
                .unwrap_or(0.0);
            let before = cum_before.get(code).copied().unwrap_or(0.0);
            let end = (before + in_period - exempt).max(0.0);
            let start = (before - exempt).max(0.0);
            let taxable = end - start;
            // Phần doanh thu của kỳ này bị mức trừ khấu đi.
            let deducted = (in_period - taxable).max(0.0);
            (code.clone(), round2(taxable), round2(deducted))
        })
        .collect()
}

/// Rút cặp `(mã, doanh thu tính thuế)` từ `split_by_period` cho chỗ chỉ cần cơ sở
/// tính thuế.
pub(crate) fn taxable_by_period(
    cum_before: &std::collections::HashMap<String, f64>,
    period: &std::collections::HashMap<String, f64>,
    alloc: &[(String, f64)],
) -> Vec<(String, f64)> {
    split_by_period(cum_before, period, alloc)
        .into_iter()
        .map(|(code, taxable, _)| (code, taxable))
        .collect()
}

/// Tính số thuế phải nộp từ dữ liệu gộp (thuần → test được):
///   Nhóm 1 (≤ 01 tỷ)          → miễn thuế GTGT + TNCN
///   Nhóm 2 (> 01–3 tỷ):
///     - GTGT = (DT tăng − DT giảm) × tỷ lệ ngành
///     - TNCN = doanh thu TÍNH THUẾ (đã trừ ngưỡng) × tỷ lệ ngành
///   Nhóm 3/4 (> 3 tỷ)        → TNCN theo thu nhập tính thuế × 17%/20% (ở tổng hợp)
/// `taxable` là doanh thu tính thuế TNCN của từng nhóm trong kỳ (map theo mã).
pub(crate) fn build_tax_declaration(
    rows: Vec<TaxAgg>,
    method: &str,
    group: i64,
    taxable: &std::collections::HashMap<String, f64>,
    // false = kỳ nằm trước khi hộ vượt ngưỡng 01 tỷ → chưa phát sinh thuế
    // (Điều 8 khoản 1a) dù hộ đã thuộc nhóm 2.
    taxable_period: bool,
    // Mức trừ ngưỡng TNCN đã khấu trong kỳ của từng nhóm ngành (map theo mã).
    deducted: &std::collections::HashMap<String, f64>,
) -> Vec<TaxDeclarationRow> {
    // Miễn thuế do NGƯỠNG doanh thu quyết định, không do nhãn nhóm: hộ ghi Nhóm 2
    // trong hồ sơ mà doanh thu chưa vượt ngưỡng vẫn không phát sinh thuế trong kỳ
    // (Điều 8 khoản 1a), và ngược lại hộ ghi Nhóm 1 vẫn phải nộp thuế cho phần
    // doanh thu vượt ngưỡng của các kỳ sau.
    let exempt = !taxable_period;
    rows.into_iter()
        // Nhóm ngành không có doanh thu trong kỳ thì không lên tờ khai — mẫu
        // chỉ liệt kê ngành có phát sinh (phiếu ghi giá 0 không tạo ra dòng
        // báo cáo). Ngành có tăng/giảm bằng nhau vẫn giữ dù doanh thu ròng 0.
        .filter(|r| r.revenue_up != 0.0 || r.revenue_down != 0.0)
        .map(|r| {
            // Giữ mã nhóm ngành lại: dùng cho tra cơ sở tính thuế và mức trừ.
            let code = r.industry_code.clone();
            let base = net_revenue(&r);
            let vat_tax = if exempt {
                0.0
            } else {
                round2(base * r.vat_rate)
            };
            let taxable_rev = if exempt {
                0.0
            } else {
                taxable.get(&code).copied().unwrap_or(0.0).max(0.0)
            };
            // TNCN theo thu nhập tính thuế không tính ở bảng tờ khai theo ngành —
            // số thuế đó tính ở thẻ tổng hợp (và tạm nộp theo tỷ lệ % doanh thu).
            let pit_tax = if exempt || group > 2 || method != "revenue" {
                0.0
            } else {
                round2(taxable_rev * r.pit_rate)
            };
            TaxDeclarationRow {
                industry_code: r.industry_code,
                industry_name: r.industry_name,
                vat_rate: r.vat_rate,
                pit_rate: r.pit_rate,
                revenue_up: round2(r.revenue_up),
                revenue_down: round2(r.revenue_down),
                revenue_taxable: round2(taxable_rev),
                // Mức trừ ngưỡng TNCN đã khấu cho nhóm ngành này trong kỳ — tờ khai
                // cần ghi rõ để thấy doanh thu tính thuế đã trừ được bao nhiêu.
                pit_deduction: round2(deducted.get(&code).copied().unwrap_or(0.0)),
                vat_tax,
                vat_payable: round2(vat_tax),
                pit_tax,
            }
        })
        .collect()
}

/// Nhóm hộ + doanh thu cả năm + cấu hình kê khai — dùng chung cho các lệnh thuế
/// để mọi màn ra cùng một con số.
pub(crate) struct TaxContext {
    pub(crate) settings: TaxSettings,
    pub(crate) group: i64,
    /// Nhóm app tự xếp từ doanh thu cả năm.
    pub(crate) auto_group: i64,
    /// true = nhóm đang dùng lấy từ hồ sơ HKD (người dùng/cơ quan thuế đã chốt),
    /// tức là không tự xếp lại theo doanh thu nữa.
    pub(crate) group_from_profile: bool,
    /// Nhóm app đã ghim cho năm tính thuế này (đã vượt ngưỡng nhưng hồ sơ chưa
    /// chốt) — dùng để hiển thị "nhóm của năm đã chốt, không đổi giữa năm".
    pub(crate) frozen_group: Option<i64>,
    /// true = hồ sơ HKD đã chốt Nhóm 2/3/4 → hộ thuộc diện nộp thuế suốt năm,
    /// mọi kỳ có doanh thu đều phải nộp GTGT, không chờ doanh thu vượt ngưỡng
    /// (mức trừ ngưỡng của TNCN vẫn áp dụng như bình thường).
    pub(crate) taxed_from_start: bool,
    pub(crate) year_revenue: f64,
    pub(crate) alloc: Vec<(String, f64)>,
}

pub(crate) async fn load_tax_context(pool: &SqlitePool, year: i64) -> Result<TaxContext, String> {
    let mut settings = load_tax_settings(pool).await?;
    let year_rows = load_tax_agg(pool, year, 0).await?;
    let year_revenue = year_revenue(&year_rows);
    let stored = load_stored_tax_group(pool)
        .await?
        .filter(|g| (1..=4).contains(g));
    let thresholds = settings.thresholds;
    let auto_group = tax_group_with(year_revenue, &thresholds);
    // Nhóm của năm: hồ sơ HKD (cơ quan thuế chỉ định) > nhóm đã chốt cho năm này >
    // nhóm xếp từ doanh thu. Vượt mốc giữa năm không đổi nhóm của năm đó.
    let mut frozen = settings.group_by_year.get(year);
    let group = effective_tax_group(stored, frozen, auto_group);
    // Hộ đã vượt ngưỡng mà hồ sơ chưa chốt nhóm: ghim nhóm của năm lại để các kỳ
    // trong năm dùng chung một nhóm (nguyên tắc ổn định). Chỉ ghim khi đã vượt
    // ngưỡng — lúc đó nhóm mới có ý nghĩa; chưa vượt thì để Nhóm 1 cho tới lúc.
    if stored.is_none()
        && frozen.is_none()
        && year_revenue > thresholds.exempt
        && settings.group_by_year.set_if_absent(year, auto_group)
    {
        let _ = sqlx::query(
            "INSERT INTO app_setting(key, value) VALUES('tax_group_by_year', ?) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(settings.group_by_year.to_json())
        .execute(pool)
        .await;
        frozen = settings.group_by_year.get(year);
    }
    // Hồ sơ chốt Nhóm 2/3/4 = cơ quan thuế đã xác định hộ thuộc diện nộp thuế cả
    // năm, nên mọi kỳ có doanh thu đều phải khai (không chờ vượt ngưỡng).
    // Lưu ý: điều này KHÔNG đụng tới mức trừ ngưỡng của TNCN — GTGT tính trên toàn
    // bộ doanh thu ghi trên hóa đơn, còn doanh thu tính thuế TNCN vẫn là phần doanh
    // thu còn lại sau khi khấu trừ mức trừ lũy kế (xem `exempt_alloc`).
    let taxed_from_start = stored.is_some() && group >= 2;
    let alloc = exempt_alloc(&year_rows, thresholds.exempt, &settings.exempt_choice);
    Ok(TaxContext {
        settings,
        group,
        auto_group,
        group_from_profile: stored.is_some(),
        frozen_group: frozen,
        taxed_from_start,
        year_revenue,
        alloc,
    })
}

/// Tỷ lệ thuế của từng nhóm ngành `(mã, %GTGT, %TNCN)` — nạp 1 lần cho sổ kê
/// khai thay vì hỏi DB từng dòng.
async fn load_industry_rates(pool: &SqlitePool) -> Result<Vec<(String, f64, f64)>, String> {
    let rows = sqlx::query!("SELECT code, vat_rate, pit_rate FROM industry_group")
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|r| (r.code, r.vat_rate, r.pit_rate))
        .collect())
}

/// Tờ khai thuế theo kỳ (sheet To Khai Thue) — nhóm theo nhóm ngành nghề.
/// period: "month" | "quarter" | "year" | "per_occurrence"; period_no: 1..12 / 1..4 / 0.
/// Nhóm hộ hiện hành + doanh thu cả năm dùng để xếp nhóm: hộp thoại cấu hình
/// HKD dùng để tự điền sẵn (và cho sửa tay nếu cơ quan thuế xếp khác).
#[tauri::command]
pub(crate) async fn get_tax_group(state: State<'_, AppState>, year: i64) -> Result<String, String> {
    let pool = state.pool.read().await;
    let stored = load_stored_tax_group(&pool).await?;
    let year_rows = load_tax_agg(&pool, year, 0).await?;
    let revenue_year: f64 = year_rows
        .iter()
        .map(|r| r.revenue_up - r.revenue_down)
        .sum();
    let settings = load_tax_settings(&pool).await?;
    let th = settings.thresholds;
    let auto = tax_group_with(revenue_year, &th);
    let confirmed = stored.filter(|g| (1..=4).contains(g));
    let frozen = settings.group_by_year.get(year);
    let out = json!({
        "group": effective_tax_group(confirmed, frozen, auto),
        "auto_group": auto,
        "revenue_year": revenue_year,
        "confirmed": confirmed.is_some(),
        "frozen_group": frozen,
        // Hồ sơ ghi khác doanh thu thực tế: KHÔNG phải lỗi (năm chưa kết thúc,
        // nguyên tắc ổn định) nên UI chỉ hiện ghi chú, không cảnh báo đỏ.
        "mismatch": group_mismatch(confirmed, auto),
        "threshold_exempt": th.exempt,
        "threshold_group3": th.group3,
        "threshold_group4": th.group4,
    });
    Ok(out.to_string())
}

/// Tổng hợp thuế phải nộp theo quy định 2026 cho kỳ khai đã chọn.
///
/// Thay đổi so với bản cũ (sửa cho đúng NĐ 68/2026):
///   • chi phí = **giá vốn FIFO từ phiếu nhập** + khoản chi được trừ của phiếu chi
///     (trước đây chỉ lấy phiếu chi nên bỏ sót giá vốn — lớn nhất với hộ bán lẻ);
///   • khoản chi không đủ điều kiện trừ (Điều 6) được tách riêng và cảnh báo;
///   • doanh thu tính thuế TNCN đã trừ mức ngưỡng 01 tỷ theo phương pháp lũy kế.
#[tauri::command]
pub(crate) async fn get_tax_overview(
    state: State<'_, AppState>,
    from_date: String,
    to_date: String,
    unit_code: String,
) -> Result<String, String> {
    let pool = state.pool.read().await;
    let year = from_date
        .get(0..4)
        .and_then(|y| y.parse::<i64>().ok())
        .ok_or("Ngày bắt đầu không hợp lệ")?;
    let ctx = load_tax_context(&pool, year).await?;

    let period_rows = load_tax_agg_in_range(&pool, &from_date, &to_date, &unit_code).await?;
    let period_revenue = year_revenue(&period_rows);
    // Lũy kế từ đầu năm tới hết ngày trước khoảng đang xem — cùng cách với tờ khai
    // để mức trừ ngưỡng TNCN của kỳ khớp đúng với bảng tờ khai.
    let cum_before = load_cum_revenue_until(&pool, year, &prev_day(&from_date)).await?;
    let (from, to) = (from_date.clone(), to_date.clone());
    let period_map: std::collections::HashMap<String, f64> = period_rows
        .iter()
        .map(|r| (r.industry_code.clone(), net_revenue(r)))
        .collect();
    let split = split_by_period(&cum_before, &period_map, &ctx.alloc);
    let taxable: std::collections::HashMap<String, f64> =
        split.iter().map(|(c, v, _)| (c.clone(), *v)).collect();
    let taxable_revenue: f64 = taxable.values().sum();
    // Mức trừ ngưỡng TNCN: cả năm đã dùng bao nhiêu (không vượt doanh thu thật được)
    // và hạn ngạch còn lại — để hộ biết khi nào doanh thu bắt đầu chịu TNCN.
    let exempt_threshold = ctx.settings.thresholds.exempt;
    let exempt_used = exempt_threshold.min(ctx.year_revenue.max(0.0));
    let exempt_remaining = round2((exempt_threshold - exempt_used).max(0.0));
    let exempt_period = round2(split.iter().map(|(_, _, d)| *d).sum());

    // Chi phí: giá vốn FIFO + khoản chi được trừ (Điều 6 khoản 1).
    let cogs = load_fifo_cogs(&pool, &from, &to).await?;
    let (cost_ok, cost_no, cost_warnings) = load_period_costs(&pool, &from, &to).await?;
    let expense = round2(cogs + cost_ok);
    let profit = round2(period_revenue - expense);

    // Kỳ này đã vượt ngưỡng 01 tỷ chưa (Điều 8 khoản 1a): hộ chưa phát sinh thuế
    // trong kỳ thì cả GTGT lẫn TNCN đều bằng 0, kể cả khi hộ đã thuộc nhóm 2 vì
    // nhóm hộ được xếp theo tổng doanh thu cả năm.
    let taxable_period = period_is_taxable(
        cum_before.values().sum(),
        period_map.values().sum(),
        ctx.settings.thresholds.exempt,
        ctx.taxed_from_start,
    );

    // Thuế GTGT phải nộp: tổng tiền ghi trên hóa đơn × tỷ lệ ngành (Điều 12
    // khoản 2 Luật GTGT) — hóa đơn HKD không tách thuế nên chính doanh thu là
    // cơ sở tính. Nhóm 1 thì miễn.
    let vat_payable: f64 = if !taxable_period {
        0.0
    } else {
        period_rows
            .iter()
            .map(|r| round2(net_revenue(r) * r.vat_rate))
            .sum()
    };

    // Thuế TNCN (Điều 7 Luật TNCN + Điều 10 khoản 2 NĐ 68/2026):
    //   Nhóm 2 + phương pháp doanh thu → doanh thu TÍNH THUẾ (đã trừ ngưỡng) × tỷ lệ ngành
    //   Còn lại → thu nhập tính thuế (DT − CP được trừ) × 15%/17%/20%
    let pit_tax: f64 = if !taxable_period {
        0.0
    } else if ctx.group <= 2 && ctx.settings.method == "revenue" {
        period_rows
            .iter()
            .map(|r| {
                let t = taxable.get(&r.industry_code).copied().unwrap_or(0.0);
                round2(t * r.pit_rate)
            })
            .sum()
    } else {
        round2(profit.max(0.0) * profit_tncn_rate(ctx.group))
    };

    let profit_rate = if ctx.group <= 2 {
        if ctx.settings.method == "profit" && ctx.group >= 2 {
            0.15
        } else {
            0.0
        }
    } else if ctx.group == 3 {
        0.17
    } else if ctx.group == 4 {
        0.20
    } else {
        0.0
    };

    // Số tạm nộp theo Điều 10 khoản 2 điểm b: hộ nộp TNCN theo thu nhập tính
    // thuế thì tạm nộp theo tỷ lệ % × doanh thu của kỳ, rồi quyết toán cả năm.
    let pit_provisional: f64 =
        if !taxable_period || (ctx.group <= 2 && ctx.settings.method == "revenue") {
            pit_tax
        } else {
            period_rows
                .iter()
                .map(|r| {
                    let t = taxable.get(&r.industry_code).copied().unwrap_or(0.0);
                    round2(t * r.pit_rate)
                })
                .sum()
        };

    let overview = TaxOverview {
        year,
        year_revenue: round2(ctx.year_revenue),
        group: ctx.group,
        auto_group: ctx.auto_group,
        group_from_profile: ctx.group_from_profile,
        frozen_group: ctx.frozen_group,
        taxed_from_start: ctx.taxed_from_start,
        method: ctx.settings.method.clone(),
        period_revenue: round2(period_revenue),
        taxable_revenue: round2(taxable_revenue),
        exempt_threshold,
        exempt_used: round2(exempt_used),
        exempt_remaining,
        exempt_period,
        group3_threshold: ctx.settings.thresholds.group3,
        group4_threshold: ctx.settings.thresholds.group4,
        cogs,
        other_expense: cost_ok,
        expense,
        non_deductible: cost_no,
        cost_warnings,
        profit,
        profit_rate,
        taxable_period,
        vat_payable: round2(vat_payable),
        pit_tax: round2(pit_tax),
        pit_provisional: round2(pit_provisional),
        // Hạn nộp chỉ hiện khi khoảng đang xem trùng đúng một kỳ khai.
        deadline: period_of_range(&from, &to).and_then(|(kind, no)| tax_deadline(&kind, no, year)),
        total_tax: round2(vat_payable + pit_tax),
    };
    Ok(serde_json::to_string(&overview).unwrap_or_default())
}

/// Tạm nộp + quyết toán thu nhập cá nhân theo năm.
///
/// NĐ 68/2026 Điều 10 khoản 2:
///   • Hộ nộp theo **thuế suất × doanh thu tính thuế** → khai theo quý cùng
///     hạn kê khai GTGT (không có bước quyết toán năm riêng).
///   • Hộ nộp theo **thu nhập tính thuế × thuế suất** → **tạm nộp** theo tháng/quý
///     trên cùng hồ sơ GTGT với số tạm nộp = tỷ lệ % × doanh thu của kỳ, rồi
///     **quyết toán cả năm** (hạn 31/03 năm sau): nộp bổ sung hoặc xử lý nộp thừa.
#[tauri::command]
pub(crate) async fn get_tax_settlement(
    state: State<'_, AppState>,
    year: i64,
) -> Result<String, String> {
    let pool = state.pool.read().await;
    let ctx = load_tax_context(&pool, year).await?;
    let period_kind = if ctx.settings.period == "month" {
        "month"
    } else {
        "quarter"
    };
    let count = if period_kind == "month" { 12 } else { 4 };
    let per = months_per_period(period_kind);

    let mut periods: Vec<serde_json::Value> = Vec::new();
    let mut provisional_total = 0.0;
    for no in 1..=count {
        let (from_m, to_m) = (no * per - per + 1, no * per);
        let rows = load_tax_agg_range(&pool, year, from_m, to_m).await?;
        let cum = load_cum_revenue_before(&pool, year, from_m).await?;
        let period_map: std::collections::HashMap<String, f64> = rows
            .iter()
            .map(|r| (r.industry_code.clone(), net_revenue(r)))
            .collect();
        let taxable: std::collections::HashMap<String, f64> =
            taxable_by_period(&cum, &period_map, &ctx.alloc)
                .into_iter()
                .collect();
        // Số tạm nộp = tỷ lệ % ngành × doanh thu tính thuế của kỳ (Điều 10 khoản 2b)
        let pit: f64 = rows
            .iter()
            .map(|r| {
                let t = taxable.get(&r.industry_code).copied().unwrap_or(0.0);
                round2(t * r.pit_rate)
            })
            .sum();
        provisional_total += pit;
        periods.push(json!({
            "period": period_kind,
            "no": no,
            "label": if period_kind == "month" { format!("Tháng {no}") } else { format!("Quý {no}") },
            "revenue": round2(year_revenue(&rows)),
            "taxable_revenue": round2(taxable.values().sum::<f64>()),
            "pit_provisional": pit,
            "deadline": tax_deadline(period_kind, no, year),
        }));
    }

    // Số quyết toán cả năm (phương pháp thu nhập tính thuế).
    let (cogs, cost_ok, cost_no) = {
        let (ok, no, _) = load_period_costs(
            &pool,
            &format!("{year:04}-01-01"),
            &format!("{year:04}-12-31"),
        )
        .await?;
        (
            load_fifo_cogs(
                &pool,
                &format!("{year:04}-01-01"),
                &format!("{year:04}-12-31"),
            )
            .await?,
            ok,
            no,
        )
    };
    let revenue = ctx.year_revenue;
    let expense = round2(cogs + cost_ok);
    let income = round2(revenue - expense);
    // Hộ nộp TNCN theo tỷ lệ % trên doanh thu thì số quyết toán cả năm chính là
    // tổng số đã tạm nộp trong năm (Điều 10 NĐ 68/2026), không tính lại theo thu
    // nhập tính thuế. Trước đây luôn nhân thu nhập × thuế suất nên card quyết toán
    // in ra số quyết toán khác 0 trong khi chênh lệch ép bằng 0 — hai ô mâu thuẫn
    // với nhau.
    let by_revenue = ctx.group <= 2 && ctx.settings.method == "revenue";
    // Quyết toán cả năm chỉ có nghĩa khi hộ thuộc diện tính thuế (Điều 8 khoản 1a);
    // hộ cả năm không vượt ngưỡng thì tổng thuế bằng 0 dù hồ sơ ghi nhóm nào.
    let year_taxable = ctx.taxed_from_start || ctx.year_revenue > ctx.settings.thresholds.exempt;
    let rate = if year_taxable {
        profit_tncn_rate(ctx.group)
    } else {
        0.0
    };
    let final_tax = settlement_tax(by_revenue, provisional_total, year_taxable, income, rate);
    // Hộ vượt ngưỡng mà hồ sơ đang ghi Nhóm 1 thì không có mức thuế suất theo
    // thu nhập để áp, nên TNCN vẫn tính theo tỷ lệ % doanh thu cho tới khi hộ
    // xác nhận lại nhóm (nguyên tắc ổn định: nhóm của năm không tự đổi giữa chừng).
    let difference = if by_revenue {
        0.0
    } else {
        round2(final_tax - provisional_total)
    };
    // Kỳ tính thuế đầu tiên trong năm: hộ vượt ngưỡng giữa năm thì khai, nộp thuế
    // kể từ quý/tháng phát sinh (Điều 8 khoản 1a) — `None` = cả năm chỉ thông báo.
    let mut cum = [0.0; 13];
    for m in 1..=12usize {
        let rows_m = load_tax_agg_range(&pool, year, m as i64, m as i64).await?;
        cum[m] = cum[m - 1] + year_revenue(&rows_m);
    }
    let step = months_per_period(period_kind);
    let first = if ctx.taxed_from_start {
        // Hộ thuộc diện nộp thuế cả năm → kỳ tính thuế đầu tiên là kỳ có doanh
        // thu đầu tiên, không chờ vượt ngưỡng.
        (1..=12i64)
            .step_by(step as usize)
            .find(|m| cum[*m as usize] > cum[*m as usize - 1])
            .map(|m| (m / step, m))
    } else {
        first_taxable_period(
            &cum,
            ctx.settings.thresholds.exempt,
            months_per_period(period_kind),
        )
    };

    let out = json!({
        "year": year,
        "group": ctx.group,
        "method": ctx.settings.method,
        "by_revenue": by_revenue,
        "first_taxable_label": first.map(|(f, _)| {
            if period_kind == "month" {
                format!("Tháng {f}/{year}")
            } else {
                format!("Quý {}/{}", (f - 1) / 3 + 1, year)
            }
        }),
        "period_kind": period_kind,
        "periods": periods,
        "provisional_total": round2(provisional_total),
        "year_revenue": round2(revenue),
        "cogs": round2(cogs),
        "other_expense": round2(cost_ok),
        "non_deductible": round2(cost_no),
        "expense": expense,
        "taxable_income": income,
        "final_rate": rate,
        "final_tax": final_tax,
        "difference": difference,
        "settle_deadline": format!("31/03/{}", year + 1),
    });
    Ok(out.to_string())
} // ─── SỐ KẾ TOÁN THEO MẪU TT 152/2025/TT-BTC ───
  //
  // Thông tư 152/2025/TT-BTC (thay TT 88/2021) quy định 7 mẫu sổ bắt buộc. Mỗi
  // mẫu có bộ cột riêng, lấy đúng theo phụ lục kèm theo:
  //
  //   S1a — nhóm 1, DT ≤ ngưỡng          → Sổ doanh thu bán hàng hóa, dịch vụ
  //   S2a — nhóm 2, TNCN theo % doanh thu  → Sổ doanh thu bán hàng hóa, dịch vụ
  //   S2b — nhóm 2 TNCN theo thu nhập / nhóm 3 → Sổ doanh thu bán hàng hóa, dịch vụ
  //   S2c — thu nhập tính thuế           → Sổ chi tiết doanh thu, chi phí
  //   S2d — vật liệu, dụng cụ, hàng hóa  → Sổ chi tiết VL, DC, SP, HH
  //   S2e — tiền mặt / tiền gửi không kỳ hạn → Sổ chi tiết tiền
  //   S3a — thuế khác (TSX/TNK/TTĐB/BVMT/TNTN/TSĐT) → Sổ theo dõi nghĩa vụ thuế khác
  //
  // Vì bộ cột khác nhau theo từng mẫu, phần dưới trả về **mô tả cột** kèm theo
  // chứ không phải một struct cứng: `columns` là danh sách cột, mỗi dòng là map
  // `key → giá trị`. Frontend và xuất Excel cùng đọc bộ mô tả đó nên thêm/sửa
  // cột của một mẫu không phải sửa cả hai bên.

/// Một ô của sổ: số hoặc chữ. Ô không có dữ liệu được bỏ trống hoàn toàn
/// (không có key trong `cells`) — frontend coi key vắng là ô trống của mẫu nên
/// không in ra số 0 ở chỗ mẫu gốc để trắng.
#[derive(serde::Serialize, PartialEq, Debug)]
#[serde(untagged)]
pub(crate) enum BookCell {
    Num(f64),
    Text(String),
}

/// Kiểu hiển thị của một cột — frontend dùng để chọn định dạng.
pub(crate) const COL_TEXT: &str = "text";
pub(crate) const COL_MONEY: &str = "money";
pub(crate) const COL_QTY: &str = "qty";
pub(crate) const COL_RATE: &str = "rate";

/// Một cột của mẫu sổ: `key` là tên ô mà dòng dữ liệu dùng, `label` là tiêu đề
/// hiển thị (kèm số thứ tự cột của mẫu gốc, ví dụ `1 · Số tiền`).
#[derive(serde::Serialize)]
pub(crate) struct BookColumn {
    pub(crate) key: String,
    pub(crate) label: String,
    pub(crate) kind: &'static str,
    pub(crate) width: &'static str,
}

/// Một dòng của sổ. `kind` quyết định cách in đậm / nền:
/// `detail` (ghi chép), `subtotal` (tổng nhóm), `total` (tổng cuối sổ),
/// `section` (dòng tiêu đề nhóm, ví dụ từng loại tiền trong sổ S2e).
#[derive(serde::Serialize)]
pub(crate) struct BookRow {
    pub(crate) kind: &'static str,
    pub(crate) cells: std::collections::BTreeMap<String, BookCell>,
}

impl BookRow {
    fn new(kind: &'static str) -> Self {
        Self {
            kind,
            cells: std::collections::BTreeMap::new(),
        }
    }

    fn detail() -> Self {
        Self::new("detail")
    }

    fn subtotal() -> Self {
        Self::new("subtotal")
    }

    fn total() -> Self {
        Self::new("total")
    }

    fn section() -> Self {
        Self::new("section")
    }

    /// Số tiền — sổ kế toán Việt Nam ghi bằng đồng nguyên nên làm tròn tới số
    /// nguyên. Giữ 2 chữ số thập phân thì sổ in ra `122.019,99` trong khi tờ khai
    /// ghi `122.020`, hai số không khớp nhau khi đối chiếu với cơ quan thuế.
    fn money(&mut self, key: &str, v: f64) -> &mut Self {
        self.cells.insert(key.to_string(), BookCell::Num(v.round()));
        self
    }

    /// Số lượng — giữ nguyên phần thập phân, chỉ làm tròn 3 chữ số để tránh sai số
    /// do số thực của SQLite.
    fn qty(&mut self, key: &str, v: f64) -> &mut Self {
        let v = (v * 1000.0).round() / 1000.0;
        self.cells.insert(key.to_string(), BookCell::Num(v));
        self
    }

    fn text(&mut self, key: &str, v: impl Into<String>) -> &mut Self {
        self.cells.insert(key.to_string(), BookCell::Text(v.into()));
        self
    }
}

/// Phần đầu sổ và phần chữ ký cuối sổ — giữ đúng bố cục mẫu gốc.
///
/// Mẫu in đặt khối "Mẫu số Sxx-HKD (Kèm theo Thông tư số 152/2025/TT-BTC …)"
/// ở góc trên bên phải, còn tên địa điểm kinh doanh / kỳ kê khai nằm ngay dưới
/// tiêu đề giữa trang — không nằm cùng khối với tên hộ như trước.
#[derive(serde::Serialize, Default)]
pub(crate) struct BookHeader {
    pub(crate) owner: String,
    pub(crate) address: String,
    pub(crate) tax_code: String,
    /// Giá trị dòng dưới tiêu đề ("Địa điểm kinh doanh: …"). Mẫu nào không có
    /// dòng này thì để rỗng.
    pub(crate) location: String,
    /// Nhãn của dòng đó — mỗi mẫu một tên ("Tên địa điểm kinh doanh" ở S2c,
    /// "Tên vật liệu, dụng cụ…" ở S2d, bỏ hẳn ở S2e). Rỗng = không hiện dòng.
    pub(crate) location_label: String,
    pub(crate) period: String,
    /// Khối "Mẫu số …" góc trên bên phải đầu sổ.
    pub(crate) form_ref: String,
    pub(crate) unit: String,
    pub(crate) sign_date: String,
    pub(crate) signer: String,
}

/// Ghi chú dưới sổ. `level` = `info` | `warn`; nếu có `action_book` thì hiện
/// nút chuyển sang mẫu sổ khác (dùng cho cảnh báo chọn sai mẫu).
#[derive(serde::Serialize)]
pub(crate) struct BookNote {
    pub(crate) level: &'static str,
    pub(crate) text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) action_book: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) action_label: Option<String>,
}

/// Nội dung 1 mẫu sổ: tiêu đề + đầu sổ + bộ cột + dòng chi tiết + tổng + ghi chú.
#[derive(serde::Serialize)]
pub(crate) struct TaxBook {
    pub(crate) book: String,
    pub(crate) title: String,
    pub(crate) period_label: String,
    pub(crate) header: BookHeader,
    pub(crate) columns: Vec<BookColumn>,
    pub(crate) rows: Vec<BookRow>,
    pub(crate) notes: Vec<BookNote>,
}

/// Bộ cột dùng chung cho S2a/S2b — "Sổ doanh thu bán hàng hóa, dịch vụ".
fn revenue_columns() -> Vec<BookColumn> {
    vec![
        BookColumn {
            key: "a".into(),
            label: "A · Số hiệu".into(),
            kind: COL_TEXT,
            width: "7rem",
        },
        BookColumn {
            key: "b".into(),
            label: "B · Ngày, tháng".into(),
            kind: COL_TEXT,
            width: "7rem",
        },
        BookColumn {
            key: "c".into(),
            label: "C · Diễn giải".into(),
            kind: COL_TEXT,
            width: "24rem",
        },
        BookColumn {
            key: "1".into(),
            label: "1 · Số tiền".into(),
            kind: COL_MONEY,
            width: "10rem",
        },
    ]
}

/// Bộ cột của S1a — mẫu gốc chỉ có 3 cột, **không có số hiệu chứng từ**.
///
/// Số hiệu chứng từ vẫn cần để đối chiếu với phiếu, nên ghi vào đầu cột
/// "Diễn giải" thay vì thêm một cột không có trong mẫu.
fn s1a_columns() -> Vec<BookColumn> {
    vec![
        BookColumn {
            key: "a".into(),
            label: "A · Ngày tháng".into(),
            kind: COL_TEXT,
            width: "8rem",
        },
        BookColumn {
            key: "b".into(),
            label: "B · Diễn giải".into(),
            kind: COL_TEXT,
            width: "30rem",
        },
        BookColumn {
            key: "1".into(),
            label: "1 · Số tiền".into(),
            kind: COL_MONEY,
            width: "11rem",
        },
    ]
}

/// Bộ cột của S2c — "Sổ chi tiết doanh thu, chi phí".
fn s2c_columns() -> Vec<BookColumn> {
    vec![
        BookColumn {
            key: "a".into(),
            label: "A · Số hiệu".into(),
            kind: COL_TEXT,
            width: "7rem",
        },
        BookColumn {
            key: "b".into(),
            label: "B · Ngày, tháng".into(),
            kind: COL_TEXT,
            width: "7rem",
        },
        BookColumn {
            key: "c".into(),
            label: "C · Diễn giải".into(),
            kind: COL_TEXT,
            width: "30rem",
        },
        BookColumn {
            key: "1".into(),
            label: "1 · Số tiền".into(),
            kind: COL_MONEY,
            width: "11rem",
        },
    ]
}

/// Bộ cột của S2d — "Sổ chi tiết vật liệu, dụng cụ, sản phẩm, hàng hóa".
///
/// Mẫu gốc mở **một quyển sổ cho từng loại vật liệu**, tên loại vật liệu nằm ở
/// đầu sổ. App hiển thị một bảng gộp tất cả mã hàng nên thêm cột `Mã · Tên`
/// ở trước cột A — cột bổ sung, các cột A/1..8 bên sau giữ nguyên mẫu.
fn s2d_columns() -> Vec<BookColumn> {
    vec![
        BookColumn {
            key: "item".into(),
            label: "Mã · Tên vật liệu".into(),
            kind: COL_TEXT,
            width: "13rem",
        },
        BookColumn {
            key: "a".into(),
            label: "A · Số hiệu".into(),
            kind: COL_TEXT,
            width: "6rem",
        },
        BookColumn {
            key: "b".into(),
            label: "B · Ngày, tháng".into(),
            kind: COL_TEXT,
            width: "6.5rem",
        },
        BookColumn {
            key: "c".into(),
            label: "C · Diễn giải".into(),
            kind: COL_TEXT,
            width: "16rem",
        },
        BookColumn {
            key: "d".into(),
            label: "D · Đơn vị tính".into(),
            kind: COL_TEXT,
            width: "6rem",
        },
        BookColumn {
            key: "1".into(),
            label: "1 · Đơn giá".into(),
            kind: COL_MONEY,
            width: "8rem",
        },
        BookColumn {
            key: "2".into(),
            label: "2 · SL nhập".into(),
            kind: COL_QTY,
            width: "7rem",
        },
        BookColumn {
            key: "3".into(),
            label: "3 · Thành tiền nhập".into(),
            kind: COL_MONEY,
            width: "9rem",
        },
        BookColumn {
            key: "4".into(),
            label: "4 · SL xuất".into(),
            kind: COL_QTY,
            width: "7rem",
        },
        BookColumn {
            key: "5".into(),
            label: "5 · Thành tiền xuất".into(),
            kind: COL_MONEY,
            width: "9rem",
        },
        BookColumn {
            key: "6".into(),
            label: "6 · SL tồn".into(),
            kind: COL_QTY,
            width: "7rem",
        },
        BookColumn {
            key: "7".into(),
            label: "7 · Thành tiền tồn".into(),
            kind: COL_MONEY,
            width: "9rem",
        },
        BookColumn {
            key: "8".into(),
            label: "8 · Ghi chú".into(),
            kind: COL_TEXT,
            width: "10rem",
        },
    ]
}

/// Bộ cột của S2e — "Sổ chi tiết tiền". Cột 1 là Thu/Gửi vào, cột 2 là
/// Chi/Rút ra; mẫu chia thành khối Tiền mặt và khối Tiền gửi không kỳ hạn
/// (riêng từng ngân hàng), mỗi khối có số dư đầu kỳ và tồn cuối kỳ.
fn s2e_columns() -> Vec<BookColumn> {
    vec![
        BookColumn {
            key: "a".into(),
            label: "A · Số hiệu".into(),
            kind: COL_TEXT,
            width: "7rem",
        },
        BookColumn {
            key: "b".into(),
            label: "B · Ngày, tháng".into(),
            kind: COL_TEXT,
            width: "7rem",
        },
        BookColumn {
            key: "c".into(),
            label: "C · Diễn giải".into(),
            kind: COL_TEXT,
            width: "26rem",
        },
        BookColumn {
            key: "1".into(),
            label: "1 · Thu / Gửi vào".into(),
            kind: COL_MONEY,
            width: "11rem",
        },
        BookColumn {
            key: "2".into(),
            label: "2 · Chi / Rút ra".into(),
            kind: COL_MONEY,
            width: "11rem",
        },
    ]
}

/// Bộ cột của S3a — "Sổ theo dõi nghĩa vụ thuế khác".
///
/// Theo phụ lục TT 152: cột A ngày tháng, B diễn giải, 1 lượng hàng hóa/dịch vụ
/// chịu thuế, 2 mức thuế tuyệt đối, 3 giá tính thuế/đơn vị, 4 thuế suất,
/// 5 thuế TSX/TNK/TTĐB theo tỷ lệ %, 6 theo phương pháp tuyệt đối,
/// 7 tổng thuế phải nộp, 8 thuế bảo vệ môi trường, 9 thuế tài nguyên,
/// 10 thuế sử dụng đất.
fn s3a_columns() -> Vec<BookColumn> {
    vec![
        BookColumn {
            key: "a".into(),
            label: "A · Ngày tháng ghi sổ".into(),
            kind: COL_TEXT,
            width: "7rem",
        },
        BookColumn {
            key: "b".into(),
            label: "B · Diễn giải".into(),
            kind: COL_TEXT,
            width: "18rem",
        },
        BookColumn {
            key: "1".into(),
            label: "1 · Lượng HH, DV chịu thuế".into(),
            kind: COL_QTY,
            width: "8rem",
        },
        BookColumn {
            key: "2".into(),
            label: "2 · Mức thuế tuyệt đối".into(),
            kind: COL_MONEY,
            width: "8rem",
        },
        BookColumn {
            key: "3".into(),
            label: "3 · Giá tính thuế / 01 đơn vị".into(),
            kind: COL_MONEY,
            width: "9rem",
        },
        BookColumn {
            key: "4".into(),
            label: "4 · Thuế suất".into(),
            kind: COL_RATE,
            width: "6rem",
        },
        BookColumn {
            key: "5".into(),
            label: "5 · TSX, TNK, TTĐB theo tỷ lệ %".into(),
            kind: COL_MONEY,
            width: "9rem",
        },
        BookColumn {
            key: "6".into(),
            label: "6 · TSX, TNK, TTĐB tuyệt đối".into(),
            kind: COL_MONEY,
            width: "9rem",
        },
        BookColumn {
            key: "7".into(),
            label: "7 · TSX, TNK, TTĐB phải nộp".into(),
            kind: COL_MONEY,
            width: "9rem",
        },
        BookColumn {
            key: "8".into(),
            label: "8 · Thuế bảo vệ môi trường".into(),
            kind: COL_MONEY,
            width: "9rem",
        },
        BookColumn {
            key: "9".into(),
            label: "9 · Thuế tài nguyên".into(),
            kind: COL_MONEY,
            width: "8rem",
        },
        BookColumn {
            key: "10".into(),
            label: "10 · Thuế sử dụng đất".into(),
            kind: COL_MONEY,
            width: "8rem",
        },
    ]
}

/// Bảng tên ngành nghề: mã → tên, để ghi đúng cột "Diễn giải".
async fn load_industry_names(
    pool: &SqlitePool,
) -> Result<std::collections::HashMap<String, String>, String> {
    let rows = sqlx::query!(r#"SELECT code AS "code!", name AS "name!" FROM industry_group"#)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(rows.into_iter().map(|r| (r.code, r.name)).collect())
}

/// Nhãn ngành nghề ghi vào cột Diễn giải: `Phân phối, cung cấp hàng hóa (PPHH)`.
///
/// Luôn kèm mã — mẫu sổ mở riêng cho từng nhóm ngành nên người đối chiếu với
/// tờ khai cần thấy đúng mã đang dùng, không chỉ tên.
fn industry_label(names: &std::collections::HashMap<String, String>, code: &str) -> String {
    match names.get(code) {
        Some(n) if !n.is_empty() && n != code => format!("{n} ({code})"),
        _ => code.to_string(),
    }
}

#[derive(serde::Serialize)]
struct OwnerInfo {
    name: String,
    address: String,
    tax_code: String,
    unit: String,
    location: String,
}

async fn load_owner(pool: &SqlitePool) -> Result<OwnerInfo, String> {
    let b = sqlx::query!(
        r#"SELECT b.name AS "name!", b.address AS "address!", b.tax_code AS "tax_code!",
                  b.location AS "location!",
                  (SELECT name FROM business_unit ORDER BY id LIMIT 1) AS "unit!"
             FROM business b LIMIT 1"#
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(match b {
        // Địa điểm kinh doanh trống → lấy địa chỉ đăng ký, đầu sổ không bao giờ
        // in ra một dòng trống phía sau dấu hai chấm.
        Some(r) => OwnerInfo {
            location: if r.location.trim().is_empty() {
                r.address.clone()
            } else {
                r.location
            },
            name: r.name,
            address: r.address,
            tax_code: r.tax_code,
            unit: r.unit,
        },
        None => OwnerInfo {
            name: String::new(),
            address: String::new(),
            tax_code: String::new(),
            unit: String::new(),
            location: String::new(),
        },
    })
}

/// Ngày ghi sổ dạng `dd/mm/yyyy` — mẫu sổ in tay viết ngày tháng kiểu Việt Nam,
/// để nguyên `2026-07-05` thì cột "Ngày tháng" in ra khác hẳn dòng chữ ký ngày
/// ở cuối sổ (`today_vn`). Chuỗi không đúng dạng ISO thì giữ nguyên, không bịa ngày.
fn vn_date(iso: &str) -> String {
    match iso.split_once('-') {
        Some((y, rest)) if y.len() == 4 && rest.len() == 5 => match rest.split_once('-') {
            Some((m, d)) => format!("{d}/{m}/{y}"),
            None => iso.to_string(),
        },
        _ => iso.to_string(),
    }
}

/// Ngày ký cuối sổ: hôm nay, định dạng `dd/mm/yyyy`.
fn today_vn() -> String {
    chrono::Local::now().format("%d/%m/%Y").to_string()
}

/// Lấy 1 mẫu sổ TT 152/2025 cho kỳ đang chọn.
///
/// `book`: S1a | S2a | S2b | S2c | S2d | S2e | S3a
#[tauri::command]
pub(crate) async fn get_tax_books(
    state: State<'_, AppState>,
    year: i64,
    period: String,
    period_no: i64,
    book: String,
) -> Result<String, String> {
    let pool = state.pool.read().await;
    build_tax_book(&pool, year, &period, period_no, &book).await
}

/// Lấy danh sách mẫu sổ **áp dụng** cho hồ sơ đang mở (Điều 4 TT 152/2025).
///
/// Màn Kế toán dùng danh sách này để chỉ hiện sổ đúng nhóm hộ — chọn nhầm mẫu
/// không thuộc nhóm thì sổ in ra không nộp được. Trả về JSON `["S1a", "S3a"]`.
#[tauri::command]
pub(crate) async fn get_applicable_books(
    state: State<'_, AppState>,
    year: i64,
) -> Result<String, String> {
    let pool = state.pool.read().await;
    let ctx = load_tax_context(&pool, year).await?;
    let books = applicable_books(ctx.group, &ctx.settings.method);
    Ok(serde_json::to_string(&books).unwrap_or_default())
}

/// Mẫu sổ TT 152/2025 áp dụng cho một hồ sơ — Điều 4 phân theo **nhóm hộ** và
/// **phương pháp tính thuế TNCN**:
///
/// * Nhóm 1 (doanh thu ≤ ngưỡng, miễn thuế) → chỉ sổ doanh thu `S1a`;
/// * Nhóm 2 nộp TNCN theo tỷ lệ % doanh thu → `S2a`;
/// * Còn lại (TNCN theo thu nhập — gồm cả Nhóm 3/4 luôn tính theo lợi nhuận)
///   → `S2b` + `S2c` + `S2d` + `S2e`;
/// * `S3a` (thuế khác) luôn mở: app chưa theo dõi các loại thuế này nên không
///   ẩn đi được, để đó khi hộ phát sinh thuế bảo vệ môi trường, thuế sử dụng đất…
///
/// Điều kiện "theo thu nhập" phải trùng với `by_revenue` của tờ khai
/// (`ctx.group <= 2 && method == "revenue"`) — hai chỗ mà lệch thì sổ in ra và
/// tờ khai khai hai kiểu thuế TNCN khác nhau.
pub(crate) fn applicable_books(group: i64, method: &str) -> Vec<&'static str> {
    let mut books: Vec<&'static str> = if group <= 1 {
        vec!["S1a"]
    } else if group <= 2 && method == "revenue" {
        vec!["S2a"]
    } else {
        vec!["S2b", "S2c", "S2d", "S2e"]
    };
    books.push("S3a");
    books
}

/// Nội dung một mẫu sổ — tách khỏi lệnh Tauri để test gọi được với pool in-memory.
async fn build_tax_book(
    pool: &SqlitePool,
    year: i64,
    period: &str,
    period_no: i64,
    book: &str,
) -> Result<String, String> {
    let ctx = load_tax_context(pool, year).await?;
    let (from_m, to_m) = period_range(period, period_no)?;
    let from = format!("{year:04}-{from_m:02}-01");
    let to = format!("{year:04}-{to_m:02}-31");
    let period_label = describe_tax_period(period, period_no, year);
    let owner = load_owner(pool).await?;
    let names = load_industry_names(pool).await?;

    let mut notes: Vec<BookNote> = Vec::new();
    let (title, columns, rows) = match book {
        // ── S1a: hộ nhóm 1 — sổ doanh thu bán hàng hóa, dịch vụ ────────────
        "S1a" => {
            let sales = sales_rows(pool, &from, &to).await?;
            let mut rows = Vec::new();
            let mut total = 0.0;
            for s in &sales {
                let amount = signed_amount(s.adjust_code.as_str(), s.amount);
                total += amount;
                let mut r = BookRow::detail();
                r.text("a", vn_date(&s.posting_date));
                r.text("b", format!("{} — {}", s.voucher_no, s.description));
                r.money("1", amount);
                rows.push(r);
            }
            let mut t = BookRow::total();
            t.text("b", "Tổng cộng");
            t.money("1", total);
            rows.push(t);
            (
                "SỔ DOANH THU BÁN HÀNG HÓA, DỊCH VỤ".to_string(),
                s1a_columns(),
                rows,
            )
        }
        // ── S2a/S2b: sổ doanh thu theo nhóm ngành có cùng tỷ lệ % ─────────
        "S2a" | "S2b" => {
            let is_s2a = book == "S2a";
            let sales = sales_rows(pool, &from, &to).await?;
            let rates = load_industry_rates(pool).await?;
            // Nhóm 1 không chịu thuế: sổ chỉ ghi doanh thu, không ghi số thuế.
            let exempt = ctx.group == 1;
            // TNCN theo tỷ lệ % doanh thu chỉ thuộc S2a. S2b là mẫu của hộ tính
            // TNCN trên thu nhập — thuế đó nằm ở S2c, ghi vào S2b là sai.
            let pit_by_revenue =
                is_s2a && !exempt && ctx.group == 2 && ctx.settings.method == "revenue";
            if is_s2a && !exempt && ctx.settings.method != "revenue" {
                notes.push(BookNote {
                    level: "warn",
                    text: "S2a-HKD là mẫu dành cho hộ nhóm 2 nộp thuế TNCN theo tỷ lệ % trên doanh thu. Hồ sơ đang đặt phương pháp TNCN theo thu nhập — khi đó phải dùng S2b kèm S2c.".into(),
                    action_book: Some("S2b".to_string()),
                    action_label: Some("Chuyển sang S2b-HKD".to_string()),
                });
            }

            let mut rows = Vec::new();
            let mut total_revenue = 0.0;
            let mut total_vat = 0.0;
            let mut total_pit = 0.0;
            // Mẫu gốc chia sổ theo từng nhóm ngành: mỗi khối là dòng
            // "n. Ngành nghề …" → dòng chứng từ của khối → "Tổng cộng (n)" →
            // "Thuế GTGT" → "Thuế TNCN". Các dòng thuế phải nằm ngay sau tổng
            // cộng của đúng nhóm đó, không gom hết thuế xuống cuối sổ.
            let mut order: Vec<String> = Vec::new();
            let mut groups: std::collections::HashMap<String, (f64, f64, f64)> =
                std::collections::HashMap::new();
            for s in &sales {
                let amount = signed_amount(s.adjust_code.as_str(), s.amount);
                let industry = s.industry_code.clone();
                if !groups.contains_key(&industry) {
                    order.push(industry.clone());
                }
                let entry = groups.entry(industry.clone()).or_insert((0.0, 0.0, 0.0));
                entry.0 += amount;
                let (rate_vat, rate_pit) = rates
                    .iter()
                    .find(|(c, _, _)| c == &industry)
                    .map(|(_, v, p)| (*v, *p))
                    .unwrap_or((0.0, 0.0));
                entry.1 += round2(amount * rate_vat);
                entry.2 += round2(amount * rate_pit);
                total_revenue += amount;
                total_vat += round2(amount * rate_vat);
                total_pit += round2(amount * rate_pit);
            }
            for (i, industry) in order.iter().enumerate() {
                let (sum, vat, pit) = groups.get(industry).copied().unwrap_or((0.0, 0.0, 0.0));
                let mut head = BookRow::section();
                head.text(
                    "c",
                    format!("{}. Ngành nghề {}", i + 1, industry_label(&names, industry)),
                );
                rows.push(head);
                // Chứng từ của đúng nhóm ngành này, vẫn giữ thứ tự ngày như mẫu.
                for s in &sales {
                    if &s.industry_code != industry {
                        continue;
                    }
                    let amount = signed_amount(s.adjust_code.as_str(), s.amount);
                    let mut r = BookRow::detail();
                    r.text("a", &s.voucher_no);
                    r.text("b", vn_date(&s.posting_date));
                    r.text("c", &s.description);
                    r.money("1", amount);
                    rows.push(r);
                }
                let mut r = BookRow::subtotal();
                r.text("c", format!("Tổng cộng ({})", i + 1));
                r.money("1", sum);
                rows.push(r);
                // Hộ nhóm 1 không phát sinh thuế nên số thuế từng nhóm ghi 0.
                let mut r = BookRow::subtotal();
                r.text("c", "Thuế GTGT");
                r.money("1", if exempt { 0.0 } else { vat });
                rows.push(r);
                if pit_by_revenue {
                    let mut r = BookRow::subtotal();
                    r.text("c", "Thuế TNCN");
                    r.money("1", pit);
                    rows.push(r);
                }
            }
            // Hai dòng tổng cuối sổ theo mẫu gốc.
            let mut t = BookRow::total();
            t.text("c", "Tổng số thuế GTGT phải nộp");
            t.money("1", if exempt { 0.0 } else { total_vat });
            rows.push(t);
            if pit_by_revenue {
                let mut t = BookRow::total();
                t.text("c", "Tổng số thuế TNCN phải nộp");
                t.money("1", total_pit);
                rows.push(t);
            }
            if exempt {
                // S2a không có cột riêng cho ghi chú, nên lời giải thích đưa vào
                // ghi chú dưới sổ thay vì ghi vào cột số không tồn tại.
                notes.push(BookNote {
                    level: "info",
                    text: "Hộ thuộc nhóm 1 chỉ thông báo doanh thu, không phát sinh thuế."
                        .to_string(),
                    action_book: None,
                    action_label: None,
                });
            }
            notes.push(BookNote {
                level: "info",
                text: format!(
                    "Nhóm {} · doanh thu {} đ · tổng thuế GTGT {} đ. {}",
                    ctx.group,
                    fmt_thousands(total_revenue),
                    fmt_thousands(total_vat),
                    if pit_by_revenue {
                        format!(
                            "Mẫu S2a: thuế TNCN tính theo tỷ lệ % trên doanh thu là {} đ.",
                            fmt_thousands(total_pit)
                        )
                    } else if exempt {
                        "Hộ nhóm 1 không phát sinh thuế.".to_string()
                    } else {
                        "Thuế TNCN theo thu nhập tính thuế ghi ở sổ S2c, không ghi vào sổ này."
                            .to_string()
                    }
                ),
                action_book: None,
                action_label: None,
            });
            (
                "SỔ DOANH THU BÁN HÀNG HÓA, DỊCH VỤ".to_string(),
                revenue_columns(),
                rows,
            )
        }
        // ── S2c: sổ chi tiết doanh thu, chi phí ─────────────────────────────
        "S2c" => {
            let period_rows = load_tax_agg_range(pool, year, from_m, to_m).await?;
            let cogs = load_fifo_cogs(pool, &from, &to).await?;
            let (cost_ok, _cost_no, warnings) = load_period_costs(pool, &from, &to).await?;
            let mut rows = Vec::new();
            let mut revenue = 0.0;
            for r in &period_rows {
                let net = round2(net_revenue(r));
                if net == 0.0 {
                    continue;
                }
                revenue += net;
            }

            // (1) Mẫu gốc chỉ có một dòng tổng doanh thu; chi tiết theo từng
            // ngành nghề đã nằm ở sổ S2a/S2b nên không lặp lại ở đây.
            let mut t = BookRow::subtotal();
            t.text("c", "1. Doanh thu bán hàng hóa, dịch vụ");
            t.money("1", revenue);
            rows.push(t);

            // (2) Chi phí hợp lý: tổng của (2) trước, sáu dòng a)–e) bên dưới
            // đúng thứ tự và đúng từng chữ như mẫu in.
            let total_cost = round2(cogs + cost_ok);
            let mut t = BookRow::subtotal();
            t.text("c", "2. Chi phí hợp lý");
            t.money("1", total_cost);
            rows.push(t);
            for (label, value) in s2c_cost_lines(cogs, cost_ok) {
                let mut r = BookRow::detail();
                r.text("c", label);
                // Không có số thì để trống — đừng ghi "0" vào ô mẫu gốc bỏ trắng.
                if value.abs() > 0.5 {
                    r.money("1", value);
                }
                rows.push(r);
            }

            let diff = round2(revenue - total_cost);
            let mut t = BookRow::total();
            t.text("c", "3. Chênh lệch {(3) = (1) − (2)}");
            t.money("1", diff);
            rows.push(t);
            // (4) Thuế TNCN theo thu nhập tính thuế — dùng cùng cách tính với
            // tờ khai để sổ và tờ khớp nhau.
            let pit = pit_on_income(pool, year, diff).await?;
            let mut t = BookRow::total();
            t.text("c", "4. Tổng số thuế TNCN phải nộp {(4) = (3) × thuế suất}");
            t.money("1", pit.0);
            rows.push(t);
            notes.push(BookNote {
                level: "info",
                text: format!(
                    "Thuế suất TNCN áp dụng {} (phương pháp thu nhập tính thuế).",
                    fmt_pct(pit.1)
                ),
                action_book: None,
                action_label: None,
            });
            if !warnings.is_empty() {
                let bad: f64 = warnings.iter().map(|w| round2(w.amount)).sum();
                notes.push(BookNote {
                    level: "warn",
                    text: format!(
                        "{} khoản chi không được trừ tổng {} đ (Điều 6 NĐ 68/2026): {}. Mẫu gốc không có dòng riêng nên sổ chỉ ghi các khoản được trừ vào (2).",
                        warnings.len(),
                        fmt_thousands(bad),
                        warnings
                            .iter()
                            .map(|w| w.voucher_no.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    action_book: None,
                    action_label: None,
                });
            }
            notes.push(BookNote {
                level: "info",
                text: "Chứng từ chưa gán mã nhóm chi phí nên chưa tách được từng dòng a)–e): giá vốn FIFO ghi vào dòng a), chi phí khác có đủ chứng từ ghi vào dòng e), dòng b)–d) để trống.".into(),
                action_book: None,
                action_label: None,
            });
            (
                "SỔ CHI TIẾT DOANH THU, CHI PHÍ".to_string(),
                s2c_columns(),
                rows,
            )
        }
        // ── S2d: sổ chi tiết vật liệu, dụng cụ, sản phẩm, hàng hóa ──────────
        "S2d" => {
            let (rows, note) = stock_ledger_rows(pool, &from, &to).await?;
            if let Some(n) = note {
                notes.push(n);
            }
            (
                "SỔ CHI TIẾT VẬT LIỆU, DỤNG CỤ, SẢN PHẨM, HÀNG HÓA".to_string(),
                s2d_columns(),
                rows,
            )
        }
        // ── S2e: sổ chi tiết tiền ───────────────────────────────────────────
        "S2e" => {
            let rows = cash_ledger_rows(pool, &from, &to).await?;
            notes.push(BookNote {
                level: "info",
                text: "Sổ chia thành khối Tiền mặt và khối Tiền gửi không kỳ hạn; mỗi khối có dòng Ngân hàng (với tiền gửi), Tiền mặt/Tiền gửi đầu kỳ, Tổng tiền thu (gửi) vào, Tổng tiền chi (rút) ra và Tiền mặt tồn / Tiền gửi cuối kỳ. Số đầu kỳ lấy từ số dư tài khoản tiền tại màn Tài khoản.".into(),
                action_book: None,
                action_label: None,
            });
            ("SỔ CHI TIẾT TIỀN".to_string(), s2e_columns(), rows)
        }
        // ── S3a: sổ theo dõi nghĩa vụ thuế khác ─────────────────────────────
        "S3a" => {
            let rows = other_tax_ledger_rows();
            notes.push(BookNote {
                level: "info",
                text: "Sổ ghi các loại thuế ngoài GTGT và TNCN: thuế xuất khẩu, nhập khẩu, tiêu thụ đặc biệt, thuế bảo vệ môi trường, thuế tài nguyên và thuế sử dụng đất. Hộ chỉ phát sinh GTGT và TNCN thì sổ này để trống toàn bộ cột số liệu.".into(),
                action_book: None,
                action_label: None,
            });
            (
                "SỔ THEO DÕI NGHĨA VỤ THUẾ KHÁC".to_string(),
                s3a_columns(),
                rows,
            )
        }
        other => return Err(format!("Mẫu sổ '{other}' không hợp lệ")),
    };

    // Ô ghi vào mà không có cột tương ứng thì in ra sẽ bị mất trắng (đã từng
    // xảy ra với ô "8" của mẫu S2d). Bắt ngay lúc chạy test thay vì để sổ in
    // ra thiếu số mà không ai biết.
    debug_assert!(
        rows.iter()
            .all(|r| r.cells.keys().all(|k| columns.iter().any(|c| &c.key == k))),
        "dòng sổ có ô không thuộc bộ cột của mẫu {book}"
    );

    // Khối "Mẫu số …" ở góc trên bên phải — mỗi mẫu một số, đúng như mẫu in
    // kèm theo Thông tư 152/2025/TT-BTC.
    let form_ref = format!(
        "Mẫu số {book}-HKD (Kèm theo Thông tư số 152/2025/TT-BTC ngày 31 tháng 12 năm 2025 \
         của Bộ trưởng Bộ Tài chính)"
    );
    // Dòng dưới tiêu đề: mỗi mẫu một nhãn khác nhau; S2e không có dòng này.
    let (location_label, location) = match book {
        "S2c" => ("Tên địa điểm kinh doanh", owner.location.clone()),
        "S2d" => ("Tên vật liệu, dụng cụ, sản phẩm, hàng hóa", String::new()),
        "S2e" => ("", String::new()),
        _ => ("Địa điểm kinh doanh", owner.location.clone()),
    };
    let header = BookHeader {
        owner: owner.name,
        address: owner.address,
        tax_code: owner.tax_code,
        location,
        location_label: location_label.to_string(),
        period: period_label.clone(),
        form_ref,
        unit: if book == "S2d" {
            // Mẫu S2d không có dòng "Đơn vị tính:" ở đầu sổ — đơn vị ghi ngay
            // trên từng dòng ở cột D, thêm dòng này là in sai mẫu.
            String::new()
        } else {
            "VNĐ".to_string()
        },
        sign_date: today_vn(),
        signer: "NGƯỜI ĐẠI DIỆN HỘ KINH DOANH / CÁ NHÂN KINH DOANH".to_string(),
    };
    let out = TaxBook {
        book: book.to_string(),
        title,
        period_label,
        header,
        columns,
        rows,
        notes,
    };
    Ok(serde_json::to_string(&out).unwrap_or_default())
}

/// Dòng doanh thu (PX) trong khoảng ngày — nguồn chung cho S1a/S2a/S2b.
///
/// Phiếu ghi giá 0 không phát sinh doanh thu nên không đưa vào sổ: để nguyên
/// thì S2a mở ra một khối ngành trống toàn dòng 0 đồng (khối "Cho thuê tài
/// sản…" từng hiện chỉ vì một phiếu giá 0, trong khi không bán gì cả).
struct SalesRow {
    voucher_no: String,
    posting_date: String,
    description: String,
    industry_code: String,
    adjust_code: String,
    amount: f64,
}

async fn sales_rows(pool: &SqlitePool, from: &str, to: &str) -> Result<Vec<SalesRow>, String> {
    let rows = sqlx::query!(
        r#"SELECT je.voucher_no AS "voucher_no!", je.posting_date AS "posting_date!",
                  je.description AS "description!", je.industry_code AS "industry_code!",
                  je.adjust_code AS "adjust_code!", je.amount
             FROM journal_entry je
            WHERE je.entry_type = 'PX'
              AND je.posting_date >= ? AND je.posting_date <= ?
              AND je.amount <> 0
              AND COALESCE(je.adjust_code, '') != 'XuatNVL'
            ORDER BY je.posting_date, je.id"#,
        from,
        to
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|r| SalesRow {
            voucher_no: r.voucher_no,
            posting_date: r.posting_date,
            description: r.description,
            industry_code: r.industry_code,
            adjust_code: r.adjust_code,
            amount: r.amount,
        })
        .collect())
}

/// `GiamDT` (giảm doanh thu) làm số tiền âm; mọi trường hợp khác giữ dấu dương.
fn signed_amount(adjust_code: &str, amount: f64) -> f64 {
    if adjust_code == "GiamDT" {
        -amount
    } else {
        amount
    }
}

/// Sáu dòng chi phí a)–e) của mẫu S2c, chép đúng văn bản in kèm Thông tư
/// 152/2025/TT-BTC (mẫu gốc đánh dấu hai lần chữ "d)" — giữ nguyên để sổ in ra
/// khớp từng chữ với mẫu nộp cơ quan thuế).
///
/// App chưa gán mã nhóm chi phí cho từng chứng từ nên chỉ hai dòng có số:
/// giá vốn FIFO vào dòng a), phần chi phí khác đã có chứng từ vào dòng e).
/// Các dòng còn lại trả `0.0` và được để trống khi in.
fn s2c_cost_lines(cogs: f64, other: f64) -> [(&'static str, f64); 6] {
    [
        (
            "a) Chi phí nguyên vật liệu, vật liệu, nhiên liệu, năng lượng, hàng hóa sử dụng \
             vào sản xuất, kinh doanh",
            cogs,
        ),
        (
            "b) Chi phí tiền lương, tiền công, các khoản phụ cấp, bảo hiểm bắt buộc và các \
             khoản chi trả cho người lao động có đóng bảo hiểm bắt buộc theo quy định; chi phí \
             tiền lương, tiền công, các khoản phụ cấp và các khoản chi trả cho người lao động \
             làm việc dưới 01 tháng.",
            0.0,
        ),
        (
            "c) Chi phí khấu hao tài sản cố định phục vụ cho hoạt động sản xuất, kinh doanh theo \
             chế độ quản lý, sử dụng và trích khấu hao tài sản cố định (nếu có).",
            0.0,
        ),
        (
            "d) Chi phí dịch vụ mua ngoài như điện, nước, điện thoại, internet, vận chuyển, thuê \
             tài sản, sửa chữa, bảo dưỡng.",
            0.0,
        ),
        (
            "d) Chi phí trả lãi tiền vay vốn sản xuất, kinh doanh của tổ chức tín dụng theo lãi \
             suất thực tế. Chi phí trả lãi tiền vay vốn sản xuất, kinh doanh của hộ gia đình \
             không phải là tổ chức tín dụng không vượt quá mức quy định tại Bộ luật Dân sự.",
            0.0,
        ),
        (
            "e) Các khoản chi khác phục vụ trực tiếp hoạt động sản xuất, kinh doanh…",
            other,
        ),
    ]
}

/// Thuế TNCN theo thu nhập tính thuế cho một khoảng lời lãi.
///
/// Công thức phải giống hệt `build_tax_declaration` (thu nhập tính thuế × thuế
/// suất theo nhóm) để sổ S2c và tờ khai không lệch nhau. Phương pháp thu nhập
/// không áp mức trừ 1 tỷ — mức trừ lũy kế đó thuộc phương pháp tính TNCN theo
/// tỷ lệ % trên doanh thu (`taxable_by_period`), nên chỉ cần trả về thuế và
/// thuế suất cho phần ghi chú dưới sổ.
async fn pit_on_income(pool: &SqlitePool, year: i64, income: f64) -> Result<(f64, f64), String> {
    let ctx = load_tax_context(pool, year).await?;
    // Quyết toán chỉ có nghĩa khi hộ thuộc diện tính thuế (Điều 8 khoản 1a).
    let taxable = ctx.taxed_from_start || ctx.year_revenue > ctx.settings.thresholds.exempt;
    let rate = if taxable {
        profit_tncn_rate(ctx.group)
    } else {
        0.0
    };
    Ok((round2(income.max(0.0) * rate), rate))
}

/// Thuế suất dạng `0,5%` / `1%` đúng kiểu Việt Nam.
fn fmt_pct(rate: f64) -> String {
    let p = rate * 100.0;
    let s = (p * 100.0).round() / 100.0;
    let txt = format!("{s:.2}");
    let txt = txt.trim_end_matches('0').trim_end_matches('.').to_string();
    format!("{txt}%")
}
/// Dòng sổ S2d — nhập / xuất / tồn từng mã vật liệu, kèm số dư đầu kỳ và cuối
/// kỳ đúng bộ khung dòng của mẫu gốc.
///
/// Giá trị tồn dùng `số lượng × đơn giá` trên chứng từ (đúng công thức cột
/// 3 = 1 × 2, 5 = 1 × 4, 7 = 1 × 6 của mẫu). Giá vốn dùng để tính thuế vẫn theo
/// FIFO như `load_fifo_cogs` — hai khái niệm khác nhau, đừng lấy số của sổ này
/// để kê khai.
async fn stock_ledger_rows(
    pool: &SqlitePool,
    from: &str,
    to: &str,
) -> Result<(Vec<BookRow>, Option<BookNote>), String> {
    let moves = sqlx::query!(
        r#"SELECT je.voucher_no AS "voucher_no!", je.posting_date AS "posting_date!",
                  je.description AS "description!", je.product_code AS "product_code!",
                  je.entry_type AS "entry_type!", je.quantity, je.unit_price,
                  COALESCE(je.adjust_code, '') AS "adjust_code!",
                  COALESCE(p.name, '') AS "product_name!", COALESCE(p.unit, '') AS "unit!"
             FROM journal_entry je
             LEFT JOIN product p ON p.code = je.product_code
            WHERE je.entry_type IN ('PN', 'PX')
              AND je.posting_date >= ? AND je.posting_date <= ?
              AND je.product_code <> ''
              AND (p.code IS NULL OR COALESCE(p.is_service, 0) = 0)
            ORDER BY je.posting_date, je.id"#,
        from,
        to
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    // Số dư trước kỳ: gom theo mã hàng từ toàn bộ lịch sử trước `from`.
    let opening = sqlx::query!(
        r#"SELECT product_code AS "product_code!",
                  SUM(CASE WHEN entry_type = 'PN' THEN quantity ELSE 0 END) AS "in_q!: f64",
                  SUM(CASE WHEN entry_type = 'PX' THEN quantity ELSE 0 END) AS "out_q!: f64",
                  SUM(CASE WHEN entry_type = 'PN' THEN quantity * unit_price ELSE 0 END) AS "in_v!: f64",
                  SUM(CASE WHEN entry_type = 'PX' THEN quantity * unit_price ELSE 0 END) AS "out_v!: f64"
             FROM journal_entry
            WHERE entry_type IN ('PN', 'PX')
              AND posting_date < ?
              AND product_code <> ''
            GROUP BY product_code"#,
        from
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    let mut open_map: std::collections::HashMap<String, (f64, f64)> = opening
        .into_iter()
        .map(|r| (r.product_code, (r.in_q - r.out_q, r.in_v - r.out_v)))
        .collect();

    // Gom dòng theo mã hàng.
    let mut order: Vec<String> = Vec::new();
    let mut by_code: std::collections::HashMap<String, Vec<_>> = std::collections::HashMap::new();
    for m in &moves {
        if !by_code.contains_key(&m.product_code) {
            order.push(m.product_code.clone());
        }
        by_code.entry(m.product_code.clone()).or_default().push(m);
    }
    // Mã chỉ có số dư đầu kỳ mà trong kỳ không phát sinh giao dịch vẫn phải hiện.
    for code in open_map.keys() {
        if !order.contains(code) {
            order.push(code.clone());
        }
    }
    order.sort();

    let mut rows: Vec<BookRow> = Vec::new();
    let mut negative = false;
    for code in &order {
        let Some(list) = by_code.get(code) else {
            // Số dư đầu kỳ có nhưng kỳ này không phát sinh gì — vẫn ghi ba dòng
            // đầu / phát sinh / cuối để sổ khớp với hàng còn nằm trong kho.
            let (q, v) = open_map.remove(code).unwrap_or((0.0, 0.0));
            let unit = unit_of(pool, code).await?;
            rows.push(balance_row(code, &unit, "Số dư đầu kỳ", q, v));
            rows.push(movement_row(code, 0.0, 0.0, 0.0, 0.0));
            rows.push(balance_row(code, &unit, "Số dư cuối kỳ", q, v));
            negative |= q < 0.0;
            continue;
        };
        let name = list
            .first()
            .map(|m| m.product_name.clone())
            .unwrap_or_default();
        let unit = list.first().map(|m| m.unit.clone()).unwrap_or_default();
        let label = if name.is_empty() {
            code.clone()
        } else {
            format!("{code} · {name}")
        };
        let (mut q, mut v) = open_map.remove(code).unwrap_or((0.0, 0.0));
        negative |= q < 0.0;
        rows.push(balance_row(&label, &unit, "Số dư đầu kỳ", q, v));
        let (mut in_q, mut in_v, mut out_q, mut out_v) = (0.0, 0.0, 0.0, 0.0);
        for m in list {
            let qty = m.quantity;
            let value = qty * m.unit_price;
            let mut r = BookRow::detail();
            r.text("item", &label);
            r.text("a", &m.voucher_no);
            r.text("b", vn_date(&m.posting_date));
            r.text("c", &m.description);
            r.text("d", &unit);
            r.money("1", m.unit_price);
            if m.entry_type == "PN" {
                q += qty;
                v += value;
                in_q += qty;
                in_v += value;
                r.qty("2", qty);
                r.money("3", value);
            } else {
                q -= qty;
                v -= value;
                out_q += qty;
                out_v += value;
                r.qty("4", qty);
                r.money("5", value);
            }
            r.qty("6", q);
            r.money("7", v);
            if m.adjust_code == "GiamCP" || m.adjust_code == "GiamDT" {
                r.text(
                    "8",
                    "Dòng điều chỉnh giảm — không phải giao dịch nhập/xuất thực tế",
                );
            }
            negative |= q < 0.0;
            rows.push(r);
        }
        rows.push(movement_row(&label, in_q, in_v, out_q, out_v));
        rows.push(balance_row(&label, &unit, "Số dư cuối kỳ", q, v));
    }

    let note = if negative {
        Some(BookNote {
            level: "warn",
            text: "Có mã hàng bị xuất nhiều hơn số đã nhập (số dư âm) — dữ liệu phiếu chưa khớp, cần rà lại các phiếu nhập kho trước khi nộp.".into(),
            action_book: None,
            action_label: None,
        })
    } else {
        Some(BookNote {
            level: "info",
            text: "Mẫu gốc mở một quyển sổ cho từng loại vật liệu; app gộp các mã hàng vào một bảng nên thêm cột 'Mã · Tên vật liệu' ở trước cột A. Giá trị tồn tính bằng số lượng × đơn giá trên chứng từ; giá vốn tính thuế vẫn theo FIFO như tờ khai.".into(),
            action_book: None,
            action_label: None,
        })
    };
    Ok((rows, note))
}

/// Đơn vị tính của một mã hàng (dùng cho dòng số dư khi kỳ không phát sinh).
async fn unit_of(pool: &SqlitePool, code: &str) -> Result<String, String> {
    let u = sqlx::query_scalar!(
        r#"SELECT COALESCE(unit, '') AS "unit!: String" FROM product WHERE code = ?"#,
        code
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(u.unwrap_or_default())
}

/// Dòng số dư (đầu kỳ / cuối kỳ) của sổ vật liệu: cột 6 = số lượng, cột 7 = giá
/// trị, cột 1 = đơn giá bình quân để cột 7 = cột 1 × cột 6 đúng công thức mẫu.
fn balance_row(item: &str, unit: &str, label: &str, qty: f64, value: f64) -> BookRow {
    let avg = if qty.abs() > 1e-9 { value / qty } else { 0.0 };
    let mut r = if label == "Số dư đầu kỳ" {
        BookRow::subtotal()
    } else {
        BookRow::total()
    };
    r.text("item", item);
    r.text("c", label);
    r.text("d", unit);
    r.money("1", avg);
    r.qty("6", qty);
    r.money("7", value);
    r
}

/// Dòng "Công phát sinh trong kỳ" của mẫu S2d — tổng cột 2/3 (nhập) và 4/5
/// (xuất) của kỳ. Mẫu gốc đánh dấu cột D (đơn vị tính) và cột 1 (đơn giá) là `X`
/// vì dòng tổng không có đơn vị, đơn giá riêng; không có số thì để trống ô đó
/// thay vì ghi số 0 vào chỗ mẫu in để trắng.
fn movement_row(item: &str, in_q: f64, in_v: f64, out_q: f64, out_v: f64) -> BookRow {
    let mut r = BookRow::subtotal();
    r.text("item", item);
    r.text("c", "Công phát sinh trong kỳ");
    r.text("d", "X");
    r.text("1", "X");
    if in_q != 0.0 || in_v != 0.0 {
        r.qty("2", in_q);
        r.money("3", in_v);
    }
    if out_q != 0.0 || out_v != 0.0 {
        r.qty("4", out_q);
        r.money("5", out_v);
    }
    r
}

/// Dòng sổ S2e — sổ chi tiết tiền.
///
/// Mẫu gốc chia thành khối **tiền mặt** và khối **tiền gửi không kỳ hạn** (mở
/// riêng từng ngân hàng nếu có); mỗi khối có số dư đầu kỳ, các dòng thu/chi,
/// tổng thu, tổng chi và số dư cuối kỳ. App ghi mọi phát sinh đi qua tài khoản
/// tiền — không chỉ phiếu thu/phiếu chi — vì đó mới là dòng tiền thật.
async fn cash_ledger_rows(pool: &SqlitePool, from: &str, to: &str) -> Result<Vec<BookRow>, String> {
    // Khối in đậm đúng văn bản mẫu in: "Tiền mặt" / "Tiền gửi không kỳ hạn".
    let accounts = [("111", "Tiền mặt"), ("112", "Tiền gửi không kỳ hạn")];
    let mut rows: Vec<BookRow> = Vec::new();

    for (code, title) in accounts {
        let opening = opening_balance(pool, code, from).await?;
        let moves = sqlx::query!(
            r#"SELECT je.voucher_no AS "voucher_no!", je.posting_date AS "posting_date!",
                      je.description AS "description!", je.amount,
                      je.bank_code AS "bank_code!",
                      CASE WHEN je.debit_account = ? THEN 1 ELSE 0 END AS "in_side!: i64"
                 FROM journal_entry je
                WHERE je.posting_date >= ? AND je.posting_date <= ?
                  AND (je.debit_account = ? OR je.credit_account = ?)
                ORDER BY je.posting_date, je.id"#,
            code,
            from,
            to,
            code,
            code
        )
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
        if opening == 0.0 && moves.is_empty() {
            continue;
        }

        // Tiền gửi: tách từng ngân hàng khi chứng từ có ghi mã ngân hàng.
        let mut banks: Vec<String> = Vec::new();
        for m in &moves {
            let b = m.bank_code.trim().to_string();
            if !b.is_empty() && !banks.contains(&b) {
                banks.push(b);
            }
        }
        let has_blank = moves.iter().any(|m| m.bank_code.trim().is_empty());
        let groups: Vec<(String, String)> = if code == "111" || banks.is_empty() {
            vec![(String::new(), title.to_string())]
        } else {
            let mut g: Vec<(String, String)> = banks
                .iter()
                .map(|b| (b.clone(), format!("{title} — NGÂN HÀNG {b}")))
                .collect();
            if has_blank {
                g.insert(0, (String::new(), title.to_string()));
            }
            g
        };

        // Mẫu gốc có một dòng khối in đậm "Tiền mặt" / "Tiền gửi không kỳ hạn"
        // trước, riêng khối tiền gửi mới có thêm dòng "Ngân hàng …" cho từng ngân
        // hàng bên dưới.
        let mut sec = BookRow::section();
        sec.text("c", title);
        rows.push(sec);

        let group_count = groups.len();
        for (bank, _group_title) in groups {
            if code == "112" {
                let mut r = BookRow::subtotal();
                r.text(
                    "c",
                    if bank.is_empty() {
                        "Ngân hàng ....".to_string()
                    } else {
                        format!("Ngân hàng {bank}")
                    },
                );
                rows.push(r);
            }
            let list: Vec<_> = moves
                .iter()
                .filter(|m| {
                    if bank.is_empty() {
                        m.bank_code.trim().is_empty()
                    } else {
                        m.bank_code.trim() == bank
                    }
                })
                .collect();
            // Số dư đầu kỳ chia theo tỉ lệ giá trị giao dịch của từng khối — chỉ
            // có nghĩa khi một tài khoản tiền gửi theo dõi nhiều ngân hàng.
            let open_value = if group_count > 1 {
                let total_amount: f64 = moves.iter().map(|m| m.amount).sum();
                let this_amount: f64 = list.iter().map(|m| m.amount).sum();
                if total_amount > 0.0 {
                    round2(opening * this_amount / total_amount)
                } else {
                    0.0
                }
            } else {
                opening
            };

            let mut r = BookRow::subtotal();
            r.text(
                "c",
                if code == "111" {
                    "Tiền mặt đầu kỳ"
                } else {
                    "Tiền gửi đầu kỳ"
                },
            );
            r.money("1", open_value);
            rows.push(r);

            let (mut total_in, mut total_out) = (0.0, 0.0);
            for m in list {
                let incoming = m.in_side == 1;
                let mut r = BookRow::detail();
                r.text("a", &m.voucher_no);
                r.text("b", vn_date(&m.posting_date));
                r.text(
                    "c",
                    format!(
                        "{} — {}",
                        if incoming { "Thu" } else { "Chi" },
                        m.description
                    ),
                );
                if incoming {
                    r.money("1", m.amount);
                    total_in += m.amount;
                } else {
                    r.money("2", m.amount);
                    total_out += m.amount;
                }
                rows.push(r);
            }
            let mut r = BookRow::subtotal();
            r.text(
                "c",
                if code == "111" {
                    "Tổng tiền thu vào trong kỳ"
                } else {
                    "Tổng gửi vào trong kỳ"
                },
            );
            r.money("1", total_in);
            rows.push(r);
            let mut r = BookRow::subtotal();
            r.text(
                "c",
                if code == "111" {
                    "Tổng tiền chi ra trong kỳ"
                } else {
                    "Tổng tiền rút ra trong kỳ"
                },
            );
            r.money("2", total_out);
            rows.push(r);
            let mut r = BookRow::total();
            r.text(
                "c",
                if code == "111" {
                    "Tiền mặt tồn cuối kỳ"
                } else {
                    "Tiền gửi cuối kỳ"
                },
            );
            r.money("1", round2(open_value + total_in - total_out));
            rows.push(r);
        }
    }
    Ok(rows)
}

/// Số dư tài khoản `code` tính tới ngay trước ngày `from`.
async fn opening_balance(pool: &SqlitePool, code: &str, from: &str) -> Result<f64, String> {
    let base = sqlx::query!(
        r#"SELECT opening_debit AS "d!", opening_credit AS "c!"
             FROM account WHERE code = ?"#,
        code
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    .map(|r| (r.d, r.c))
    .unwrap_or((0.0, 0.0));
    let moved = sqlx::query!(
        r#"SELECT COALESCE(SUM(CASE WHEN debit_account = ? THEN amount ELSE 0 END), 0) AS "debit_sum!: f64",
                  COALESCE(SUM(CASE WHEN credit_account = ? THEN amount ELSE 0 END), 0) AS "credit_sum!: f64"
             FROM journal_entry
            WHERE posting_date < ? AND (debit_account = ? OR credit_account = ?)"#,
        code,
        code,
        from,
        code,
        code
    )
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(round2(base.0 - base.1 + moved.debit_sum - moved.credit_sum))
}

/// Sổ S3a — theo dõi nghĩa vụ thuế khác.
///
/// App **chưa lưu giao dịch** của các loại thuế này: `journal_entry` chỉ có
/// PN/PX/PT/PC, còn `product.import_tax_rate` chỉ là thuế suất ở mức danh mục
/// chứ không gắn với từng lần nhập. Số liệu suy ra từ đó sẽ không khớp tờ khai
/// thật, nên sổ trả về rỗng thay vì điền số bịa — phần đầu sổ và bộ cột 1-10 vẫn
/// đầy đủ để in ra, chỉ là chưa có dòng số liệu.
fn other_tax_ledger_rows() -> Vec<BookRow> {
    Vec::new()
}

/// Số tiền dạng văn bản có dấu chấm phân tách nghìn: `12000000` → `12.000.000`.
fn fmt_thousands(v: f64) -> String {
    let n = round2(v).round() as i64;
    let digits = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(c);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}

/// Nhãn kỳ hiển thị trên đầu sổ: `Quý 3/2026`, `Tháng 9/2026`, `Năm 2026`.
fn describe_tax_period(period: &str, no: i64, year: i64) -> String {
    match period {
        "quarter" => format!("Quý {}/{}", no, year),
        "month" => format!("Tháng {}/{}", no, year),
        _ => format!("Năm {}", year),
    }
}

/// Tổng hợp doanh thu - chi phí (≡ Tong Hop DTCP sheet)
///
/// Chi phí gồm hai phần phải cộng vào với nhau: chứng từ chi trong kỳ **và** giá
/// vốn của hàng đã xuất kho tính theo FIFO. Không cộng giá vốn thì hàng bán ra
/// không hiện ở cột chi phí nào, nên "lợi nhuận trước thuế" in ra lớn hơn lợi
/// nhuận thật (khác với số quyết toán trên cùng màn hình).
#[tauri::command]
pub(crate) async fn get_revenue_expense(
    state: State<'_, AppState>,
    from_date: String,
    to_date: String,
) -> Result<String, String> {
    let pool = state.pool.read().await;
    Ok(get_revenue_expense_core(&pool, &from_date, &to_date)
        .await?
        .to_string())
}

/// Phần lõi của [`get_revenue_expense`] — tách ra để test được với `test_pool`.
pub(crate) async fn get_revenue_expense_core(
    pool: &SqlitePool,
    from_date: &str,
    to_date: &str,
) -> Result<serde_json::Value, String> {
    let row = sqlx::query!(
        "SELECT
            -- 'XuatNVL' = phiếu xuất NVL tự sinh theo định mức (F4): không phải
            -- doanh thu nên LOẠI khỏi mọi sổ doanh thu / giá vốn / bán hàng.
            COALESCE(SUM(CASE WHEN entry_type = 'PX' AND adjust_code NOT IN ('GiamDT', 'XuatNVL') THEN amount ELSE 0.0 END), 0.0) AS revenue_up,
            COALESCE(SUM(CASE WHEN entry_type = 'PX' AND adjust_code = 'GiamDT' THEN amount ELSE 0.0 END), 0.0) AS revenue_down,
            COALESCE(SUM(CASE WHEN entry_type = 'PC' AND adjust_code != 'GiamCP'
                               AND COALESCE(debit_account, '') != '331' THEN amount ELSE 0.0 END), 0.0) AS expense_up,
            COALESCE(SUM(CASE WHEN entry_type = 'PC' AND adjust_code = 'GiamCP' THEN amount ELSE 0.0 END), 0.0) AS expense_down
         FROM journal_entry
         WHERE posting_date >= ?
           AND posting_date <= ?",
        from_date,
        to_date
    )
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;
    let cogs = load_fifo_cogs(pool, from_date, to_date).await?;
    Ok(serde_json::json!({
        "revenue_up": row.revenue_up,
        "revenue_down": row.revenue_down,
        "expense_up": row.expense_up,
        "expense_down": row.expense_down,
        "cogs": round2(cogs),
    }))
}

/// Tổng hợp tồn kho theo sản phẩm (≡ Vat Tu NXT sheet)
#[tauri::command]
pub(crate) async fn get_inventory_summary(state: State<'_, AppState>) -> Result<String, String> {
    let rows: Vec<InventoryRow> = sqlx::query_as!(
        InventoryRow,
        "SELECT
            p.code AS product_code,
            p.name AS product_name,
            p.unit,
            COALESCE(SUM(CASE WHEN je.entry_type = 'PN' AND je.adjust_code != 'GiamCP' THEN je.quantity ELSE 0.0 END), 0.0)
              - COALESCE(SUM(CASE WHEN je.entry_type = 'PN' AND je.adjust_code = 'GiamCP' THEN je.quantity ELSE 0.0 END), 0.0) AS inbound,
            COALESCE(SUM(CASE WHEN je.entry_type = 'PX' AND je.adjust_code != 'GiamDT' THEN je.quantity ELSE 0.0 END), 0.0) AS outbound,
            (COALESCE(SUM(CASE WHEN je.entry_type = 'PN' AND je.adjust_code != 'GiamCP' THEN je.quantity ELSE 0.0 END), 0.0)
              - COALESCE(SUM(CASE WHEN je.entry_type = 'PN' AND je.adjust_code = 'GiamCP' THEN je.quantity ELSE 0.0 END), 0.0)
              - COALESCE(SUM(CASE WHEN je.entry_type = 'PX' AND je.adjust_code != 'GiamDT' THEN je.quantity ELSE 0.0 END), 0.0)) AS balance
         FROM product p
         LEFT JOIN journal_entry je ON je.product_code = p.code
         GROUP BY p.code
         ORDER BY p.code"
    )
    .fetch_all(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    Ok(serde_json::to_string(&rows).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::stock::save_outbound_core;
    use crate::models::{OutboundInvoiceInput, OutboundItemInput};
    use crate::test_support::*;

    fn agg(up: f64, down: f64) -> TaxAgg {
        TaxAgg {
            industry_code: "PPHH".into(),
            industry_name: "Phân phối, cung cấp hàng hóa".into(),
            vat_rate: 0.01,
            pit_rate: 0.005,
            revenue_up: up,
            revenue_down: down,
        }
    }

    /// Doanh thu tính thuế TNCN = toàn bộ doanh thu của kỳ (chưa trừ ngưỡng 01 tỷ).
    fn full(code: &str, value: f64) -> std::collections::HashMap<String, f64> {
        [(code.to_string(), value)].into_iter().collect()
    }

    /// Chèn 1 dòng sổ với cặp Nợ/Có tự chọn — dùng cho test chi phí.
    async fn add_journal_row(
        pool: &SqlitePool,
        posting_date: &str,
        voucher_no: &str,
        entry_type: &str,
        debit: &str,
        credit: &str,
        amount: f64,
    ) {
        sqlx::query!(
            "INSERT INTO journal_entry
             (posting_date, voucher_no, doc_date, entry_type, description,
              quantity, unit_price, amount, debit_account, credit_account,
              industry_code, vat_rate, pit_rate, unit_code, adjust_code, note)
             VALUES (?, ?, ?, ?, 'test', 1.0, ?, ?, ?, ?, '', 0.0, 0.0, 'HKD', '', '')",
            posting_date,
            voucher_no,
            posting_date,
            entry_type,
            amount,
            amount,
            debit,
            credit
        )
        .execute(pool)
        .await
        .expect("add journal row");
    }

    /// PC trả nhà cung cấp là **thanh toán phải trả** (Nợ 331), không phải chi phí:
    /// tiền hàng đã nằm ở giá vốn khi xuất kho. Trước đây cả hai đều bị cộng vào
    /// → hàng mua bị tính hai lần (PC001 đúng bằng tổng PN0131 trong dữ liệu thật).
    #[tokio::test]
    async fn pc_tra_nha_cung_cap_khong_vao_chi_phi() {
        let pool = test_pool().await;
        add_journal_row(&pool, "2026-03-01", "PN001", "PN", "152", "331", 500_000.0).await;
        add_journal_row(&pool, "2026-03-02", "PC001", "PC", "331", "111", 500_000.0).await;
        add_journal_row(&pool, "2026-03-03", "PC002", "PC", "642", "111", 150_000.0).await;

        let (cost_ok, cost_no, _) = load_period_costs(&pool, "2026-03-01", "2026-03-31")
            .await
            .expect("chi phí kỳ");
        assert_eq!(cost_ok, 150_000.0, "chỉ chi phí thật mới vào chi phí kỳ");
        assert_eq!(cost_no, 0.0);

        let out = get_revenue_expense_core(&pool, "2026-03-01", "2026-03-31")
            .await
            .expect("tổng hợp DT-CP");
        assert_eq!(
            out["expense_up"].as_f64().expect("expense_up"),
            150_000.0,
            "màn Tổng hợp DT-CP cũng phải loại PC trả nhà cung cấp"
        );
    }

    #[test]
    fn to_khai_tinh_gtgt_toan_bo_khong_con_giam_thue() {
        // DT 100tr, ngành PPHH 1%, hộ nhóm 2 → GTGT phải nộp = 1.000.000
        // (chức năng giảm thuế GTGT đã bỏ — không còn trừ khoản "được giảm")
        let out = build_tax_declaration(
            vec![agg(100_000_000.0, 0.0)],
            "revenue",
            2,
            &full("PPHH", 100_000_000.0),
            true,
            &std::collections::HashMap::new(),
        );
        let r = &out[0];
        assert_eq!(r.vat_tax, 1_000_000.0);
        assert_eq!(r.vat_payable, 1_000_000.0);
        assert_eq!(r.pit_tax, 500_000.0); // 100tr x 0,5%
    }

    #[test]
    fn to_khai_tru_doanh_thu_dieu_chinh_giam() {
        // DT tăng 100tr, DT giảm 10tr → còn 90tr
        let out = build_tax_declaration(
            vec![agg(100_000_000.0, 10_000_000.0)],
            "revenue",
            2,
            &full("PPHH", 90_000_000.0),
            true,
            &std::collections::HashMap::new(),
        );
        let r = &out[0];
        assert_eq!(r.revenue_up, 100_000_000.0);
        assert_eq!(r.revenue_down, 10_000_000.0);
        assert_eq!(r.vat_tax, 900_000.0); // 90tr x 1%
        assert_eq!(r.vat_payable, 900_000.0);
        assert_eq!(r.pit_tax, 450_000.0); // 90tr x 0,5%
    }

    /// Ngành nghề không có doanh thu trong kỳ thì không được liệt kê trên tờ
    /// khai — mẫu chỉ ghi ngành có phát sinh (phiếu ghi giá 0 không tạo ra
    /// dòng báo cáo), nhưng ngành có tăng/giảm bằng nhau thì vẫn phải ghi.
    #[test]
    fn to_khai_khong_liet_ke_nhom_nganh_khong_phat_sinh() {
        fn nganh(revenue_up: f64, revenue_down: f64) -> TaxAgg {
            TaxAgg {
                industry_code: "DV-TS".into(),
                industry_name: "Cho thuê tài sản, đại lý bảo hiểm, xổ số, bán hàng đa cấp".into(),
                vat_rate: 0.05,
                pit_rate: 0.05,
                revenue_up,
                revenue_down,
            }
        }

        // Cả kỳ không có gì → mẫu không còn dòng ngành trống.
        let out = build_tax_declaration(
            vec![nganh(0.0, 0.0), agg(100_000_000.0, 0.0)],
            "revenue",
            2,
            &full("PPHH", 100_000_000.0),
            true,
            &std::collections::HashMap::new(),
        );
        assert_eq!(out.len(), 1, "chỉ ngành có doanh thu mới lên tờ khai");
        assert_eq!(out[0].industry_code, "PPHH");

        // Bán 500k rồi trả lại 500k → ròng 0 nhưng đã phát sinh → vẫn ghi.
        let out = build_tax_declaration(
            vec![nganh(500_000.0, 500_000.0)],
            "revenue",
            2,
            &full("DV-TS", 0.0),
            true,
            &std::collections::HashMap::new(),
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].revenue_up, 500_000.0);
        assert_eq!(out[0].revenue_down, 500_000.0);
    }

    #[test]
    fn chua_vuot_nguong_khong_phat_sinh_thue_du_nhom_trong_ho_sao_la_2() {
        // Hồ sơ ghi Nhóm 2 nhưng doanh thu mới 12,4 triệu → kỳ chưa vượt ngưỡng
        // nên KHÔNG phát sinh thuế (Điều 8 khoản 1a). Nhãn nhóm trong hồ sơ không
        // tạo ra thuế giả, tránh tờ khai "GTGT có, TNCN không" gây khó hiểu.
        let out = build_tax_declaration(
            vec![agg(12_401_999.0, 0.0)],
            "revenue",
            2,
            &full("PPHH", 0.0),
            false,
            &std::collections::HashMap::new(),
        );
        let r = &out[0];
        assert_eq!(r.vat_tax, 0.0);
        assert_eq!(r.vat_payable, 0.0);
        assert_eq!(r.pit_tax, 0.0);
        // Doanh thu vẫn hiển thị để theo dõi ngưỡng
        assert_eq!(r.revenue_up, 12_401_999.0);
    }

    #[test]
    fn vuot_nguong_thi_phai_nop_thue_du_nhom_trong_ho_sao_la_1() {
        // Ngược lại: hồ sơ ghi Nhóm 1 nhưng kỳ đã vượt ngưỡng → phải nộp thuế,
        // không được miễn chỉ vì nhãn nhóm trong hồ sơ.
        let out = build_tax_declaration(
            vec![agg(1_200_000_000.0, 0.0)],
            "revenue",
            1,
            &full("PPHH", 200_000_000.0),
            true,
            &std::collections::HashMap::new(),
        );
        let r = &out[0];
        assert_eq!(r.vat_tax, 12_000_000.0); // 1,2 tỷ x 1%
        assert_eq!(r.pit_tax, 1_000_000.0); // 200tr x 0,5%
        assert_eq!(r.revenue_taxable, 200_000_000.0);
    }

    #[test]
    fn nhom2_theo_loi_nhuan_thi_tncn_khong_tinh_tren_dong() {
        // Phương pháp lợi nhuận: cột TNCN theo tỷ lệ doanh thu bằng 0
        // (số TNCN = lợi nhuận x 15% tính ở phần tổng hợp)
        let out = build_tax_declaration(
            vec![agg(2_000_000_000.0, 0.0)],
            "profit",
            2,
            &full("PPHH", 0.0),
            true,
            &std::collections::HashMap::new(),
        );
        assert_eq!(out[0].pit_tax, 0.0);
        assert_eq!(out[0].vat_tax, 20_000_000.0); // 2 tỷ x 1%
    }

    #[test]
    fn nhom3_4_tncn_luon_theo_loi_nhuan() {
        // Nhóm 3 (> 3–50 tỷ): TNCN không theo tỷ lệ doanh thu, kể cả khi cấu hình "revenue"
        let out = build_tax_declaration(
            vec![agg(10_000_000_000.0, 0.0)],
            "revenue",
            3,
            &full("PPHH", 0.0),
            true,
            &std::collections::HashMap::new(),
        );
        assert_eq!(out[0].pit_tax, 0.0);
        assert_eq!(out[0].vat_tax, 100_000_000.0); // 10 tỷ x 1%
    }

    #[test]
    fn xep_nhom_theo_tong_doanh_thu_nam() {
        // Ngưỡng: ≤ 1 tỷ = nhóm 1 (miễn); > 1–3 tỷ = 2; > 3–50 tỷ = 3; > 50 tỷ = 4
        assert_eq!(tax_group_with(500_000_000.0, &TaxThresholds::default()), 1);
        assert_eq!(
            tax_group_with(1_000_000_000.0, &TaxThresholds::default()),
            1
        ); // 1 tỷ (bằng ngưỡng) → miễn
        assert_eq!(
            tax_group_with(1_000_000_000.01, &TaxThresholds::default()),
            2
        );
        assert_eq!(
            tax_group_with(3_000_000_000.0, &TaxThresholds::default()),
            2
        );
        assert_eq!(
            tax_group_with(3_000_000_000.01, &TaxThresholds::default()),
            3
        );
        assert_eq!(
            tax_group_with(50_000_000_000.0, &TaxThresholds::default()),
            3
        );
        assert_eq!(
            tax_group_with(50_000_000_000.01, &TaxThresholds::default()),
            4
        );
    }

    #[test]
    fn han_nop_chi_hien_khi_khoang_date_trung_ky_khai() {
        // Trùng kỳ khai → có hạn nộp để in ra tờ khai nộp cơ quan thuế.
        assert_eq!(
            period_of_range("2026-01-01", "2026-12-31"),
            Some(("year".to_string(), 0))
        );
        assert_eq!(
            period_of_range("2026-07-01", "2026-09-30"),
            Some(("quarter".to_string(), 3))
        );
        assert_eq!(
            period_of_range("2026-09-01", "2026-09-30"),
            Some(("month".to_string(), 9))
        );
        // Năm nhuận: 29/02 vẫn là ngày cuối tháng 2.
        assert_eq!(
            period_of_range("2028-02-01", "2028-02-29"),
            Some(("month".to_string(), 2))
        );
        // Khoảng tùy ý (xem cho nhanh) hoặc qua nhiều năm → không có hạn nộp,
        // vì tờ khai chỉ nộp theo kỳ.
        assert_eq!(period_of_range("2026-09-01", "2026-09-15"), None);
        assert_eq!(period_of_range("2026-07-01", "2026-09-29"), None);
        assert_eq!(period_of_range("2025-12-01", "2026-01-31"), None);
        assert_eq!(period_of_range("x", "y"), None);
    }

    #[test]
    fn nhom_ho_ua_tien_hon_hom_so() {
        // Chưa chốt ở đâu cả → dùng nhóm app xếp từ doanh thu.
        assert_eq!(effective_tax_group(None, None, 1), 1);
        assert_eq!(effective_tax_group(None, None, 3), 3);
        // Nhóm đã ghim cho năm (hộ vượt ngưỡng, hồ sơ chưa chốt) → giữ nguyên cả năm,
        // doanh thu tăng tiếp cũng không tự lên nhóm kế tiếp (nguyên tắc ổn định).
        assert_eq!(effective_tax_group(None, Some(2), 3), 2);
        // Hồ sơ HKD chốt → thắng tuyệt đối, kể cả khi lệch với doanh thu thực tế.
        assert_eq!(effective_tax_group(Some(3), Some(2), 1), 3);
        assert_eq!(effective_tax_group(Some(1), None, 4), 1);
        // Giá trị ngoài 1–4 coi như chưa chốt thay vì tin vào.
        assert_eq!(effective_tax_group(Some(0), None, 4), 4);
        assert_eq!(effective_tax_group(Some(7), None, 1), 1);
    }

    #[test]
    fn nguong_toi_da_duoc_chinh() {
        let raw = std::collections::HashMap::from([
            ("tax_threshold_exempt".to_string(), "1500000000".to_string()),
            ("tax_threshold_group3".to_string(), "4000000000".to_string()),
            (
                "tax_threshold_group4".to_string(),
                "60000000000".to_string(),
            ),
        ]);
        let th = TaxThresholds::from_settings(&raw);
        assert_eq!(th.exempt, 1_500_000_000.0);
        assert_eq!(th.group3, 4_000_000_000.0);
        assert_eq!(th.group4, 60_000_000_000.0);
        assert_eq!(tax_group_with(1_500_000_000.0, &th), 1);
        assert_eq!(tax_group_with(1_500_000_001.0, &th), 2);
        assert_eq!(tax_group_with(4_000_000_001.0, &th), 3);
        assert_eq!(tax_group_with(60_000_000_001.0, &th), 4);
    }

    #[test]
    fn nguong_rac_hoi_thi_giu_nguong_mac_dinh() {
        // Rác (không đọc được, số âm, 0) và thứ tự lộn xộn đều không làm hỏng tờ khai.
        let raw = std::collections::HashMap::from([
            ("tax_threshold_exempt".to_string(), "abc".to_string()),
            ("tax_threshold_group3".to_string(), "-5".to_string()),
            ("tax_threshold_group4".to_string(), "1000000000".to_string()),
        ]);
        let th = TaxThresholds::from_settings(&raw);
        assert_eq!(th.exempt, THRESHOLD_EXEMPT);
        assert_eq!(th.group3, THRESHOLD_GROUP3);
        // group4 nhỏ hơn group3 → bị nâng lên bằng group3 để nhóm xếp không lộn xộn.
        assert_eq!(th.group4, th.group3);
    }

    #[test]
    fn nhom_ghim_theo_nam_luu_va_doc_lai() {
        let mut g = FrozenGroups::default();
        assert_eq!(g.get(2026), None);
        assert!(g.set_if_absent(2026, 2));
        assert!(!g.set_if_absent(2026, 3), "không ghi đè nhóm đã chốt");
        assert!(g.set_if_absent(2027, 1));
        assert!(!g.set_if_absent(2028, 9), "nhóm ngoài 1–4 bị bỏ");
        let back = FrozenGroups::from_json(Some(&g.to_json()));
        assert_eq!(back.get(2026), Some(2));
        assert_eq!(back.get(2027), Some(1));
        assert_eq!(back.get(2028), None);
        // JSON rác → coi như chưa chốt gì, không làm hỏng tờ khai.
        assert_eq!(
            FrozenGroups::from_json(Some(&"{oops".to_string())),
            FrozenGroups::default()
        );
        assert_eq!(FrozenGroups::from_json(None), FrozenGroups::default());
    }

    #[test]
    fn thue_suat_tncn_theo_loi_nhuan() {
        assert_eq!(profit_tncn_rate(2), 0.15);
        assert_eq!(profit_tncn_rate(3), 0.17);
        assert_eq!(profit_tncn_rate(4), 0.20);
    }

    #[tokio::test]
    async fn ho_ho_so_nhom_2_thi_phai_nop_thue_du_dt_chua_vuot_nguong() {
        // Trường hợp người dùng thật gặp: hồ sơ chốt Nhóm 2, doanh thu năm chỉ
        // 12,4 triệu (chưa vượt ngưỡng 01 tỷ) nhưng tờ khai ra thuế = 0. Sai, vì
        // ngưỡng chỉ dùng XẾP NHÓM chứ không phải khoản miễn cho hộ đã thuộc diện
        // nộp thuế — khi đó thuế tính trên toàn bộ doanh thu của kỳ.
        let pool = test_pool().await;
        seed_product(&pool, "P1", "Hàng 1", 0.01).await;
        sqlx::query("UPDATE business SET tax_group = 2")
            .execute(&pool)
            .await
            .unwrap();
        add_journal_px(
            &pool,
            "2026-07-10",
            "PX-G2",
            "P1",
            "PPHH",
            12.0,
            1_000_000.0,
            12_000_000.0,
            0.01,
            0.005,
        )
        .await;

        let ctx = load_tax_context(&pool, 2026).await.unwrap();
        assert_eq!(ctx.group, 2, "nhóm lấy từ hồ sơ");
        assert!(
            ctx.taxed_from_start,
            "chốt Nhóm 2 là thuộc diện nộp thuế cả năm"
        );
        // Mức trừ vẫn còn, nhưng không bao giờ vượt doanh thu thật được: cả năm mới
        // 12 triệu thì trừ hết 12 triệu → doanh thu tính thuế TNCN bằng 0.
        assert_eq!(
            ctx.alloc.iter().map(|(_, v)| *v).sum::<f64>(),
            12_000_000.0,
            "mức trừ bị giới hạn bởi doanh thu thật được"
        );

        // Kỳ có doanh thu → phải chịu thuế dù lũy kế 12 triệu < 01 tỷ.
        let period_map = std::collections::HashMap::from([("PPHH".to_string(), 12_000_000.0)]);
        let split = split_by_period(&Default::default(), &period_map, &ctx.alloc);
        let taxable: std::collections::HashMap<String, f64> =
            split.iter().map(|(c, v, _)| (c.clone(), *v)).collect();
        let deducted: std::collections::HashMap<String, f64> =
            split.iter().map(|(c, _, d)| (c.clone(), *d)).collect();
        // Cả năm mới 12 triệu → kỳ này bị trừ hết, cơ sở tính TNCN bằng 0.
        assert_eq!(deducted.get("PPHH").copied(), Some(12_000_000.0));
        assert!(period_is_taxable(
            0.0,
            12_000_000.0,
            ctx.settings.thresholds.exempt,
            true
        ));

        let rows = load_tax_agg_range(&pool, 2026, 7, 7).await.unwrap();
        let out = build_tax_declaration(
            rows,
            "revenue",
            ctx.group,
            &taxable,
            period_is_taxable(0.0, 12_000_000.0, ctx.settings.thresholds.exempt, true),
            &deducted,
        );
        // Tờ khai xếp theo mã nhóm ngành → lấy đúng dòng PPHH.
        let r = out
            .iter()
            .find(|r| r.industry_code == "PPHH")
            .expect("dòng PPHH");
        // GTGT: 1% × toàn bộ doanh thu ghi trên hóa đơn (ngưỡng không trừ cho GTGT).
        assert_eq!(r.vat_tax, 120_000.0, "12 triệu × 1%");
        // TNCN: doanh thu tính thuế = phần còn lại sau khi trừ mức trừ lũy kế 01 tỷ
        // (12 triệu chưa vượt mức trừ nên bằng 0) → không phát sinh thuế TNCN.
        assert_eq!(r.revenue_taxable, 0.0, "12 triệu < mức trừ 01 tỷ");
        assert_eq!(r.pit_tax, 0.0);
    }

    #[tokio::test]
    async fn to_khai_tu_db_tong_hop_theo_ky() {
        let pool = test_pool().await;
        seed_product(&pool, "P1", "Hàng 1", 0.01).await;
        seed_product(&pool, "P2", "Hàng 2", 0.01).await;

        // Trong kỳ 05/2026: P1 1 tỷ + P2 500tr → tổng năm > 1 tỷ (nhóm 2)
        add_journal_px(
            &pool,
            "2026-05-10",
            "PX1",
            "P1",
            "PPHH",
            1000.0,
            1_000_000.0,
            1_000_000_000.0,
            0.01,
            0.005,
        )
        .await;
        add_journal_px(
            &pool,
            "2026-05-11",
            "PX2",
            "P2",
            "PPHH",
            500.0,
            1_000_000.0,
            500_000_000.0,
            0.01,
            0.005,
        )
        .await;
        // Ngoài kỳ (06/2026) — không được tính vào tờ khai tháng 5
        add_journal_px(
            &pool,
            "2026-06-01",
            "PX3",
            "P2",
            "PPHH",
            100.0,
            1_000_000.0,
            100_000_000.0,
            0.01,
            0.005,
        )
        .await;

        let out = build_tax_declaration(
            load_tax_agg(&pool, 2026, 5).await.unwrap(),
            "revenue",
            2,
            &full("PPHH", 1_500_000_000.0),
            true,
            &std::collections::HashMap::new(),
        );
        let pphh = out.iter().find(|r| r.industry_code == "PPHH").unwrap();
        assert_eq!(pphh.revenue_up, 1_500_000_000.0); // chỉ PX1 + PX2
        assert_eq!(pphh.vat_tax, 15_000_000.0); // 1,5 tỷ x 1%
        assert_eq!(pphh.vat_payable, 15_000_000.0); // không còn giảm thuế
                                                    // TNCN tính trên DOANH THU TÍNH THUẾ (đã trừ mức ngưỡng), không phải
                                                    // toàn bộ doanh thu: ở đây test truyền 1,5 tỷ → 1,5 tỷ x 0,5%.
        assert_eq!(pphh.revenue_taxable, 1_500_000_000.0);
        assert_eq!(pphh.pit_tax, 7_500_000.0); // 1,5 tỷ x 0,5%

        // Các ngành khác không phát sinh doanh thu
        for r in out.iter().filter(|r| r.industry_code != "PPHH") {
            assert_eq!(r.revenue_up, 0.0);
            assert_eq!(r.vat_tax, 0.0);
        }

        // Khai cả năm gồm cả PX3 → 1,6 tỷ
        let year = build_tax_declaration(
            load_tax_agg(&pool, 2026, 0).await.unwrap(),
            "revenue",
            2,
            &full("PPHH", 1_600_000_000.0),
            true,
            &std::collections::HashMap::new(),
        );
        let pphh_year = year.iter().find(|r| r.industry_code == "PPHH").unwrap();
        assert_eq!(pphh_year.revenue_up, 1_600_000_000.0);
    }

    #[tokio::test]
    async fn to_khai_tu_db_lap_quy() {
        // Kỳ khai theo quý 2/2026 gồm tháng 4 + 5 + 6
        let pool = test_pool().await;
        seed_product(&pool, "P1", "Hàng thường", 0.01).await;
        add_journal_px(
            &pool,
            "2026-04-10",
            "PX1",
            "P1",
            "PPHH",
            100.0,
            1_000_000.0,
            100_000_000.0,
            0.01,
            0.005,
        )
        .await;
        add_journal_px(
            &pool,
            "2026-05-10",
            "PX2",
            "P1",
            "PPHH",
            100.0,
            1_000_000.0,
            100_000_000.0,
            0.01,
            0.005,
        )
        .await;
        add_journal_px(
            &pool,
            "2026-06-10",
            "PX3",
            "P1",
            "PPHH",
            100.0,
            1_000_000.0,
            100_000_000.0,
            0.01,
            0.005,
        )
        .await;
        // Ngoài quý 2
        add_journal_px(
            &pool,
            "2026-07-01",
            "PX4",
            "P1",
            "PPHH",
            10.0,
            1_000_000.0,
            10_000_000.0,
            0.01,
            0.005,
        )
        .await;

        let out = build_tax_declaration(
            load_tax_agg_range(&pool, 2026, 4, 6).await.unwrap(),
            "revenue",
            2,
            &full("PPHH", 300_000_000.0),
            true,
            &std::collections::HashMap::new(),
        );
        let pphh = out.iter().find(|r| r.industry_code == "PPHH").unwrap();
        assert_eq!(pphh.revenue_up, 300_000_000.0);
        assert_eq!(pphh.vat_tax, 3_000_000.0); // 300tr x 1%
        assert_eq!(pphh.vat_payable, 3_000_000.0);

        // period_range: quý 2 → tháng 4..6
        assert_eq!(period_range("quarter", 2).unwrap(), (4, 6));
        assert_eq!(period_range("month", 3).unwrap(), (3, 3));
        assert_eq!(period_range("year", 0).unwrap(), (1, 12));
        assert!(period_range("quarter", 5).is_err());
    }

    // ── Sổ dựng đủ theo mẫu in — gọi thẳng builder với pool in-memory ──────
    //
    // Trước đây chỉ có test bộ cột, nên đầu sổ và dòng tổng cộng sai mẫu thì
    // không gì bắt được. Đây là các test dựng nguyên quyển sổ rồi soi từng dòng.

    /// Dựng 1 sổ (kỳ Quý 3/2026) và đọc lại dưới dạng JSON.
    async fn build_book(pool: &SqlitePool, book: &str) -> serde_json::Value {
        let raw = build_tax_book(pool, 2026, "quarter", 3, book)
            .await
            .unwrap_or_else(|e| panic!("dựng sổ {book} lỗi: {e}"));
        serde_json::from_str(&raw).expect("sổ là JSON hợp lệ")
    }

    /// Khóa 1 ô chữ của dòng sổ — ô không có key = mẫu để trắng.
    fn text_cell<'a>(row: &'a serde_json::Value, key: &str) -> &'a str {
        row["cells"].get(key).and_then(|v| v.as_str()).unwrap_or("")
    }

    /// Các cột của sổ theo đúng thứ tự in trên phụ lục.
    fn column_labels(book: &serde_json::Value) -> Vec<&str> {
        book["columns"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["label"].as_str().unwrap())
            .collect()
    }

    #[tokio::test]
    async fn s1a_ghi_dung_mau_tung_dong_va_dong_tong_cong() {
        let pool = test_pool().await;
        seed_product(&pool, "P1", "Hàng 1", 0.01).await;
        add_journal_px(
            &pool,
            "2026-07-05",
            "PX-01",
            "P1",
            "PPHH",
            1.0,
            100_000.0,
            100_000.0,
            0.01,
            0.005,
        )
        .await;
        add_journal_px(
            &pool,
            "2026-07-20",
            "PX-02",
            "P1",
            "PPHH",
            1.0,
            50_000.0,
            50_000.0,
            0.01,
            0.005,
        )
        .await;

        let book = build_book(&pool, "S1a").await;
        assert_eq!(book["title"], "SỔ DOANH THU BÁN HÀNG HÓA, DỊCH VỤ");
        assert!(
            book["header"]["form_ref"]
                .as_str()
                .unwrap()
                .starts_with("Mẫu số S1a-HKD"),
            "khối mẫu số phải đúng mẫu S1a"
        );
        assert_eq!(book["header"]["period"], "Quý 3/2026");
        assert_eq!(book["header"]["unit"], "VNĐ");
        // Mẫu S1a chỉ có 3 cột, không có cột số hiệu chứng từ.
        assert_eq!(
            column_labels(&book),
            ["A · Ngày tháng", "B · Diễn giải", "1 · Số tiền"]
        );

        let rows = book["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 3, "2 phiếu bán + 1 dòng tổng cộng");
        assert_eq!(text_cell(&rows[0], "a"), "05/07/2026");
        assert_eq!(text_cell(&rows[0], "b"), "PX-01 — test");
        assert_eq!(rows[0]["cells"]["1"].as_f64(), Some(100_000.0));
        assert_eq!(text_cell(&rows[1], "a"), "20/07/2026");
        // Dòng cuối đúng chữ "Tổng cộng" ở cột B và cộng đúng cả kỳ.
        assert_eq!(rows[2]["kind"], "total");
        assert_eq!(text_cell(&rows[2], "b"), "Tổng cộng");
        assert_eq!(rows[2]["cells"]["1"].as_f64(), Some(150_000.0));
    }

    #[tokio::test]
    async fn s3a_du_12_cot_thue_va_khong_bia_so_khi_chua_phat_sinh() {
        let pool = test_pool().await;
        let book = build_book(&pool, "S3a").await;
        assert_eq!(book["title"], "SỔ THEO DÕI NGHĨA VỤ THUẾ KHÁC");
        assert!(book["header"]["form_ref"]
            .as_str()
            .unwrap()
            .starts_with("Mẫu số S3a-HKD"));
        assert_eq!(book["header"]["location_label"], "Địa điểm kinh doanh");
        // Đúng 12 cột A + B + 1..10 và đúng văn bản từng cột trên phụ lục.
        assert_eq!(
            column_labels(&book),
            [
                "A · Ngày tháng ghi sổ",
                "B · Diễn giải",
                "1 · Lượng HH, DV chịu thuế",
                "2 · Mức thuế tuyệt đối",
                "3 · Giá tính thuế / 01 đơn vị",
                "4 · Thuế suất",
                "5 · TSX, TNK, TTĐB theo tỷ lệ %",
                "6 · TSX, TNK, TTĐB tuyệt đối",
                "7 · TSX, TNK, TTĐB phải nộp",
                "8 · Thuế bảo vệ môi trường",
                "9 · Thuế tài nguyên",
                "10 · Thuế sử dụng đất",
            ]
        );
        // App chưa lưu giao dịch thuế khác → không có dòng số liệu nào, không bịa.
        assert!(book["rows"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn dau_so_khong_bao_gio_in_dong_dia_diem_kinh_doanh_trong() {
        let pool = test_pool().await;
        // Chưa khai địa điểm → lấy địa chỉ đăng ký làm địa điểm kinh doanh.
        sqlx::query("UPDATE business SET address = '12 Đường A, TP B' WHERE id = 1")
            .execute(&pool)
            .await
            .unwrap();
        let book = build_book(&pool, "S1a").await;
        assert_eq!(book["header"]["location_label"], "Địa điểm kinh doanh");
        assert_eq!(book["header"]["location"], "12 Đường A, TP B");

        // Hộ bán ở chợ → khai riêng thì đúng chỗ bán mới được ghi lên sổ.
        sqlx::query("UPDATE business SET location = 'Chợ Bến Thành' WHERE id = 1")
            .execute(&pool)
            .await
            .unwrap();
        let book = build_book(&pool, "S2a").await;
        assert_eq!(book["header"]["location"], "Chợ Bến Thành");

        // S2c đổi nhãn theo mẫu; S2e không có dòng này; S2d không có dòng
        // "Đơn vị tính:" ở đầu sổ (đơn vị nằm trong cột D từng dòng).
        let book = build_book(&pool, "S2c").await;
        assert_eq!(book["header"]["location_label"], "Tên địa điểm kinh doanh");
        let book = build_book(&pool, "S2e").await;
        assert_eq!(book["header"]["location_label"], "");
        assert_eq!(book["header"]["location"], "");
        let book = build_book(&pool, "S2d").await;
        assert_eq!(book["header"]["unit"], "");
    }

    #[tokio::test]
    async fn s2b_chi_ghi_thue_gtgt_theo_mau_con_s2a_moi_co_thue_tncn() {
        let pool = test_pool().await;
        sqlx::query("UPDATE business SET tax_group = 2")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO app_setting(key, value) VALUES('tax_method', 'revenue') \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .execute(&pool)
        .await
        .unwrap();
        seed_product(&pool, "P1", "Hàng 1", 0.01).await;
        add_journal_px(
            &pool,
            "2026-07-05",
            "PX-01",
            "P1",
            "PPHH",
            1.0,
            1_000_000.0,
            1_000_000.0,
            0.01,
            0.005,
        )
        .await;

        // Mẫu S2b: mỗi khối ngành chỉ có "Thuế GTGT", tổng cuối chỉ 1 dòng
        // "Tổng số thuế GTGT phải nộp" — không có số thuế TNCN ở sổ này.
        let s2b = build_book(&pool, "S2b").await;
        let labels: Vec<String> = s2b["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| text_cell(r, "c").to_string())
            .collect();
        assert!(
            labels.iter().all(|l| !l.contains("TNCN")),
            "S2b không được ghi thuế TNCN: {labels:?}"
        );
        assert_eq!(labels.last().unwrap(), "Tổng số thuế GTGT phải nộp");

        // Cùng dữ liệu nhưng mở mẫu S2a (hộ nhóm 2 nộp theo % doanh thu) thì
        // phải có đủ "Thuế TNCN" trong khối ngành và dòng tổng cuối.
        let s2a = build_book(&pool, "S2a").await;
        let labels: Vec<String> = s2a["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| text_cell(r, "c").to_string())
            .collect();
        assert!(labels.iter().any(|l| l == "Thuế TNCN"), "{labels:?}");
        assert_eq!(labels.last().unwrap(), "Tổng số thuế TNCN phải nộp");
    }

    // ── S2a: mỗi ngành một khối, thuế tính theo tỷ lệ % của đúng ngành đó ────
    //
    // Hai ngành thường gặp của hộ kinh doanh: phân phối, cung cấp hàng hóa
    // (GTGT 1% – TNCN 0,5%) và dịch vụ, xây dựng có bao thầu vật liệu
    // (GTGT 3% – TNCN 1,5%). Sổ phải mở riêng từng khối, gom đúng chứng từ của
    // ngành vào đúng khối và tính thuế theo tỷ lệ của ngành đó — gộp hai ngành
    // vào một chỗ, hoặc lấy tỷ lệ của ngành khác, là sai mẫu.

    /// Hồ sơ hộ nhóm 2 nộp TNCN theo % doanh thu — dạng thì mở được mẫu S2a.
    async fn ho_nhom_2_theo_doanh_thu(pool: &SqlitePool) {
        sqlx::query("UPDATE business SET tax_group = 2")
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO app_setting(key, value) VALUES('tax_method', 'revenue') \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .execute(pool)
        .await
        .unwrap();
    }

    /// (kind, số hiệu, diễn giải, số tiền) của từng dòng sổ — soi đúng thứ tự in.
    fn row_lines(book: &serde_json::Value) -> Vec<(&str, &str, &str, f64)> {
        book["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                (
                    r["kind"].as_str().unwrap_or(""),
                    text_cell(r, "a"),
                    text_cell(r, "c"),
                    r["cells"]["1"].as_f64().unwrap_or(0.0),
                )
            })
            .collect()
    }

    /// Số tiền của dòng có diễn giải đúng bằng `label`.
    ///
    /// Chỉ dùng cho sổ một khối ngành (mỗi nhãn "Thuế GTGT"/"Thuế TNCN" mới
    /// xuất hiện một lần); sổ nhiều ngành thì phải soi theo thứ tự `row_lines`.
    fn line_money(book: &serde_json::Value, label: &str) -> f64 {
        row_lines(book)
            .into_iter()
            .find(|(_, _, c, _)| *c == label)
            .unwrap_or_else(|| panic!("sổ thiếu dòng {label:?}"))
            .3
    }

    #[tokio::test]
    async fn s2a_mo_tung_khoi_nganh_va_tinh_thue_theo_ty_le_cua_nganh_do() {
        let pool = test_pool().await;
        ho_nhom_2_theo_doanh_thu(&pool).await;
        seed_product(&pool, "P1", "Hàng 1", 0.01).await;
        // Phân phối, cung cấp hàng hóa: 1% GTGT + 0,5% TNCN. Hai phiếu cùng
        // ngành để kiểm tra cộng dồn bên trong một khối.
        add_journal_px(
            &pool,
            "2026-07-05",
            "PX-01",
            "P1",
            "PPHH",
            1.0,
            600_000.0,
            600_000.0,
            0.01,
            0.005,
        )
        .await;
        add_journal_px(
            &pool,
            "2026-07-06",
            "PX-02",
            "P1",
            "PPHH",
            1.0,
            400_000.0,
            400_000.0,
            0.01,
            0.005,
        )
        .await;
        // Dịch vụ, xây dựng có bao thầu vật liệu: 3% GTGT + 1,5% TNCN.
        add_journal_px(
            &pool,
            "2026-07-10",
            "PX-03",
            "P1",
            "DVXD-KL",
            1.0,
            2_000_000.0,
            2_000_000.0,
            0.03,
            0.015,
        )
        .await;

        let book = build_book(&pool, "S2a").await;
        assert_eq!(
            row_lines(&book),
            vec![
                (
                    "section",
                    "",
                    "1. Ngành nghề Phân phối, cung cấp hàng hóa (PPHH)",
                    0.0
                ),
                ("detail", "PX-01", "test", 600_000.0),
                ("detail", "PX-02", "test", 400_000.0),
                ("subtotal", "", "Tổng cộng (1)", 1_000_000.0),
                ("subtotal", "", "Thuế GTGT", 10_000.0), // 1.000.000 × 1%
                ("subtotal", "", "Thuế TNCN", 5_000.0),  // 1.000.000 × 0,5%
                (
                    "section",
                    "",
                    "2. Ngành nghề Dịch vụ, xây dựng có bao thầu NVL (DVXD-KL)",
                    0.0
                ),
                ("detail", "PX-03", "test", 2_000_000.0),
                ("subtotal", "", "Tổng cộng (2)", 2_000_000.0),
                ("subtotal", "", "Thuế GTGT", 60_000.0), // 2.000.000 × 3%
                ("subtotal", "", "Thuế TNCN", 30_000.0), // 2.000.000 × 1,5%
                ("total", "", "Tổng số thuế GTGT phải nộp", 70_000.0),
                ("total", "", "Tổng số thuế TNCN phải nộp", 35_000.0),
            ],
        );
    }

    #[tokio::test]
    async fn s2a_lay_ty_le_thu_tu_bang_nganh_neu_doi_ty_le_so_doi_theo() {
        let pool = test_pool().await;
        ho_nhom_2_theo_doanh_thu(&pool).await;
        seed_product(&pool, "P1", "Hàng 1", 0.01).await;
        add_journal_px(
            &pool,
            "2026-07-05",
            "PX-01",
            "P1",
            "PPHH",
            1.0,
            1_000_000.0,
            1_000_000.0,
            0.01,
            0.005,
        )
        .await;

        // Tỷ lệ PPHH đang là 1% → sổ ghi 10.000 đ.
        let book = build_book(&pool, "S2a").await;
        assert_eq!(line_money(&book, "Thuế GTGT"), 10_000.0);

        // Hộ sửa tỷ lệ ở màn Danh mục (1% → 2%) thì số thuế đổi theo — tỷ lệ
        // lấy từ bảng `industry_group`, không nằm cứng trong builder.
        sqlx::query("UPDATE industry_group SET vat_rate = 0.02 WHERE code = 'PPHH'")
            .execute(&pool)
            .await
            .unwrap();
        let book = build_book(&pool, "S2a").await;
        assert_eq!(line_money(&book, "Thuế GTGT"), 20_000.0);
        // Tỷ lệ TNCN không đụng tới thì số thuế TNCN giữ nguyên.
        assert_eq!(line_money(&book, "Thuế TNCN"), 5_000.0);
    }

    /// Một hóa đơn (một phiếu PX) bán trộn mặt hàng 1,5% và mặt hàng 7%:
    /// sổ phải tách đúng hai khối ngành, mỗi khối chỉ nhận dòng tiền của mặt
    /// hàng thuộc ngành đó và thuế tính theo tỷ lệ riêng của từng ngành —
    /// gộp hai dòng vào một khối hay áp chung một tỷ lệ là sai.
    #[tokio::test]
    async fn s2a_hoa_don_tron_hai_ty_le_khac_mo_dung_hai_khoi_nganh() {
        let pool = test_pool().await;
        ho_nhom_2_theo_doanh_thu(&pool).await;
        // Hai tỷ lệ này không có trong danh mục seed — hộ thêm được ở màn
        // Danh mục; test chèn thẳng để xác nhận sổ lấy tỷ lệ từ bảng ngành.
        sqlx::query(
            "INSERT INTO industry_group (code, name, vat_rate, pit_rate) VALUES
                ('TYLE-15', 'Mặt hàng thuế suất 1,5%', 0.015, 0.01),
                ('TYLE-7',  'Mặt hàng thuế suất 7%',   0.07,  0.02)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let h1 = seed_product(&pool, "H1", "Mặt hàng 1,5%", 0.015).await;
        let h2 = seed_product(&pool, "H2", "Mặt hàng 7%", 0.07).await;
        let w: i64 =
            sqlx::query_scalar!(r#"SELECT id as "id!" FROM warehouse WHERE code = 'KHO-CHINH'"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        add_stock_lot(&pool, h1, w, 5.0, 100_000.0, "2026-01-01").await;
        add_stock_lot(&pool, h2, w, 5.0, 100_000.0, "2026-01-01").await;

        // Một phiếu xuất duy nhất gộp hai mặt hàng thuộc hai nhóm ngành khác
        // tỷ lệ — đúng đường người dùng tạo hóa đơn trộn.
        let items = [
            OutboundItemInput {
                product_code: "H1".into(),
                quantity: 1.0,
                unit_price: 2_000_000.0,
                discount: 0.0,
                industry_code: "TYLE-15".into(),
                warehouse_code: "".into(),

                line_name: String::new(),
            },
            OutboundItemInput {
                product_code: "H2".into(),
                quantity: 1.0,
                unit_price: 1_000_000.0,
                discount: 0.0,
                industry_code: "TYLE-7".into(),
                warehouse_code: "".into(),

                line_name: String::new(),
            },
        ];
        let r = save_outbound_core(
            &pool,
            "2026-07-15",
            "PX-TRON",
            "Bán trộn hai tỷ lệ",
            "",
            "HKD",
            &items,
            "",
            false,
            "sale",
            "",
            &OutboundInvoiceInput::default(),
        )
        .await
        .expect("xuất kho");
        // Hai mặt hàng → hai dòng doanh thu, mỗi dòng một nhóm ngành.
        assert_eq!(r.entries, 2);
        assert_eq!(r.revenue, 3_000_000.0);
        let px_lines: i64 = sqlx::query_scalar!(
            r#"SELECT COUNT(*) AS "n!" FROM journal_entry
                WHERE voucher_no = 'PX-TRON' AND entry_type = 'PX'"#
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(px_lines, 2);

        let book = build_book(&pool, "S2a").await;
        assert_eq!(
            row_lines(&book),
            vec![
                (
                    "section",
                    "",
                    "1. Ngành nghề Mặt hàng thuế suất 1,5% (TYLE-15)",
                    0.0
                ),
                ("detail", "PX-TRON", "Bán trộn hai tỷ lệ", 2_000_000.0),
                ("subtotal", "", "Tổng cộng (1)", 2_000_000.0),
                ("subtotal", "", "Thuế GTGT", 30_000.0), // 2.000.000 × 1,5%
                ("subtotal", "", "Thuế TNCN", 20_000.0), // 2.000.000 × 1%
                (
                    "section",
                    "",
                    "2. Ngành nghề Mặt hàng thuế suất 7% (TYLE-7)",
                    0.0
                ),
                ("detail", "PX-TRON", "Bán trộn hai tỷ lệ", 1_000_000.0),
                ("subtotal", "", "Tổng cộng (2)", 1_000_000.0),
                ("subtotal", "", "Thuế GTGT", 70_000.0), // 1.000.000 × 7%
                ("subtotal", "", "Thuế TNCN", 20_000.0), // 1.000.000 × 2%
                ("total", "", "Tổng số thuế GTGT phải nộp", 100_000.0),
                ("total", "", "Tổng số thuế TNCN phải nộp", 40_000.0),
            ],
        );
    }

    /// Phiếu ghi giá 0 không phát sinh doanh thu: không được mở khối ngành
    /// trống trong sổ. S2a từng hiện khối "Cho thuê tài sản…" chỉ toàn dòng 0
    /// đồng dù cả kỳ không bán gì thuộc ngành đó.
    #[tokio::test]
    async fn s2a_bo_qua_phieu_tien_0_khong_mo_khoi_nganh_trong() {
        let pool = test_pool().await;
        ho_nhom_2_theo_doanh_thu(&pool).await;
        seed_product(&pool, "P1", "Hàng 1", 0.01).await;
        add_journal_px(
            &pool,
            "2026-07-05",
            "PX-0",
            "P1",
            "DV-TS",
            1.0,
            0.0,
            0.0,
            0.05,
            0.05,
        )
        .await;
        add_journal_px(
            &pool,
            "2026-07-06",
            "PX-1",
            "P1",
            "PPHH",
            1.0,
            1_000_000.0,
            1_000_000.0,
            0.01,
            0.005,
        )
        .await;

        // S2a chỉ mở khối PPHH — khối DV-TS (phiếu giá 0) biến mất.
        let book = build_book(&pool, "S2a").await;
        assert_eq!(
            row_lines(&book),
            vec![
                (
                    "section",
                    "",
                    "1. Ngành nghề Phân phối, cung cấp hàng hóa (PPHH)",
                    0.0
                ),
                ("detail", "PX-1", "test", 1_000_000.0),
                ("subtotal", "", "Tổng cộng (1)", 1_000_000.0),
                ("subtotal", "", "Thuế GTGT", 10_000.0),
                ("subtotal", "", "Thuế TNCN", 5_000.0),
                ("total", "", "Tổng số thuế GTGT phải nộp", 10_000.0),
                ("total", "", "Tổng số thuế TNCN phải nộp", 5_000.0),
            ],
        );

        // S1a (sổ một cột số tiền) cũng không ghi dòng 0 đồng: còn 1 dòng bán
        // + 1 dòng tổng cộng.
        let s1a = build_book(&pool, "S1a").await;
        assert_eq!(
            s1a["rows"].as_array().unwrap().len(),
            2,
            "phiếu 0 đồng không lên sổ S1a"
        );
    }

    #[test]
    fn danh_sach_so_ap_dung_theo_nhom_ho_va_phuong_phap_thue() {
        // Nhóm 1 (miễn thuế) → chỉ sổ doanh thu + sổ thuế khác.
        assert_eq!(applicable_books(1, "revenue"), ["S1a", "S3a"]);
        assert_eq!(applicable_books(1, "profit"), ["S1a", "S3a"]);
        // Nhóm 2 nộp TNCN theo tỷ lệ % doanh thu → S2a.
        assert_eq!(applicable_books(2, "revenue"), ["S2a", "S3a"]);
        // Nhóm 2 nộp TNCN theo thu nhập → đủ 4 sổ chi tiết.
        assert_eq!(
            applicable_books(2, "profit"),
            ["S2b", "S2c", "S2d", "S2e", "S3a"]
        );
        // Nhóm 3/4 luôn tính TNCN theo lợi nhuận, kể cả khi cấu hình còn để
        // "theo doanh thu" — trùng với `by_revenue` của tờ khai.
        assert_eq!(
            applicable_books(3, "revenue"),
            ["S2b", "S2c", "S2d", "S2e", "S3a"]
        );
        assert_eq!(
            applicable_books(4, "revenue"),
            ["S2b", "S2c", "S2d", "S2e", "S3a"]
        );
        // S3a có mặt ở mọi hồ sơ: app chưa theo dõi thuế khác nên không ẩn được.
        for (group, method) in [(1, "revenue"), (2, "revenue"), (3, "profit")] {
            assert!(applicable_books(group, method).contains(&"S3a"));
        }
    }

    #[test]
    fn ngay_ghi_so_dang_dd_mm_yyyy_theo_mau_in() {
        assert_eq!(vn_date("2026-07-05"), "05/07/2026");
        assert_eq!(vn_date("2026-12-31"), "31/12/2026");
        // Không phải ngày ISO thì giữ nguyên — không bịa ngày cho chứng từ.
        assert_eq!(vn_date(""), "");
        assert_eq!(vn_date("05/07/2026"), "05/07/2026");
    }

    // ─── F4 — PHIẾU XUẤT NVL TỰ SINH KHÔNG PHẢI DOANH THU / GIÁ VỐN ───

    /// Chèn 1 dòng sổ đầy đủ (PN / PX) với mã hàng, số lượng, giá, adjust_code.
    #[allow(clippy::too_many_arguments)]
    async fn add_row(
        pool: &SqlitePool,
        date: &str,
        voucher: &str,
        kind: &str,
        code: &str,
        qty: f64,
        unit_price: f64,
        industry: &str,
        adjust: &str,
    ) {
        let amount = round2(qty * unit_price);
        sqlx::query!(
            "INSERT INTO journal_entry
             (posting_date, voucher_no, doc_date, entry_type, description, product_code,
              quantity, unit_price, amount, debit_account, credit_account,
              industry_code, vat_rate, pit_rate, unit_code, adjust_code, note)
             VALUES (?, ?, ?, ?, 'test', ?, ?, ?, ?, '152', '331', ?, 0.0, 0.0, 'HKD', ?, '')",
            date,
            voucher,
            date,
            kind,
            code,
            qty,
            unit_price,
            amount,
            industry,
            adjust
        )
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn xuat_nvl_tu_sinh_khong_vao_doanh_thu_va_gia_von() {
        let pool = test_pool().await;
        seed_product(&pool, "XN1", "Vật tư", 0.01).await;

        // Nhập 20 @ 6.000 (đầu vào FIFO).
        add_row(
            &pool,
            "2026-03-01",
            "PN-1",
            "PN",
            "XN1",
            20.0,
            6_000.0,
            "",
            "",
        )
        .await;
        // Bán 10 @ 10.000 → doanh thu thật 100.000 (đặt trước để tiêu lô trước).
        add_row(
            &pool,
            "2026-03-10",
            "PX-BAN",
            "PX",
            "XN1",
            10.0,
            10_000.0,
            "PPHH",
            "",
        )
        .await;
        // Xuất NVL tự sinh theo định mức 5 @ 5.000 → KHÔNG phải doanh thu.
        add_row(
            &pool,
            "2026-03-11",
            "PX001",
            "PX",
            "XN1",
            5.0,
            5_000.0,
            "",
            "XuatNVL",
        )
        .await;

        // Sổ bán hàng: chỉ phiếu bán thường.
        let rows = sales_rows(&pool, "2026-03-01", "2026-03-31").await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].voucher_no, "PX-BAN");

        // Sổ doanh thu / chi phí: chỉ 100.000 doanh thu.
        let v = get_revenue_expense_core(&pool, "2026-03-01", "2026-03-31")
            .await
            .unwrap();
        assert_eq!(v["revenue_up"].as_f64().unwrap(), 100_000.0);

        // Giá vốn FIFO kỳ: chỉ bán 10 x 6.000 = 60.000 (không cộng dòng XuatNVL).
        let cogs = load_fifo_cogs(&pool, "2026-03-01", "2026-03-31")
            .await
            .unwrap();
        assert_eq!(cogs, 60_000.0);

        // Kế hoạch ghi bổ sung giá vốn: dòng XuatNVL đã có Nợ 154 / Có 152 rồi
        // → không được đề nghị ghi thêm Nợ 632 / Có 152 (credit 152 lần 2).
        let (receipts, issues) = load_fifo_rows(&pool).await.unwrap();
        assert_eq!(issues.len(), 1, "chỉ lượt xuất bán hàng cần giá vốn");
        let costs = issue_costs(&receipts, &issues);
        let plan = cogs_backfill_plan(&issues, &costs, &Default::default());
        assert_eq!(plan, vec![0]);
    }
}

// ─── TEST CÁC QUY TẮC THUẾ (Điều 3–10 NĐ 68/2026, Điều 7 Luật TNCN 109/2025) ───
#[cfg(test)]
mod tax_rules_tests {
    use super::*;

    /// Doanh thu tính thuế của 1 nhóm ngành (map theo mã).
    fn full(code: &str, value: f64) -> std::collections::HashMap<String, f64> {
        [(code.to_string(), value)].into_iter().collect()
    }

    fn row(code: &str, up: f64, down: f64, pit_rate: f64) -> TaxAgg {
        TaxAgg {
            industry_code: code.into(),
            industry_name: code.into(),
            vat_rate: 0.01,
            pit_rate,
            revenue_up: up,
            revenue_down: down,
        }
    }

    #[test]
    fn xep_nhom_theo_doanh_thu_nam_dung_moc_luat() {
        // Điều 7 khoản 2 Luật TNCN: 15% / 17% / 20% theo các mốc 3 tỷ, 50 tỷ.
        assert_eq!(tax_group_with(0.0, &TaxThresholds::default()), 1);
        assert_eq!(
            tax_group_with(THRESHOLD_EXEMPT, &TaxThresholds::default()),
            1
        );
        assert_eq!(
            tax_group_with(THRESHOLD_EXEMPT + 1.0, &TaxThresholds::default()),
            2
        );
        assert_eq!(
            tax_group_with(THRESHOLD_GROUP3, &TaxThresholds::default()),
            2
        );
        assert_eq!(
            tax_group_with(THRESHOLD_GROUP3 + 1.0, &TaxThresholds::default()),
            3
        );
        assert_eq!(
            tax_group_with(THRESHOLD_GROUP4, &TaxThresholds::default()),
            3
        );
        assert_eq!(
            tax_group_with(THRESHOLD_GROUP4 + 1.0, &TaxThresholds::default()),
            4
        );
        assert_eq!(profit_tncn_rate(2), 0.15);
        assert_eq!(profit_tncn_rate(3), 0.17);
        assert_eq!(profit_tncn_rate(4), 0.20);
    }

    #[test]
    fn tru_nguong_1_ty_theo_tung_nhom_nghanh() {
        // Luật TNCN Điều 7 khoản 3 điểm a: doanh thu tính thuế = phần DT VƯỢT
        // trên mức 01 tỷ. DT năm 1,5 tỷ ở một nhóm → chỉ 0,5 tỷ chịu thuế.
        let rows = vec![row("PPHH", 1_500_000_000.0, 0.0, 0.005)];
        let alloc = exempt_alloc(&rows, THRESHOLD_EXEMPT, &TaxableExemptChoice::Auto);
        assert_eq!(alloc, vec![("PPHH".to_string(), 1_000_000_000.0)]);
        // Cả năm là 1 kỳ → lũy kế trước kỳ = 0, mức trừ dùng đúng một lần.
        let taxable = taxable_by_period(
            &Default::default(),
            &rows
                .iter()
                .map(|r| (r.industry_code.clone(), net_revenue(r)))
                .collect(),
            &alloc,
        );
        assert_eq!(taxable[0].1, 500_000_000.0);
        // 0,5 tỷ × 0,5% = 2,5 triệu (không phải 1,5 tỷ × 0,5% = 7,5 triệu).
        assert_eq!(round2(taxable[0].1 * 0.005), 2_500_000.0);
    }

    #[test]
    fn ranh_gioc_1_ty_dung_theo_nguong_khong_phai_nop_thue() {
        // Đúng 01 tỷ → khoản 1 Điều 7 Luật TNCN 109/2025: "từ mức 01 tỷ trở xuống
        // không phải nộp" → hộ thuộc NHÓM 1, doanh thu tính thuế = 0, TNCN = 0.
        let rows = vec![row("PPHH", THRESHOLD_EXEMPT, 0.0, 0.005)];
        assert_eq!(
            tax_group_with(year_revenue(&rows), &TaxThresholds::default()),
            1
        );
        let alloc = exempt_alloc(&rows, THRESHOLD_EXEMPT, &TaxableExemptChoice::Auto);
        let taxable = taxable_by_period(
            &Default::default(),
            &rows
                .iter()
                .map(|r| (r.industry_code.clone(), net_revenue(r)))
                .collect(),
            &alloc,
        );
        assert_eq!(
            taxable[0].1, 0.0,
            "đúng ngưỡng thì doanh thu tính thuế bằng 0"
        );

        // 01 tỷ + 1 đồng → vượt ngưỡng: thuộc nhóm 2 và CHỈ 1 đồng đó chịu thuế
        // (điểm a khoản 3 Điều 7: doanh thu tính thuế = phần vượt trên mức khoản 1).
        let rows2 = vec![row("PPHH", THRESHOLD_EXEMPT + 1.0, 0.0, 0.005)];
        assert_eq!(
            tax_group_with(year_revenue(&rows2), &TaxThresholds::default()),
            2
        );
        let alloc2 = exempt_alloc(&rows2, THRESHOLD_EXEMPT, &TaxableExemptChoice::Auto);
        let taxable2 = taxable_by_period(
            &Default::default(),
            &rows2
                .iter()
                .map(|r| (r.industry_code.clone(), net_revenue(r)))
                .collect(),
            &alloc2,
        );
        assert_eq!(taxable2[0].1, 1.0, "chỉ phần vượt ngưỡng mới chịu thuế");
    }

    #[test]
    fn ky_truoc_khi_vuot_nguong_thi_chua_phat_sinh_thue() {
        // Điều 8 khoản 1a: khai, nộp thuế KỂ TỪ kỳ phát sinh doanh thu vượt ngưỡng.
        // Quý 1–2 doanh thu lũy kế chưa vượt 01 tỷ → chưa phát sinh thuế.
        assert!(!period_is_taxable(
            0.0,
            400_000_000.0,
            THRESHOLD_EXEMPT,
            false
        ));
        assert!(!period_is_taxable(
            400_000_000.0,
            550_000_000.0,
            THRESHOLD_EXEMPT,
            false
        ));
        // Quý 3 lũy kế 1,1 tỷ → có nghĩa vụ; cả GTGT lẫn TNCN tính trên quý này.
        assert!(period_is_taxable(
            950_000_000.0,
            150_000_000.0,
            THRESHOLD_EXEMPT,
            false
        ));

        // Hồ sơ chốt Nhóm 2/3/4 → thuộc diện nộp thuế suốt năm: kỳ nào có doanh
        // thu là phải nộp, không chờ vượt ngưỡng.
        assert!(period_is_taxable(
            0.0,
            400_000_000.0,
            THRESHOLD_EXEMPT,
            true
        ));
        assert!(period_is_taxable(0.0, 1.0, THRESHOLD_EXEMPT, true));
        assert!(
            !period_is_taxable(0.0, 0.0, THRESHOLD_EXEMPT, true),
            "kỳ không có DT"
        );

        // Hệ quả trên tờ khai: hộ nhóm 2 (DT cả năm > 1 tỷ) nhưng kỳ trước khi
        // vượt ngưỡng thì cột thuế vẫn bằng 0.
        let pre = build_tax_declaration(
            vec![row("PPHH", 400_000_000.0, 0.0, 0.005)],
            "revenue",
            2,
            &full("PPHH", 0.0),
            false,
            &std::collections::HashMap::new(),
        );
        assert_eq!(pre[0].vat_tax, 0.0);
        assert_eq!(pre[0].pit_tax, 0.0);
        // Cùng dữ liệu đó nhưng kỳ đã vượt ngưỡng → có thuế.
        let post = build_tax_declaration(
            vec![row("PPHH", 400_000_000.0, 0.0, 0.005)],
            "revenue",
            2,
            &full("PPHH", 400_000_000.0),
            true,
            &std::collections::HashMap::new(),
        );
        assert_eq!(post[0].vat_tax, 4_000_000.0); // 400tr x 1%
        assert_eq!(post[0].pit_tax, 2_000_000.0); // 400tr x 0,5%
    }

    #[test]
    fn muc_tru_chi_dung_mot_lan_trong_ca_nam() {
        // 1,1 tỷ trải đều 4 quý: các quý trước bằng 0, quý cuối gánh hết phần
        // vượt ngưỡng; tổng 4 quý đúng bằng 100 triệu (không trừ lặp/thiếu).
        let alloc = vec![("PPHH".to_string(), THRESHOLD_EXEMPT)];
        let per_quarter = 1_100_000_000.0 / 4.0;
        let mut cum = std::collections::HashMap::new();
        let mut total = 0.0;
        for q in 0..4 {
            let period = [("PPHH".to_string(), per_quarter)].into_iter().collect();
            let taxable = taxable_by_period(&cum, &period, &alloc);
            total += taxable[0].1;
            cum.insert("PPHH".to_string(), per_quarter * (q as f64 + 1.0));
        }
        assert_eq!(round2(total), 100_000_000.0);
    }

    #[test]
    fn chia_nguong_theo_ty_le_doanh_thu_khi_ho_chua_chot() {
        let rows = vec![
            row("PPHH", 600_000_000.0, 0.0, 0.005),
            row("DVXD-KNL", 400_000_000.0, 0.0, 0.02),
        ];
        let alloc = exempt_alloc(&rows, THRESHOLD_EXEMPT, &TaxableExemptChoice::Auto);
        // 1 tỷ chia theo 60% / 40%
        assert_eq!(alloc[0].1, 600_000_000.0);
        assert_eq!(alloc[1].1, 400_000_000.0);
    }

    #[test]
    fn ho_duoc_tuon_chon_nhom_nganh_co_loi_nhat() {
        // NĐ 68/2026 Điều 4 khoản 3: hộ tự chọn nhóm nào áp mức trừ theo
        // phương án có lợi nhất. Trừ hết cho nhóm tỷ lệ thấp (PPHH 0,5%).
        let rows = vec![
            row("PPHH", 600_000_000.0, 0.0, 0.005),
            row("DVXD-KNL", 400_000_000.0, 0.0, 0.02),
        ];
        let mut map = std::collections::HashMap::new();
        map.insert("PPHH".to_string(), 600_000_000.0);
        map.insert("DVXD-KNL".to_string(), 400_000_000.0);
        let alloc = exempt_alloc(&rows, THRESHOLD_EXEMPT, &TaxableExemptChoice::Manual(map));
        let taxable = taxable_by_period(
            &Default::default(),
            &rows
                .iter()
                .map(|r| (r.industry_code.clone(), net_revenue(r)))
                .collect(),
            &alloc,
        );
        // 2 nhóm đều được trừ hết → phần chịu thuế bằng 0.
        assert_eq!(taxable[0].1, 0.0);
        assert_eq!(taxable[1].1, 0.0);
        // JSON hỏng / rỗng → coi như chưa chốt, không làm hỏng tờ khai.
        assert_eq!(
            TaxableExemptChoice::from_json(""),
            TaxableExemptChoice::Auto
        );
        assert_eq!(
            TaxableExemptChoice::from_json("{oops"),
            TaxableExemptChoice::Auto
        );
        assert!(matches!(
            TaxableExemptChoice::from_json(r#"{"PPHH": 600000000}"#),
            TaxableExemptChoice::Manual(_)
        ));
    }

    #[test]
    fn khong_tru_nguong_khi_doanh_thu_nam_duoi_nguong() {
        // DT 800 triệu < ngưỡng → mức trừ bằng chính DT, không phát sinh DT âm.
        let rows = vec![row("PPHH", 800_000_000.0, 0.0, 0.005)];
        let alloc = exempt_alloc(&rows, THRESHOLD_EXEMPT, &TaxableExemptChoice::Auto);
        assert_eq!(alloc[0].1, 800_000_000.0);
        let taxable = taxable_by_period(
            &Default::default(),
            &rows
                .iter()
                .map(|r| (r.industry_code.clone(), net_revenue(r)))
                .collect(),
            &alloc,
        );
        assert_eq!(taxable[0].1, 0.0);
    }

    #[test]
    fn vuot_nguong_giua_nam_thi_tinh_tu_ky_phat_sinh() {
        // Quý: lũy kế tháng 1-3 = 400tr, tháng 4-6 = 1,1 tỷ (vượt 1 tỷ) →
        // kỳ tính thuế đầu tiên là quý 2 (tháng 4..6).
        let mut cum = [0.0; 13];
        cum[3] = 400_000_000.0;
        cum[6] = 1_100_000_000.0;
        assert_eq!(
            first_taxable_period(&cum, THRESHOLD_EXEMPT, 3),
            Some((4, 6))
        );
        // Cả năm không vượt ngưỡng → chỉ thông báo doanh thu, không có kỳ tính thuế.
        let mut small = [0.0; 13];
        small[12] = 900_000_000.0;
        assert_eq!(first_taxable_period(&small, THRESHOLD_EXEMPT, 3), None);
        // Khai theo tháng: vượt ngưỡng ở tháng 5 → kỳ đầu tiên là tháng 5.
        let mut m = [0.0; 13];
        m[5] = 1_050_000_000.0;
        assert_eq!(first_taxable_period(&m, THRESHOLD_EXEMPT, 1), Some((5, 5)));
    }

    #[test]
    fn han_nop_dung_quy_dinh() {
        // Theo quý: chậm nhất ngày cuối tháng đầu quý tiếp theo.
        assert_eq!(
            tax_deadline("quarter", 1, 2026).as_deref(),
            Some("30/04/2026")
        );
        assert_eq!(
            tax_deadline("quarter", 3, 2026).as_deref(),
            Some("31/10/2026")
        );
        assert_eq!(
            tax_deadline("quarter", 4, 2026).as_deref(),
            Some("31/01/2027")
        );
        // Theo tháng: ngày 20 tháng tiếp theo.
        assert_eq!(
            tax_deadline("month", 9, 2026).as_deref(),
            Some("20/10/2026")
        );
        assert_eq!(
            tax_deadline("month", 12, 2026).as_deref(),
            Some("20/01/2027")
        );
        // Nhóm 1: thông báo doanh thu chậm nhất 31/01 năm sau.
        assert_eq!(tax_deadline("year", 0, 2026).as_deref(), Some("31/01/2027"));
        // Số kỳ sai → không bịa ra hạn.
        assert_eq!(tax_deadline("quarter", 9, 2026), None);
        assert_eq!(tax_deadline("month", 13, 2026), None);
        assert_eq!(tax_deadline("abc", 1, 2026), None);
    }

    #[test]
    fn gia_von_fifo_khop_luc_lap_phieu_xuat() {
        // Nhập 10 giá 100.000 rồi 5 giá 200.000; xuất 12 giá → 10×100k + 2×200k.
        let receipts = vec![
            ("SP1".to_string(), 10.0, 100_000.0),
            ("SP1".to_string(), 5.0, 200_000.0),
        ];
        let issues = vec![("SP1".to_string(), 12.0)];
        assert_eq!(fifo_cogs(&receipts, &issues), 1_400_000.0);
        // Lần xuất sau lấy nốt 3 giá còn lại của lô giá 200.000.
        assert_eq!(fifo_cogs(&receipts, &[("SP1".into(), 15.0)]), 2_000_000.0);
        // Nhiều mã hàng không lẫn lộn nhau.
        let r2 = vec![("A".to_string(), 2.0, 10.0), ("B".to_string(), 2.0, 50.0)];
        let i2 = vec![("A".to_string(), 1.0), ("B".to_string(), 1.0)];
        assert_eq!(fifo_cogs(&r2, &i2), 60.0);
        // Xuất vượt tồn (dữ liệu lệch) → không làm hỏng tờ khai.
        assert_eq!(fifo_cogs(&r2, &[("A".to_string(), 99.0)]), 20.0);
    }

    #[test]
    fn gia_von_tung_luot_xuat_cong_lai_khop_fifo_cogs() {
        // Ghi bổ sung bút toán lịch sử cần giá vốn TỪNG lượt xuất (mỗi lượt một
        // dòng 632/152), nhưng cộng lại vẫn phải ra đúng số tờ khai đang dùng.
        let receipts = vec![
            ("SP1".to_string(), 10.0, 100_000.0),
            ("SP1".to_string(), 5.0, 200_000.0),
        ];
        let issues = vec![("SP1".to_string(), 12.0), ("SP1".to_string(), 15.0)];
        let per_issue = fifo_cogs_per_issue(&receipts, &issues);
        // Lượt 1: 10×100k + 2×200k. Lượt 2: hết lô nên chỉ lấy nốt 3×200k.
        assert_eq!(per_issue, vec![1_400_000.0, 600_000.0]);
        assert_eq!(
            round2(per_issue.iter().sum::<f64>()),
            fifo_cogs(&receipts, &issues)
        );
        // Lượt xuất âm không có giá vốn nhưng vẫn giữ chỗ → chỉ số luôn khớp
        // với danh sách lượt xuất (không lệch nhầm dòng khi ghi bù).
        let mixed = vec![("SP1".to_string(), -3.0), ("SP1".to_string(), 1.0)];
        assert_eq!(fifo_cogs_per_issue(&receipts, &mixed), vec![0.0, 100_000.0]);
    }

    #[test]
    fn ke_hoach_bo_sung_gia_von_bo_qua_lo_da_co_but_toan() {
        let mk = |voucher: &str, code: &str, qty: f64| IssueRow {
            voucher_no: voucher.to_string(),
            posting_date: "2026-03-01".to_string(),
            description: String::new(),
            product_code: code.to_string(),
            customer_code: String::new(),
            unit_code: String::new(),
            industry_code: String::new(),
            quantity: qty,
            note: String::new(),
        };
        // PX-1 đã ghi CP; PX-2, PX-3 còn thiếu.
        let issues = vec![
            mk("PX-1", "SP1", 1.0),
            mk("PX-2", "SP1", 1.0),
            mk("PX-3", "SP2", 1.0),
        ];
        let costs = vec![100.0, 200.0, 300.0];
        let mut existing = std::collections::HashMap::new();
        existing.insert(("PX-1".to_string(), "SP1".to_string()), 1);
        assert_eq!(cogs_backfill_plan(&issues, &costs, &existing), vec![1, 2]);
        // Không có giá vốn (hàng giá 0 / xuất vượt tồn) → không đưa vào kế hoạch,
        // không bao giờ ghi bút toán 632/152 bằng 0 làm lệch bảng cân đối.
        assert_eq!(
            cogs_backfill_plan(&issues, &[100.0, 200.0, 0.0], &existing),
            vec![1]
        );
        // Sau khi ghi bù → không còn lượt nào thiếu, chạy lại không trùng.
        existing.insert(("PX-2".to_string(), "SP1".to_string()), 1);
        existing.insert(("PX-3".to_string(), "SP2".to_string()), 1);
        assert!(cogs_backfill_plan(&issues, &costs, &existing).is_empty());
        // Cùng phiếu có hai dòng cùng mã hàng: mỗi dòng một bút toán CP nên đã
        // ghi 1 dòng thì vẫn còn thiếu 1 (đúng như lúc lập phiếu xuất ghi).
        let dup = vec![mk("PX-1", "SP1", 1.0), mk("PX-1", "SP1", 2.0)];
        let dup_costs = vec![100.0, 200.0];
        let mut dup_existing = std::collections::HashMap::new();
        dup_existing.insert(("PX-1".to_string(), "SP1".to_string()), 1);
        assert_eq!(cogs_backfill_plan(&dup, &dup_costs, &dup_existing), vec![1]);
        // Chi phí không khớp danh sách lượt xuất → từ chối để không ghi nhầm số.
        assert!(cogs_backfill_plan(&issues, &[100.0], &existing).is_empty());
    }

    #[test]
    fn canh_bao_khi_nhom_ho_lech_doanh_thu() {
        // Chưa chốt nhóm → không cảnh báo, app tự xếp.
        assert!(!group_mismatch(None, 1));
        assert!(!group_mismatch(Some(1), 1));
        // Hồ sơ ghi Nhóm 2 trong khi doanh thu 12 triệu (app xếp Nhóm 1) → cảnh báo.
        assert!(group_mismatch(Some(2), 1));
        // Ngược lại: hồ sơ còn Nhóm 1 nhưng doanh thu đã vượt 1 tỷ.
        assert!(group_mismatch(Some(1), 2));
        // Giá trị ngoài 1..4 coi như chưa chốt (không cảnh báo).
        assert!(!group_mismatch(Some(0), 2));
        assert!(!group_mismatch(Some(9), 2));
    }

    #[test]
    fn chi_khong_duoc_tru_theo_dieu_6() {
        let items = vec![
            // Được trừ: dưới 5 triệu, không cần chứng từ ngân hàng.
            CostItem {
                deductible: true,
                amount: 1_000_000.0,
                has_bank_doc: false,
            },
            // Được trừ: từ 5 triệu nhưng có chứng từ thanh toán không dùng tiền mặt.
            CostItem {
                deductible: true,
                amount: 10_000_000.0,
                has_bank_doc: true,
            },
            // KHÔNG được trừ: từ 5 triệu, trả tiền mặt không chứng từ.
            CostItem {
                deductible: true,
                amount: 7_000_000.0,
                has_bank_doc: false,
            },
            // KHÔNG được trừ: người dùng đánh dấu (lương chủ hộ, chi cá nhân…).
            CostItem {
                deductible: false,
                amount: 3_000_000.0,
                has_bank_doc: true,
            },
        ];
        let (ok, no) = split_deductible_cost(&items);
        assert_eq!(ok, 11_000_000.0);
        assert_eq!(no, 10_000_000.0);
        assert_eq!(NON_CASH_PAYMENT_LIMIT, 5_000_000.0);
    }

    // ── Bộ cột của 7 mẫu sổ TT 152/2025/TT-BTC ────────────────────────────────
    //
    // Bộ cột là hợp đồng giữa backend và màn hình/xuất Excel: lệch thứ tự hay
    // thiếu cột là sổ in ra không đúng phụ lục nên phải khoá bằng test.

    /// Khóa cột của một mẫu, theo đúng thứ tự in trên phụ lục.
    fn keys(cols: &[BookColumn]) -> Vec<&str> {
        cols.iter().map(|c| c.key.as_str()).collect()
    }

    #[test]
    fn bo_cot_s1a_chi_co_3_cot_khong_co_so_hieu() {
        // Mẫu S1a không có cột số hiệu chứng từ — số hiệu ghi chung ô Diễn giải.
        assert_eq!(keys(&s1a_columns()), ["a", "b", "1"]);
        assert_eq!(s1a_columns()[0].label, "A · Ngày tháng");
        assert_eq!(s1a_columns()[2].kind, COL_MONEY);
    }

    #[test]
    fn bo_cot_s2a_va_s2c_theo_mau_goc() {
        assert_eq!(keys(&revenue_columns()), ["a", "b", "c", "1"]);
        assert_eq!(revenue_columns()[0].label, "A · Số hiệu");
        assert_eq!(revenue_columns()[3].label, "1 · Số tiền");
        // S2c cùng khung A/B/C/1 nhưng là sổ chi tiết doanh thu, chi phí.
        assert_eq!(keys(&s2c_columns()), ["a", "b", "c", "1"]);
    }

    #[test]
    fn bo_cot_s2d_them_cot_ma_ten_va_du_8_cot_mau() {
        let cols = s2d_columns();
        assert_eq!(keys(&cols)[0], "item");
        assert_eq!(
            keys(&cols)[1..],
            ["a", "b", "c", "d", "1", "2", "3", "4", "5", "6", "7", "8"]
        );
        // Cột 2/4/6 là số lượng (nhập, xuất, tồn), cột 3/5/7 là thành tiền.
        assert_eq!(cols[6].kind, COL_QTY);
        assert_eq!(cols[8].kind, COL_QTY);
        assert_eq!(cols[10].kind, COL_QTY);
        assert_eq!(cols[7].kind, COL_MONEY);
        assert_eq!(cols[12].kind, COL_TEXT); // cột 8 · Ghi chú
    }

    #[test]
    fn bo_cot_s2e_tach_thu_vao_va_chi_ra() {
        let cols = s2e_columns();
        assert_eq!(keys(&cols), ["a", "b", "c", "1", "2"]);
        assert_eq!(cols[3].label, "1 · Thu / Gửi vào");
        assert_eq!(cols[4].label, "2 · Chi / Rút ra");
    }

    #[test]
    fn bo_cot_s3a_du_10_cot_thue_theo_phu_luc() {
        let cols = s3a_columns();
        assert_eq!(cols.len(), 12); // A + B + cột 1..10
        assert_eq!(
            keys(&cols)[2..],
            ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"]
        );
        assert_eq!(cols[5].label, "4 · Thuế suất");
        assert_eq!(cols[5].kind, COL_RATE);
        assert_eq!(cols[11].label, "10 · Thuế sử dụng đất");
    }

    #[test]
    fn o_khong_co_so_thi_de_trang_khong_ghi_so_0() {
        // Mẫu gốc để trắng ô không phát sinh; thiếu key thì frontend in trống,
        // còn ghi 0 vào sẽ làm số liệu cột tổng bị sai.
        let mut r = BookRow::detail();
        r.text("a", "PT-01");
        r.money("1", 1000.0);
        let cells = serde_json::to_value(&r)
            .unwrap()
            .get("cells")
            .unwrap()
            .as_object()
            .unwrap()
            .clone();
        assert!(cells.contains_key("1"));
        assert!(cells.contains_key("a"));
        assert!(
            !cells.contains_key("2"),
            "cột không phát sinh phải vắng key"
        );
    }

    #[test]
    fn so_tien_am_chi_cho_giam_doanh_thu() {
        assert_eq!(signed_amount("GiamDT", 500.0), -500.0);
        assert_eq!(signed_amount("TangDT", 500.0), 500.0);
        assert_eq!(signed_amount("", 500.0), 500.0);
    }

    #[test]
    fn nhan_nganh_hoat_duoc_ma_ke_cung() {
        let mut names = std::collections::HashMap::new();
        names.insert(
            "PPHH".to_string(),
            "Phân phối, cung cấp hàng hóa".to_string(),
        );
        assert_eq!(
            industry_label(&names, "PPHH"),
            "Phân phối, cung cấp hàng hóa (PPHH)"
        );
        // Ngành chưa có tên trong danh mục → chỉ ghi mã, không bịa tên.
        assert_eq!(industry_label(&names, "KHAC"), "KHAC");
        assert_eq!(
            industry_label(&std::collections::HashMap::new(), "PPHH"),
            "PPHH"
        );
    }

    #[test]
    fn s3a_khong_bia_so_khi_ung_thue_chua_luu_giao_dich() {
        // App chưa lưu giao dịch thuế khác → sổ rỗng để trống, đừng suy ra số.
        assert!(other_tax_ledger_rows().is_empty());
    }

    #[test]
    fn quyet_toan_theo_doanh_thu_lay_tong_tam_nop() {
        // Hộ nhóm 2 nộp TNCN theo tỷ lệ % doanh thu: số quyết toán = tổng tạm
        // nộp cả năm. Trước đây luôn nhân thu nhập × thuế suất nên ra 1.374.866
        // trong khi chênh lệch ép bằng 0 — hai ô mâu thuẫn nhau.
        assert_eq!(settlement_tax(true, 0.0, false, 9_165_770.0, 0.15), 0.0);
        assert_eq!(
            settlement_tax(true, 1_500_000.0, true, 9_165_770.0, 0.15),
            1_500_000.0
        );
        // Phương pháp thu nhập vẫn tính theo thu nhập tính thuế × thuế suất.
        assert_eq!(
            settlement_tax(false, 0.0, true, 9_165_770.0, 0.15),
            1_374_865.5
        );
        // Cả năm không thuộc diện nộp thuế (Điều 8 khoản 1a) → 0 dù tạm nộp có
        // phát sinh trong năm.
        assert_eq!(
            settlement_tax(false, 1_500_000.0, false, 9_165_770.0, 0.15),
            0.0
        );
    }

    #[test]
    fn so_tien_so_kenh_lam_tron_nguyen_de_khop_to_khai() {
        // Tờ khai ghi 122.020 đ, sổ mà in 122.019,99 thì hai giấy không khớp.
        let mut r = BookRow::detail();
        r.money("1", 122_019.99);
        assert_eq!(r.cells.get("1"), Some(&BookCell::Num(122_020.0)));
        // Số lượng vẫn giữ phần thập phân.
        let mut r = BookRow::detail();
        r.qty("4", 12.3456);
        assert_eq!(r.cells.get("4"), Some(&BookCell::Num(12.346)));
    }

    #[test]
    fn s2c_liet_ke_du_sau_dong_chi_phi_theo_mau_goc() {
        let lines = s2c_cost_lines(2_877_257.0, 358_972.0);
        // Đúng sáu dòng và đúng từng chữ cái a) b) c) d) d) e) như mẫu in kèm
        // Thông tư (mẫu gốc đánh dấu hai lần chữ "d)" — giữ nguyên).
        let marks: Vec<&str> = lines.iter().map(|(label, _)| &label[..3]).collect();
        assert_eq!(marks, ["a) ", "b) ", "c) ", "d) ", "d) ", "e) "]);
        // Giá vốn FIFO vào dòng a), chi phí khác có chứng từ vào dòng e),
        // các dòng chưa có dữ liệu để 0 (frontend sẽ in trống).
        assert_eq!(lines[0].1, 2_877_257.0);
        assert_eq!(lines[5].1, 358_972.0);
        assert_eq!(lines[1].1, 0.0);
        assert_eq!(lines[2].1, 0.0);
        assert_eq!(lines[3].1, 0.0);
        assert_eq!(lines[4].1, 0.0);
        // Tổng a)…e) phải khớp dòng (2) mà sổ cộng ra.
        let sum: f64 = lines.iter().map(|(_, v)| v).sum();
        assert_eq!(round2(sum), 3_236_229.0);
    }

    #[test]
    fn dong_cong_phat_sinh_trong_ky_danh_dau_x_theo_mau_s2d() {
        let r = movement_row("SP001 · Hàng A", 10.0, 100_000.0, 4.0, 40_000.0);
        assert_eq!(
            r.cells.get("c"),
            Some(&BookCell::Text("Công phát sinh trong kỳ".into()))
        );
        // Mẫu gốc đánh dấu X vào cột Đơn vị tính và cột Đơn giá.
        assert_eq!(r.cells.get("d"), Some(&BookCell::Text("X".into())));
        assert_eq!(r.cells.get("1"), Some(&BookCell::Text("X".into())));
        assert_eq!(r.cells.get("2"), Some(&BookCell::Num(10.0)));
        assert_eq!(r.cells.get("3"), Some(&BookCell::Num(100_000.0)));
        assert_eq!(r.cells.get("4"), Some(&BookCell::Num(4.0)));
        assert_eq!(r.cells.get("5"), Some(&BookCell::Num(40_000.0)));
        // Kỳ không phát sinh gì → không ghi số 0 vào ô mẫu gốc để trắng.
        let r = movement_row("SP001 · Hàng A", 0.0, 0.0, 0.0, 0.0);
        assert!(!r.cells.contains_key("2"));
        assert!(!r.cells.contains_key("3"));
        assert!(!r.cells.contains_key("4"));
        assert!(!r.cells.contains_key("5"));
    }
}
