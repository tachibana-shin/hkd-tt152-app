#![allow(unused_imports)]

use crate::helpers::*;
use crate::models::*;
use serde_json::json;
use sqlx::sqlite::{SqlitePoolOptions, SqliteTransaction};
use sqlx::SqlitePool;
use tauri::{State, Manager, AppHandle};

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

/// Bảng cân đối số phát sinh: số dư đầu kỳ (từ DMTK) + phát sinh trong kỳ
/// (từ journal_entry) + số dư cuối kỳ, nhóm theo từng tài khoản.
#[tauri::command]
pub(crate) async fn get_trial_balance(
    state: State<'_, AppState>,
    from_date: String,
    to_date: String,
) -> Result<String, String> {
    let pool = state.pool.read().await;
    let accounts: Vec<(String, String, f64, f64)> = sqlx::query_as(
        "SELECT code, name, opening_debit, opening_credit FROM account ORDER BY code",
    )
    .fetch_all(&*pool)
    .await
    .map_err(|e| e.to_string())?;

    // Tổng phát sinh Nợ / Có theo từng tài khoản trong kỳ.
    let mvmt: Vec<(String, f64, f64)> = sqlx::query_as(
        "SELECT code, SUM(d) AS debit_mvmt, SUM(c) AS credit_mvmt FROM (
            SELECT debit_account AS code, amount AS d, 0.0 AS c FROM journal_entry
            WHERE posting_date >= ? AND posting_date <= ?
            UNION ALL
            SELECT credit_account, 0.0, amount FROM journal_entry
            WHERE posting_date >= ? AND posting_date <= ?
         ) GROUP BY code",
    )
    .bind(&from_date)
    .bind(&to_date)
    .bind(&from_date)
    .bind(&to_date)
    .fetch_all(&*pool)
    .await
    .map_err(|e| e.to_string())?;

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

