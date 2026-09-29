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
#[tauri::command]
pub(crate) async fn get_tax_summary(
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
    let rows: Vec<TaxAgg> = sqlx::query_as!(
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
    .fetch_all(&*pool)
    .await
    .map_err(|e| e.to_string())?;

    // Mức trừ ngưỡng 01 tỷ là số tiền của cả năm: dùng lũy kế doanh thu từ đầu
    // năm tới hết ngày trước kỳ đang xem để biết kỳ này còn trừ được bao nhiêu.
    let cum = load_cum_revenue_until(&pool, year, &prev_day(&from_date)).await?;
    let period: std::collections::HashMap<String, f64> = rows
        .iter()
        .map(|r| (r.industry_code.clone(), net_revenue(r)))
        .collect();
    let taxable = taxable_by_period(&cum, &period, &ctx.alloc)
        .into_iter()
        .collect();
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
    );
    Ok(serde_json::to_string(&out).unwrap_or_default())
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
pub(crate) fn fifo_cogs(receipts: &[Receipt], issues: &[Issue]) -> f64 {
    // Tồn lô theo mã hàng: Vec<(số lượng còn, đơn giá)> theo thứ tự nhập.
    let mut lots: std::collections::HashMap<&str, Vec<(f64, f64)>> =
        std::collections::HashMap::new();
    for (code, qty, price) in receipts {
        if *qty > 0.0 {
            lots.entry(code.as_str()).or_default().push((*qty, *price));
        }
    }
    let mut cogs = 0.0;
    for (code, qty) in issues {
        if *qty <= 0.0 {
            continue;
        }
        let mut remaining = *qty;
        if let Some(queue) = lots.get_mut(code.as_str()) {
            for lot in queue.iter_mut() {
                if remaining <= 1e-9 {
                    break;
                }
                let take = remaining.min(lot.0);
                cogs += take * lot.1;
                lot.0 -= take;
                remaining -= take;
            }
        }
    }
    round2(cogs)
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

/// Doanh thu tính thuế TNCN của từng nhóm ngành **trong kỳ**, theo phương pháp
/// trừ ngưỡng lũy kế.
///
/// NĐ 68/2026 Điều 8 khoản 1 điểm a: hộ vượt ngưỡng giữa năm thì "khai, nộp thuế
/// kể từ quý phát sinh" → mức trừ 01 tỷ được dùng dần theo doanh thu lũy kế, và
/// tổng các kỳ trong năm luôn bằng đúng mức trừ một lần.
pub(crate) fn taxable_by_period(
    cum_before: &std::collections::HashMap<String, f64>,
    period: &std::collections::HashMap<String, f64>,
    alloc: &[(String, f64)],
) -> Vec<(String, f64)> {
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
            (code.clone(), round2(end - start))
        })
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
) -> Vec<TaxDeclarationRow> {
    // Miễn thuế do NGƯỠNG doanh thu quyết định, không do nhãn nhóm: hộ ghi Nhóm 2
    // trong hồ sơ mà doanh thu chưa vượt ngưỡng vẫn không phát sinh thuế trong kỳ
    // (Điều 8 khoản 1a), và ngược lại hộ ghi Nhóm 1 vẫn phải nộp thuế cho phần
    // doanh thu vượt ngưỡng của các kỳ sau.
    let exempt = !taxable_period;
    rows.into_iter()
        .map(|r| {
            let base = net_revenue(&r);
            let vat_tax = if exempt {
                0.0
            } else {
                round2(base * r.vat_rate)
            };
            let taxable_rev = if exempt {
                0.0
            } else {
                taxable
                    .get(&r.industry_code)
                    .copied()
                    .unwrap_or(0.0)
                    .max(0.0)
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
#[tauri::command]
pub(crate) async fn get_tax_declaration(
    state: State<'_, AppState>,
    year: i64,
    period: String,
    period_no: i64,
) -> Result<String, String> {
    let pool = state.pool.read().await;
    let ctx = load_tax_context(&pool, year).await?;
    let (from_m, to_m) = period_range(&period, period_no)?;
    let rows = load_tax_agg_range(&pool, year, from_m, to_m).await?;
    let cum_before = load_cum_revenue_before(&pool, year, from_m).await?;
    let period_map: std::collections::HashMap<String, f64> = rows
        .iter()
        .map(|r| (r.industry_code.clone(), net_revenue(r)))
        .collect();
    let taxable: std::collections::HashMap<String, f64> =
        taxable_by_period(&cum_before, &period_map, &ctx.alloc)
            .into_iter()
            .collect();
    let taxable_period = period_is_taxable(
        cum_before.values().sum(),
        period_map.values().sum(),
        ctx.settings.thresholds.exempt,
        ctx.taxed_from_start,
    );
    let out = build_tax_declaration(
        rows,
        &ctx.settings.method,
        ctx.group,
        &taxable,
        taxable_period,
    );
    Ok(serde_json::to_string(&out).unwrap_or_default())
}

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
    year: i64,
    period: String,
    period_no: i64,
) -> Result<String, String> {
    let pool = state.pool.read().await;
    let ctx = load_tax_context(&pool, year).await?;
    let (from_m, to_m) = period_range(&period, period_no)?;
    let (from, to) = (
        format!("{:04}-{:02}-01", year, from_m),
        format!("{:04}-{:02}-31", year, to_m),
    );

    let period_rows = load_tax_agg_range(&pool, year, from_m, to_m).await?;
    let period_revenue = year_revenue(&period_rows);
    let cum_before = load_cum_revenue_before(&pool, year, from_m).await?;
    let period_map: std::collections::HashMap<String, f64> = period_rows
        .iter()
        .map(|r| (r.industry_code.clone(), net_revenue(r)))
        .collect();
    let taxable: std::collections::HashMap<String, f64> =
        taxable_by_period(&cum_before, &period_map, &ctx.alloc)
            .into_iter()
            .collect();
    let taxable_revenue: f64 = taxable.values().sum();

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
        exempt_threshold: ctx.settings.thresholds.exempt,
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
        deadline: tax_deadline(&period, period_no, year),
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
    // Quyết toán cả năm chỉ có nghĩa khi hộ thuộc diện tính thuế (Điều 8 khoản 1a);
    // hộ cả năm không vượt ngưỡng thì tổng thuế bằng 0 dù hồ sơ ghi nhóm nào.
    let year_taxable = ctx.taxed_from_start || ctx.year_revenue > ctx.settings.thresholds.exempt;
    let rate = if year_taxable {
        profit_tncn_rate(ctx.group)
    } else {
        0.0
    };
    let final_tax = if year_taxable {
        round2(income.max(0.0) * rate)
    } else {
        0.0
    };
    // Hộ nộp theo tỷ lệ % doanh thu thì số phải nộp chính là tổng tạm nộp.
    // Hộ vượt ngưỡng mà hồ sơ đang ghi Nhóm 1 thì không có mức thuế suất theo
    // thu nhập để áp, nên TNCN vẫn tính theo tỷ lệ % doanh thu cho tới khi hộ
    // xác nhận lại nhóm (nguyên tắc ổn định: nhóm của năm không tự đổi giữa chừng).
    let by_revenue = ctx.group <= 2 && ctx.settings.method == "revenue";
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
}

// ─── SỔ KẾ TOÁN THEO MẪU TT 152/2025/TT-BTC ───
//
// Thông tư 152/2025/TT-BTC (thay TT 88/2021) quy định mẫu sổ bắt buộc:
//   S1a-HKD  — nhóm không chịu GTGT, không nộp TNCN  → Sổ doanh thu bán hàng hóa, dịch vụ
//   S2a-HKD  — nộp GTGT và TNCN theo tỷ lệ % trên doanh thu → Sổ doanh thu theo nhóm ngành
//   S2b-HKD  — nộp GTGT theo % doanh thu, TNCN trên thu nhập → Sổ doanh thu theo nhóm ngành
//   S2c-HKD  — Sổ chi tiết doanh thu, chi phí
//   S2d-HKD  — Sổ chi tiết vật liệu, dụng cụ, sản phẩm, hàng hóa
//   S2e-HKD  — Sổ chi tiết tiền
// Mỗi sổ ghi **theo từng chứng từ** và **theo từng nhóm ngành có cùng tỷ lệ %**
// nên phần "dòng chi tiết" dưới đây là dữ liệu bắt buộc phải có khi lưu sổ.

/// Một dòng chi tiết của sổ (một dòng chứng t�� nội dung sổ).
#[derive(serde::Serialize)]
pub(crate) struct BookRow {
    pub(crate) group: String,    // nhóm ngành (trống = không theo nhóm)
    pub(crate) doc_no: String,   // Số hiệu chứng từ (cột A)
    pub(crate) doc_date: String, // Ngày tháng (cột B)
    pub(crate) desc: String,     // Diễn giải (cột C)
    pub(crate) amount: f64,      // Số tiền (cột D)
    pub(crate) quantity: f64,    // Số lượng (sổ vật liệu — S2d)
}

/// Nội dung 1 mẫu sổ: tiêu đề + dòng chi tiết theo chứng từ + tổng + ghi chú.
#[derive(serde::Serialize)]
pub(crate) struct TaxBook {
    pub(crate) book: String,
    pub(crate) title: String,
    pub(crate) period_label: String,
    pub(crate) rows: Vec<BookRow>,
    pub(crate) total_revenue: f64,
    pub(crate) total_vat: f64,
    pub(crate) total_pit: f64,
    pub(crate) notes: String,
}

/// Lấy 1 mẫu sổ TT 152/2025 cho kỳ đang chọn.
///
/// `book`: S1a | S2a | S2b | S2c | S2d | S2e
#[tauri::command]
pub(crate) async fn get_tax_books(
    state: State<'_, AppState>,
    year: i64,
    period: String,
    period_no: i64,
    book: String,
) -> Result<String, String> {
    let pool = state.pool.read().await;
    let ctx = load_tax_context(&pool, year).await?;
    let (from_m, to_m) = period_range(&period, period_no)?;
    let from = format!("{year:04}-{from_m:02}-01");
    let to = format!("{year:04}-{to_m:02}-31");
    let period_label = describe_tax_period(&period, period_no, year);

    // Chi tiết chứng từ doanh thu (PX) theo nhóm ngành — dùng chung cho S1a/S2a/S2b.
    let sales = sqlx::query!(
        r#"SELECT je.voucher_no AS "voucher_no!", je.posting_date AS "posting_date!",
                  je.description AS "description!", je.industry_code AS "industry_code!",
                  je.adjust_code AS "adjust_code!", je.amount, je.quantity,
                  p.name AS "product_name!", COALESCE(p.unit, '') AS "unit!"
             FROM journal_entry je
             LEFT JOIN product p ON p.code = je.product_code
            WHERE je.entry_type = 'PX'
              AND je.posting_date >= ? AND je.posting_date <= ?
            ORDER BY je.posting_date, je.id"#,
        from,
        to
    )
    .fetch_all(&*pool)
    .await
    .map_err(|e| e.to_string())?;

    let mut rows: Vec<BookRow> = Vec::new();
    let (mut total_revenue, mut total_vat, mut total_pit) = (0.0, 0.0, 0.0);

    match book.as_str() {
        // S1a: hộ nhóm 1 — sổ doanh thu, ghi theo từng chứng từ.
        "S1a" => {
            for s in &sales {
                let amount = if s.adjust_code == "GiamDT" {
                    -s.amount
                } else {
                    s.amount
                };
                total_revenue += amount;
                rows.push(BookRow {
                    group: s.industry_code.clone(),
                    doc_no: s.voucher_no.clone(),
                    doc_date: s.posting_date.clone(),
                    desc: s.description.clone(),
                    amount: round2(amount),
                    quantity: s.quantity,
                });
            }
        }
        // S2a/S2b: sổ doanh thu theo nhóm ngành có cùng tỷ lệ %.
        "S2a" | "S2b" => {
            let cum = load_cum_revenue_before(&pool, year, from_m).await?;
            let period_rows = load_tax_agg_range(&pool, year, from_m, to_m).await?;
            let period_map: std::collections::HashMap<String, f64> = period_rows
                .iter()
                .map(|r| (r.industry_code.clone(), net_revenue(r)))
                .collect();
            let taxable: std::collections::HashMap<String, f64> =
                taxable_by_period(&cum, &period_map, &ctx.alloc)
                    .into_iter()
                    .collect();
            // Hộ nhóm 1 không chịu thuế nên sổ chỉ ghi doanh thu, không ghi số
            // thuế (khớp với tờ khai và thẻ tổng hợp trên cùng màn).
            let exempt = ctx.group == 1;
            let rates = load_industry_rates(&pool).await?;
            for s in &sales {
                let amount = if s.adjust_code == "GiamDT" {
                    -s.amount
                } else {
                    s.amount
                };
                total_revenue += amount;
                // TNCN chỉ tính trên doanh thu TÍNH THUẾ của cả kỳ, nên dòng
                // chi tiết chỉ ghi doanh thu; số thuế tính ở dòng tổng theo nhóm.
                let rate_vat = if exempt {
                    0.0
                } else {
                    rates
                        .iter()
                        .find(|(c, _, _)| c == &s.industry_code)
                        .map(|(_, v, _)| *v)
                        .unwrap_or(0.0)
                };
                total_vat += round2(amount * rate_vat);
                rows.push(BookRow {
                    group: s.industry_code.clone(),
                    doc_no: s.voucher_no.clone(),
                    doc_date: s.posting_date.clone(),
                    desc: s.description.clone(),
                    amount: round2(amount),
                    quantity: s.quantity,
                });
            }
            // TNCN theo tỷ lệ % doanh thu chỉ áp dụng cho hộ nhóm 2 chọn phương
            // pháp doanh thu; hộ nhóm 1 thì không phát sinh thuế.
            if !exempt && ctx.group == 2 && ctx.settings.method == "revenue" {
                total_pit = period_rows
                    .iter()
                    .map(|r| {
                        let t = taxable.get(&r.industry_code).copied().unwrap_or(0.0);
                        round2(t * r.pit_rate)
                    })
                    .sum();
            }
        }
        // S2c: sổ chi tiết doanh thu, chi phí (cơ sở tính thu nhập tính thuế).
        "S2c" => {
            let period_rows = load_tax_agg_range(&pool, year, from_m, to_m).await?;
            let cogs = load_fifo_cogs(&pool, &from, &to).await?;
            let (cost_ok, cost_no, warnings) = load_period_costs(&pool, &from, &to).await?;
            for r in &period_rows {
                let net = round2(net_revenue(r));
                if net == 0.0 {
                    continue;
                }
                total_revenue += net;
                rows.push(BookRow {
                    group: r.industry_code.clone(),
                    doc_no: "—".into(),
                    doc_date: format!("{year:04}-{from_m:02}…{year:04}-{to_m:02}"),
                    desc: format!("Doanh thu bán hàng hóa, dịch vụ — {}", r.industry_name),
                    amount: net,
                    quantity: 0.0,
                });
            }
            rows.push(BookRow {
                group: String::new(),
                doc_no: "—".into(),
                doc_date: format!("{year:04}-{from_m:02}…{year:04}-{to_m:02}"),
                desc: "Giá vốn hàng hóa, dịch vụ (FIFO theo phiếu nhập)".into(),
                amount: -round2(cogs),
                quantity: 0.0,
            });
            rows.push(BookRow {
                group: String::new(),
                doc_no: "—".into(),
                doc_date: format!("{year:04}-{from_m:02}…{year:04}-{to_m:02}"),
                desc: "Chi phí khác được trừ (phiếu chi đủ chứng từ)".into(),
                amount: -round2(cost_ok),
                quantity: 0.0,
            });
            for w in &warnings {
                rows.push(BookRow {
                    group: String::new(),
                    doc_no: w.voucher_no.clone(),
                    doc_date: w.posting_date.clone(),
                    desc: format!("KHÔNG được trừ: {} — {}", w.description, w.reason),
                    amount: -round2(w.amount),
                    quantity: 0.0,
                });
            }
            let _ = cost_no;
        }
        // S2d: sổ chi tiết vật liệu, dụng cụ, sản phẩm, hàng hóa (nhập / xuất / tồn).
        "S2d" => {
            let moves = sqlx::query!(
                r#"SELECT je.voucher_no AS "voucher_no!", je.posting_date AS "posting_date!",
                          je.description AS "description!", je.product_code AS "product_code!",
                          je.entry_type AS "entry_type!", je.quantity, je.unit_price, je.amount
                     FROM journal_entry je
                    WHERE je.entry_type IN ('PN', 'PX')
                      AND je.posting_date >= ? AND je.posting_date <= ?
                    ORDER BY je.posting_date, je.id"#,
                from,
                to
            )
            .fetch_all(&*pool)
            .await
            .map_err(|e| e.to_string())?;
            for m in &moves {
                rows.push(BookRow {
                    group: m.product_code.clone(),
                    doc_no: m.voucher_no.clone(),
                    doc_date: m.posting_date.clone(),
                    desc: format!(
                        "{} {} — {}",
                        if m.entry_type == "PN" {
                            "Nhập"
                        } else {
                            "Xuất"
                        },
                        m.quantity,
                        m.description
                    ),
                    amount: round2(m.quantity * m.unit_price),
                    quantity: m.quantity,
                });
            }
        }
        // S2e: sổ chi tiết tiền (phiếu thu / phiếu chi).
        "S2e" => {
            let cash = sqlx::query!(
                r#"SELECT je.voucher_no AS "voucher_no!", je.posting_date AS "posting_date!",
                          je.description AS "description!", je.entry_type AS "entry_type!",
                          je.amount, je.bank_code AS "bank_code!"
                     FROM journal_entry je
                    WHERE je.entry_type IN ('PT', 'PC')
                      AND je.posting_date >= ? AND je.posting_date <= ?
                    ORDER BY je.posting_date, je.id"#,
                from,
                to
            )
            .fetch_all(&*pool)
            .await
            .map_err(|e| e.to_string())?;
            for c in &cash {
                let sign = if c.entry_type == "PT" { 1.0 } else { -1.0 };
                total_revenue += sign * c.amount;
                rows.push(BookRow {
                    group: String::new(),
                    doc_no: c.voucher_no.clone(),
                    doc_date: c.posting_date.clone(),
                    desc: format!(
                        "{} {}{}",
                        if c.entry_type == "PT" { "Thu" } else { "Chi" },
                        c.description,
                        if c.bank_code.trim().is_empty() {
                            " (tiền mặt)"
                        } else {
                            ""
                        }
                    ),
                    amount: round2(sign * c.amount),
                    quantity: 0.0,
                });
            }
        }
        other => return Err(format!("Mẫu sổ '{other}' không hợp lệ")),
    }

    let title = match book.as_str() {
        "S1a" => "SỔ DOANH THU BÁN HÀNG HÓA, DỊCH VỤ",
        "S2a" => "SỔ DOANH THU BÁN HÀNG HÓA, DỊCH VỤ (theo nhóm ngành có cùng tỷ lệ %)",
        "S2b" => "SỔ DOANH THU BÁN HÀNG HÓA, DỊCH VỤ (thiêu thuế GTGT theo nhóm ngành)",
        "S2c" => "SỔ CHI TIẾT DOANH THU, CHI PHÍ",
        "S2d" => "SỔ CHI TIẾT VẬT LIỆU, DỤNG CỤ, SẢN PHẨM, HÀNG HÓA",
        _ => "SỔ CHI TIẾT TIỀN",
    };
    let notes = match book.as_str() {
        "S1a" => format!(
            "Hộ thuộc nhóm {} — chỉ thông báo doanh thu, hạn {}.",
            ctx.group,
            tax_deadline("year", 0, year).unwrap_or_default()
        ),
        "S2a" | "S2b" => {
            let base = format!(
                "Nhóm {} · phương pháp TNCN: {} · mức trừ ngưỡng đã dùng trong năm: {} đ",
                ctx.group,
                if ctx.settings.method == "revenue" { "theo doanh thu" } else { "theo thu nhập" },
                fmt_thousands(ctx.alloc.iter().map(|(_, v)| v).sum::<f64>())
            );
            if ctx.group == 1 {
                format!("{base} — hộ nhóm 1 chỉ thông báo doanh thu, không phát sinh thuế.")
            } else {
                base
            }
        }
        "S2c" => "Dòng 'KHÔNG được trừ' là các khoản chi chưa đủ chứng từ theo Điều 6 Nghị định 68/2026.".into(),
        "S2d" => "Số tiền = số lượng × đơn giá ghi trên chứng từ; giá vốn xuất tính FIFO.".into(),
        _ => "Ghi rõ chứng từ thanh toán không dùng tiền mặt để khoản chi từ 05 triệu trở lên được trừ.".into(),
    };
    let out = TaxBook {
        book: book.clone(),
        title: title.into(),
        period_label,
        rows,
        total_revenue: round2(total_revenue),
        total_vat: round2(total_vat),
        total_pit: round2(total_pit),
        notes,
    };
    Ok(serde_json::to_string(&out).unwrap_or_default())
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
#[tauri::command]
pub(crate) async fn get_revenue_expense(
    state: State<'_, AppState>,
    from_date: String,
    to_date: String,
) -> Result<String, String> {
    let row: RevenueExpenseRow = sqlx::query_as!(
        RevenueExpenseRow,
        "SELECT
            COALESCE(SUM(CASE WHEN entry_type = 'PX' AND adjust_code != 'GiamDT' THEN amount ELSE 0.0 END), 0.0) AS revenue_up,
            COALESCE(SUM(CASE WHEN entry_type = 'PX' AND adjust_code = 'GiamDT' THEN amount ELSE 0.0 END), 0.0) AS revenue_down,
            COALESCE(SUM(CASE WHEN entry_type = 'PC' AND adjust_code != 'GiamCP' THEN amount ELSE 0.0 END), 0.0) AS expense_up,
            COALESCE(SUM(CASE WHEN entry_type = 'PC' AND adjust_code = 'GiamCP' THEN amount ELSE 0.0 END), 0.0) AS expense_down
         FROM journal_entry
         WHERE posting_date >= ?
           AND posting_date <= ?",
        from_date,
        to_date
    )
    .fetch_one(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    Ok(serde_json::to_string(&row).unwrap_or_default())
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
        );
        let r = &out[0];
        assert_eq!(r.revenue_up, 100_000_000.0);
        assert_eq!(r.revenue_down, 10_000_000.0);
        assert_eq!(r.vat_tax, 900_000.0); // 90tr x 1%
        assert_eq!(r.vat_payable, 900_000.0);
        assert_eq!(r.pit_tax, 450_000.0); // 90tr x 0,5%
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
        let taxable = taxable_by_period(&Default::default(), &period_map, &ctx.alloc)
            .into_iter()
            .collect();
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
}