/// Doanh thu + thuế theo nhóm ngành nghề (≡ To Khai Thue sheet)
#[tauri::command]
pub(crate) async fn get_tax_summary(
    state: State<'_, AppState>,
    from_date: String,
    to_date: String,
    unit_code: String,
) -> Result<String, String> {
    let rows: Vec<TaxSummaryRow> = sqlx::query_as!(
        TaxSummaryRow,
        "SELECT
            ig.code AS industry_code,
            ig.name AS industry_name,
            ig.vat_rate,
            ig.pit_rate,
            COALESCE(SUM(CASE WHEN je.adjust_code != 'GiamDT' THEN je.amount ELSE 0.0 END), 0.0) AS revenue_up,
            COALESCE(SUM(CASE WHEN je.adjust_code = 'GiamDT' THEN je.amount ELSE 0.0 END), 0.0) AS revenue_down,
            COALESCE(SUM(CASE WHEN je.adjust_code != 'GiamDT' THEN je.amount ELSE 0.0 END) - SUM(CASE WHEN je.adjust_code = 'GiamDT' THEN je.amount ELSE 0.0 END), 0.0) * ig.vat_rate AS vat_tax,
            COALESCE(SUM(CASE WHEN je.adjust_code != 'GiamDT' THEN je.amount ELSE 0.0 END) - SUM(CASE WHEN je.adjust_code = 'GiamDT' THEN je.amount ELSE 0.0 END), 0.0) * ig.pit_rate AS pit_tax
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
    .fetch_all(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    Ok(serde_json::to_string(&rows).unwrap_or_default())
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

// ─── QUY ĐỊNH THUẾ HKD 2026 (NĐ 68/2026/NĐ-CP, NĐ 141/2026/NĐ-CP) ───
//
// Nhóm hộ theo tổng doanh thu CẢ NĂM:
//   Nhóm 1: ≤ 1 tỷ đồng/năm  → miễn thuế GTGT + TNCN (chỉ thông báo doanh thu năm)
//   Nhóm 2: > 1 tỷ – 3 tỷ     → GTGT theo tỷ lệ ngành; TNCN chọn: theo doanh thu x tỷ lệ
//                                ngành HOẶC lợi nhuận x 15%
//   Nhóm 3: > 3 tỷ – 50 tỷ    → GTGT theo tỷ lệ ngành; TNCN = lợi nhuận x 17%
//   Nhóm 4: > 50 tỷ           → GTGT theo tỷ lệ ngành; TNCN = lợi nhuận x 20%
// Kỳ khai: quý (nhóm 2–3), tháng (nhóm 4), năm (nhóm 1 — thông báo DT), từng lần phát sinh.
const THRESHOLD_EXEMPT: f64 = 1_000_000_000.0; // 1 tỷ
const THRESHOLD_GROUP3: f64 = 3_000_000_000.0; // 3 tỷ
const THRESHOLD_GROUP4: f64 = 50_000_000_000.0; // 50 tỷ

/// Xếp nhóm hộ kinh doanh theo tổng doanh thu cả năm.
pub(crate) fn tax_group(total_revenue_year: f64) -> i64 {
    if total_revenue_year <= THRESHOLD_EXEMPT {
        1
    } else if total_revenue_year <= THRESHOLD_GROUP3 {
        2
    } else if total_revenue_year <= THRESHOLD_GROUP4 {
        3
    } else {
        4
    }
}

/// Thuế suất TNCN theo lợi nhuận theo nhóm hộ (15% / 17% / 20%).
pub(crate) fn profit_tncn_rate(group: i64) -> f64 {
    match group {
        2 => 0.15,
        3 => 0.17,
        _ => 0.20,
    }
}

/// Đọc cấu hình kê khai thuế từ app_setting: (kỳ khai, phương pháp TNCN).
/// Mặc định: quý + theo doanh thu.
pub(crate) async fn load_tax_settings(pool: &SqlitePool) -> Result<(String, String), String> {
    let rows: Vec<(Option<String>, String)> = sqlx::query_as(
        "SELECT key, value FROM app_setting WHERE key IN ('tax_period', 'tax_method')",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    let mut period = "quarter".to_string();
    let mut method = "revenue".to_string();
    for (k, v) in rows {
        match k.as_deref() {
            Some("tax_period") => period = v,
            Some("tax_method") => method = v,
            _ => {}
        }
    }
    Ok((period, method))
}

/// Chuyển kỳ khai (year/quarter/month/occurrence) + số thứ tự kỳ thành khoảng tháng.
fn period_range(period: &str, no: i64) -> Result<(i64, i64), String> {
    match period {
        "year" | "occurrence" => Ok((1, 12)),
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

/// Tính số thuế phải nộp từ dữ liệu gộp thô (thuần → test được) theo quy định 2026:
///   Nhóm 1 (≤ 1 tỷ)          → miễn thuế GTGT + TNCN
///   Nhóm 2 (> 1–3 tỷ):
///     - GTGT = (DT tăng - DT giảm) x tỷ lệ ngành
///     - TNCN: theo doanh thu (x tỷ lệ ngành) hoặc theo lợi nhuận (x 15% — ở tổng hợp)
///   Nhóm 3/4 (> 3 tỷ)        → TNCN theo lợi nhuận x 17%/20% (ở tổng hợp)
pub(crate) fn build_tax_declaration(rows: Vec<TaxAgg>, method: &str, group: i64) -> Vec<TaxDeclarationRow> {
    let exempt = group == 1;
    rows.into_iter()
        .map(|r| {
            let base = r.revenue_up - r.revenue_down;
            let vat_tax = if exempt { 0.0 } else { round2(base * r.vat_rate) };
            let pit_tax = if exempt || group != 2 || method != "revenue" {
                0.0
            } else {
                round2(base * r.pit_rate)
            };
            TaxDeclarationRow {
                industry_code: r.industry_code,
                industry_name: r.industry_name,
                vat_rate: r.vat_rate,
                pit_rate: r.pit_rate,
                revenue_up: round2(r.revenue_up),
                revenue_down: round2(r.revenue_down),
                vat_tax,
                vat_payable: round2(vat_tax),
                pit_tax,
            }
        })
        .collect()
}

/// Tờ khai thuế theo kỳ (sheet To Khai Thue) — nhóm theo nhóm ngành nghề.
/// period: "month" | "quarter" | "year" | "occurrence"; period_no: 1..12 / 1..4 / 0.
#[tauri::command]
pub(crate) async fn get_tax_declaration(
    state: State<'_, AppState>,
    year: i64,
    period: String,
    period_no: i64,
) -> Result<String, String> {
    let pool = state.pool.read().await;
    let (_, method) = load_tax_settings(&pool).await?;
    let (from_m, to_m) = period_range(&period, period_no)?;
    // Nhóm hộ xác định theo tổng doanh thu CẢ NĂM (không phụ thuộc kỳ khai).
    let group = {
        let year_rows = load_tax_agg(&pool, year, 0).await?;
        tax_group(year_rows.iter().map(|r| r.revenue_up - r.revenue_down).sum())
    };
    let rows = load_tax_agg_range(&pool, year, from_m, to_m).await?;
    let out = build_tax_declaration(rows, &method, group);
    Ok(serde_json::to_string(&out).unwrap_or_default())
}

/// Tổng hợp thuế phải nộp theo quy định 2026 cho kỳ khai đã chọn:
/// xếp nhóm hộ theo doanh thu cả năm, tính GTGT + TNCN (doanh thu hoặc lợi nhuận).
#[tauri::command]
pub(crate) async fn get_tax_overview(
    state: State<'_, AppState>,
    year: i64,
    period: String,
    period_no: i64,
) -> Result<String, String> {
    let pool = state.pool.read().await;
    let (_, method) = load_tax_settings(&pool).await?;
    let (from_m, to_m) = period_range(&period, period_no)?;
    let (from, to) = (
        format!("{:04}-{:02}-01", year, from_m),
        format!("{:04}-{:02}-31", year, to_m),
    );

    let year_rows = load_tax_agg(&pool, year, 0).await?;
    let year_revenue: f64 = year_rows.iter().map(|r| r.revenue_up - r.revenue_down).sum();
    let group = tax_group(year_revenue);

    let period_rows = load_tax_agg_range(&pool, year, from_m, to_m).await?;
    let period_revenue: f64 = period_rows.iter().map(|r| r.revenue_up - r.revenue_down).sum();

    // Chi phí trong kỳ (phiếu chi PC, trừ điều chỉnh giảm chi phí).
    let (exp_up, exp_down): (f64, f64) = sqlx::query_as(
        "SELECT
            COALESCE(SUM(CASE WHEN entry_type = 'PC' AND adjust_code != 'GiamCP' THEN amount ELSE 0.0 END), 0.0),
            COALESCE(SUM(CASE WHEN entry_type = 'PC' AND adjust_code = 'GiamCP' THEN amount ELSE 0.0 END), 0.0)
         FROM journal_entry
         WHERE posting_date >= ? AND posting_date <= ?",
    )
    .bind(&from)
    .bind(&to)
    .fetch_one(&*pool)
    .await
    .map_err(|e| e.to_string())?;
    let expense = exp_up - exp_down;
    let profit = period_revenue - expense;

    // Thuế GTGT phải nộp (miễn cho nhóm 1).
    let vat_payable: f64 = if group == 1 {
        0.0
    } else {
        period_rows
            .iter()
            .map(|r| {
                let base = r.revenue_up - r.revenue_down;
                round2(base * r.vat_rate)
            })
            .sum()
    };

    // Thuế TNCN:
    //   Nhóm 2 + phương pháp doanh thu → (DT tăng - DT giảm) x tỷ lệ ngành
    //   Còn lại (nhóm 2 theo lợi nhuận, nhóm 3/4) → lợi nhuận x 15%/17%/20% (≥ 0)
    let pit_tax: f64 = if group == 1 {
        0.0
    } else if group == 2 && method == "revenue" {
        period_rows
            .iter()
            .map(|r| round2((r.revenue_up - r.revenue_down) * r.pit_rate))
            .sum()
    } else {
        round2(profit.max(0.0) * profit_tncn_rate(group))
    };

    let profit_rate = if group == 2 {
        if method == "profit" { 0.15 } else { 0.0 }
    } else if group == 3 {
        0.17
    } else if group == 4 {
        0.20
    } else {
        0.0
    };

    let overview = TaxOverview {
        year,
        year_revenue: round2(year_revenue),
        group,
        method,
        period_revenue: round2(period_revenue),
        expense: round2(expense),
        profit: round2(profit),
        profit_rate,
        vat_payable: round2(vat_payable),
        pit_tax: round2(pit_tax),
        total_tax: round2(vat_payable + pit_tax),
    };
    Ok(serde_json::to_string(&overview).unwrap_or_default())
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
            COALESCE(SUM(CASE WHEN je.entry_type = 'PN' AND je.adjust_code != 'GiamCP' THEN je.quantity ELSE 0.0 END), 0.0) AS inbound,
            COALESCE(SUM(CASE WHEN je.entry_type = 'PX' AND je.adjust_code != 'GiamDT' THEN je.quantity ELSE 0.0 END), 0.0) AS outbound,
            (COALESCE(SUM(CASE WHEN je.entry_type = 'PN' AND je.adjust_code != 'GiamCP' THEN je.quantity ELSE 0.0 END), 0.0) - COALESCE(SUM(CASE WHEN je.entry_type = 'PX' AND je.adjust_code != 'GiamDT' THEN je.quantity ELSE 0.0 END), 0.0)) AS balance
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

    #[test]
    fn to_khai_tinh_gtgt_toan_bo_khong_con_giam_thue() {
        // DT 100tr, ngành PPHH 1%, hộ nhóm 2 → GTGT phải nộp = 1.000.000
        // (chức năng giảm thuế GTGT đã bỏ — không còn trừ khoản "được giảm")
        let out = build_tax_declaration(vec![agg(100_000_000.0, 0.0)], "revenue", 2);
        let r = &out[0];
        assert_eq!(r.vat_tax, 1_000_000.0);
        assert_eq!(r.vat_payable, 1_000_000.0);
        assert_eq!(r.pit_tax, 500_000.0); // 100tr x 0,5%
    }

    #[test]
    fn to_khai_tru_doanh_thu_dieu_chinh_giam() {
        // DT tăng 100tr, DT giảm 10tr → còn 90tr
        let out = build_tax_declaration(vec![agg(100_000_000.0, 10_000_000.0)], "revenue", 2);
        let r = &out[0];
        assert_eq!(r.revenue_up, 100_000_000.0);
        assert_eq!(r.revenue_down, 10_000_000.0);
        assert_eq!(r.vat_tax, 900_000.0); // 90tr x 1%
        assert_eq!(r.vat_payable, 900_000.0);
        assert_eq!(r.pit_tax, 450_000.0); // 90tr x 0,5%
    }

    #[test]
    fn nhom1_doanh_thu_duoi_1_ty_mien_thue() {
        // Nhóm 1 (≤ 1 tỷ/năm): miễn GTGT + TNCN kể cả khi có doanh thu trong kỳ
        let out = build_tax_declaration(vec![agg(800_000_000.0, 0.0)], "revenue", 1);
        let r = &out[0];
        assert_eq!(r.vat_tax, 0.0);
        assert_eq!(r.vat_payable, 0.0);
        assert_eq!(r.pit_tax, 0.0);
        // Doanh thu vẫn hiển thị để theo dõi ngưỡng
        assert_eq!(r.revenue_up, 800_000_000.0);
    }

    #[test]
    fn nhom2_theo_loi_nhuan_thi_tncn_khong_tinh_tren_dong() {
        // Phương pháp lợi nhuận: cột TNCN theo tỷ lệ doanh thu bằng 0
        // (số TNCN = lợi nhuận x 15% tính ở phần tổng hợp)
        let out = build_tax_declaration(vec![agg(2_000_000_000.0, 0.0)], "profit", 2);
        assert_eq!(out[0].pit_tax, 0.0);
        assert_eq!(out[0].vat_tax, 20_000_000.0); // 2 tỷ x 1%
    }

    #[test]
    fn nhom3_4_tncn_luon_theo_loi_nhuan() {
        // Nhóm 3 (> 3–50 tỷ): TNCN không theo tỷ lệ doanh thu, kể cả khi cấu hình "revenue"
        let out = build_tax_declaration(vec![agg(10_000_000_000.0, 0.0)], "revenue", 3);
        assert_eq!(out[0].pit_tax, 0.0);
        assert_eq!(out[0].vat_tax, 100_000_000.0); // 10 tỷ x 1%
    }

    #[test]
    fn xep_nhom_theo_tong_doanh_thu_nam() {
        // Ngưỡng: ≤ 1 tỷ = nhóm 1 (miễn); > 1–3 tỷ = 2; > 3–50 tỷ = 3; > 50 tỷ = 4
        assert_eq!(tax_group(500_000_000.0), 1);
        assert_eq!(tax_group(1_000_000_000.0), 1); // 1 tỷ (bằng ngưỡng) → miễn
        assert_eq!(tax_group(1_000_000_000.01), 2);
        assert_eq!(tax_group(3_000_000_000.0), 2);
        assert_eq!(tax_group(3_000_000_000.01), 3);
        assert_eq!(tax_group(50_000_000_000.0), 3);
        assert_eq!(tax_group(50_000_000_000.01), 4);
    }

    #[test]
    fn thue_suat_tncn_theo_loi_nhuan() {
        assert_eq!(profit_tncn_rate(2), 0.15);
        assert_eq!(profit_tncn_rate(3), 0.17);
        assert_eq!(profit_tncn_rate(4), 0.20);
    }

    #[tokio::test]
    async fn to_khai_tu_db_tong_hop_theo_ky() {
        let pool = test_pool().await;
        seed_product(&pool, "P1", "Hàng 1", 0.01).await;
        seed_product(&pool, "P2", "Hàng 2", 0.01).await;

        // Trong kỳ 05/2026: P1 1 tỷ + P2 500tr → tổng năm > 1 tỷ (nhóm 2)
        add_journal_px(&pool, "2026-05-10", "PX1", "P1", "PPHH", 1000.0, 1_000_000.0, 1_000_000_000.0, 0.01, 0.005).await;
        add_journal_px(&pool, "2026-05-11", "PX2", "P2", "PPHH", 500.0, 1_000_000.0, 500_000_000.0, 0.01, 0.005).await;
        // Ngoài kỳ (06/2026) — không được tính vào tờ khai tháng 5
        add_journal_px(&pool, "2026-06-01", "PX3", "P2", "PPHH", 100.0, 1_000_000.0, 100_000_000.0, 0.01, 0.005).await;

        let out = build_tax_declaration(load_tax_agg(&pool, 2026, 5).await.unwrap(), "revenue", 2);
        let pphh = out.iter().find(|r| r.industry_code == "PPHH").unwrap();
        assert_eq!(pphh.revenue_up, 1_500_000_000.0); // chỉ PX1 + PX2
        assert_eq!(pphh.vat_tax, 15_000_000.0); // 1,5 tỷ x 1%
        assert_eq!(pphh.vat_payable, 15_000_000.0); // không còn giảm thuế
        assert_eq!(pphh.pit_tax, 7_500_000.0); // 1,5 tỷ x 0,5%

        // Các ngành khác không phát sinh doanh thu
        for r in out.iter().filter(|r| r.industry_code != "PPHH") {
            assert_eq!(r.revenue_up, 0.0);
            assert_eq!(r.vat_tax, 0.0);
        }

        // Khai cả năm gồm cả PX3 → 1,6 tỷ
        let year = build_tax_declaration(load_tax_agg(&pool, 2026, 0).await.unwrap(), "revenue", 2);
        let pphh_year = year.iter().find(|r| r.industry_code == "PPHH").unwrap();
        assert_eq!(pphh_year.revenue_up, 1_600_000_000.0);
    }

    #[tokio::test]
    async fn to_khai_tu_db_lap_quy() {
        // Kỳ khai theo quý 2/2026 gồm tháng 4 + 5 + 6
        let pool = test_pool().await;
        seed_product(&pool, "P1", "Hàng thường", 0.01).await;
        add_journal_px(&pool, "2026-04-10", "PX1", "P1", "PPHH", 100.0, 1_000_000.0, 100_000_000.0, 0.01, 0.005).await;
        add_journal_px(&pool, "2026-05-10", "PX2", "P1", "PPHH", 100.0, 1_000_000.0, 100_000_000.0, 0.01, 0.005).await;
        add_journal_px(&pool, "2026-06-10", "PX3", "P1", "PPHH", 100.0, 1_000_000.0, 100_000_000.0, 0.01, 0.005).await;
        // Ngoài quý 2
        add_journal_px(&pool, "2026-07-01", "PX4", "P1", "PPHH", 10.0, 1_000_000.0, 10_000_000.0, 0.01, 0.005).await;

        let out = build_tax_declaration(
            load_tax_agg_range(&pool, 2026, 4, 6).await.unwrap(),
            "revenue",
            2,
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
