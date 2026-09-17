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
    let (from, to) = if (1..=12).contains(&month) {
        (
            format!("{:04}-{:02}-01", year, month),
            format!("{:04}-{:02}-31", year, month),
        )
    } else {
        (format!("{:04}-01-01", year), format!("{:04}-12-31", year))
    };
    sqlx::query_as::<_, TaxAgg>(
        "SELECT
            ig.code AS industry_code,
            ig.name AS industry_name,
            ig.vat_rate,
            ig.pit_rate,
            COALESCE(SUM(CASE WHEN je.adjust_code <> 'GiamDT' THEN je.amount ELSE 0.0 END), 0.0) AS revenue_up,
            COALESCE(SUM(CASE WHEN je.adjust_code = 'GiamDT' THEN je.amount ELSE 0.0 END), 0.0) AS revenue_down,
            COALESCE(SUM(CASE WHEN p.vat_reduced = 1 AND je.adjust_code <> 'GiamDT'
                              THEN je.amount * je.vat_rate * 0.2 ELSE 0.0 END), 0.0) AS vat_reduced
         FROM industry_group ig
         LEFT JOIN journal_entry je
            ON je.industry_code = ig.code
           AND je.entry_type = 'PX'
           AND je.posting_date >= ?
           AND je.posting_date <= ?
         LEFT JOIN product p ON p.code = je.product_code
         GROUP BY ig.code
         ORDER BY ig.code",
    )
    .bind(&from)
    .bind(&to)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())
}

/// Tính số thuế phải nộp từ dữ liệu gộp thô (thuần → test được):
///   Thuế GTGT trước giảm = (DT tăng - DT giảm) x tỷ lệ
///   Thuế GTGT phải nộp   = Thuế GTGT trước giảm - Thuế GTGT được giảm
///                          (sản phẩm giảm thuế: DT x tỷ lệ x 20%)
///   Thuế TNCN            = (DT tăng - DT giảm) x tỷ lệ
pub(crate) fn build_tax_declaration(rows: Vec<TaxAgg>) -> Vec<TaxDeclarationRow> {
    rows.into_iter()
        .map(|r| {
            let base = r.revenue_up - r.revenue_down;
            let vat_tax = round2(base * r.vat_rate);
            TaxDeclarationRow {
                industry_code: r.industry_code,
                industry_name: r.industry_name,
                vat_rate: r.vat_rate,
                pit_rate: r.pit_rate,
                revenue_up: round2(r.revenue_up),
                revenue_down: round2(r.revenue_down),
                vat_reduced: round2(r.vat_reduced),
                vat_tax,
                vat_payable: round2(vat_tax - r.vat_reduced),
                pit_tax: round2(base * r.pit_rate),
            }
        })
        .collect()
}

/// Tờ khai thuế theo kỳ (sheet To Khai Thue) — nhóm theo nhóm ngành nghề.
/// month = 0 → khai cả năm; ngược lại khai theo tháng 1..12.
/// Số thuế GTGT phải nộp đã trừ "giảm thuế GTGT" (sản phẩm giảm thuế, 20% thuế suất).
#[tauri::command]
pub(crate) async fn get_tax_declaration(
    state: State<'_, AppState>,
    year: i64,
    month: i64,
) -> Result<String, String> {
    let rows = {
        let pool = state.pool.read().await;
        load_tax_agg(&pool, year, month).await?
    };
    let out = build_tax_declaration(rows);
    Ok(serde_json::to_string(&out).unwrap_or_default())
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

/// Bảng kê giảm thuế GTGT (sheet Giam Thue GTGT của bộ mẫu):
/// tỷ lệ sau giảm = 80% tỷ lệ quy định; thuế GTGT được giảm = DT x Tỷ lệ x 20%.
/// Chỉ liệt kê dòng bán hàng (PX) của sản phẩm có đánh dấu giảm thuế (vat_reduced).
#[tauri::command]
pub(crate) async fn get_vat_reduction_list(
    state: State<'_, AppState>,
    from_date: String,
    to_date: String,
) -> Result<String, String> {
    let rows: Vec<VatReductionRow> = sqlx::query_as::<_, VatReductionRow>(
        "SELECT je.posting_date, je.voucher_no, je.product_code,
                COALESCE(p.name, '') AS product_name,
                je.quantity, je.unit_price, je.amount, je.vat_rate,
                ROUND(je.vat_rate * 0.8, 4) AS reduced_rate,
                ROUND(je.amount * je.vat_rate * 0.2, 2) AS vat_reduced
         FROM journal_entry je
         LEFT JOIN product p ON p.code = je.product_code
         WHERE je.entry_type = 'PX' AND p.vat_reduced = 1
           AND (? = '' OR je.posting_date >= ?)
           AND (? = '' OR je.posting_date <= ?)
         ORDER BY je.posting_date ASC, je.id ASC",
    )
    .bind(&from_date)
    .bind(&from_date)
    .bind(&to_date)
    .bind(&to_date)
    .fetch_all(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    Ok(serde_json::to_string(&rows).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;

    fn agg(up: f64, down: f64, reduced: f64) -> TaxAgg {
        TaxAgg {
            industry_code: "PPHH".into(),
            industry_name: "Phân phối, cung cấp hàng hóa".into(),
            vat_rate: 0.01,
            pit_rate: 0.005,
            revenue_up: up,
            revenue_down: down,
            vat_reduced: reduced,
        }
    }

    #[test]
    fn to_khai_giam_gtgt_con_80_phan_tram() {
        // DT 100tr, ngành PPHH 1% → GTGT trước giảm 1.000.000;
        // sản phẩm giảm thuế: được giảm = DT x 1% x 20% = 200.000
        let out = build_tax_declaration(vec![agg(100_000_000.0, 0.0, 200_000.0)]);
        let r = &out[0];
        assert_eq!(r.vat_tax, 1_000_000.0);
        assert_eq!(r.vat_reduced, 200_000.0);
        assert_eq!(r.vat_payable, 800_000.0); // = 80% thuế GTGT
        assert_eq!(r.vat_payable, round2(r.vat_tax * 0.8));
        assert_eq!(r.pit_tax, 500_000.0); // 100tr x 0,5%
    }

    #[test]
    fn to_khai_tru_doanh_thu_dieu_chinh_giam() {
        // DT tăng 100tr, DT giảm 10tr → còn 90tr
        let out = build_tax_declaration(vec![agg(100_000_000.0, 10_000_000.0, 0.0)]);
        let r = &out[0];
        assert_eq!(r.revenue_up, 100_000_000.0);
        assert_eq!(r.revenue_down, 10_000_000.0);
        assert_eq!(r.vat_tax, 900_000.0); // 90tr x 1%
        assert_eq!(r.vat_payable, 900_000.0); // không có SP giảm thuế
        assert_eq!(r.pit_tax, 450_000.0); // 90tr x 0,5%
    }

    #[tokio::test]
    async fn to_khai_tu_db_tong_hop_theo_ky_va_san_pham_giam_thue() {
        let pool = test_pool().await;
        seed_product(&pool, "P1", "Hàng giảm thuế", 0.01, true).await;
        seed_product(&pool, "P2", "Hàng thường", 0.01, false).await;

        // Trong kỳ 05/2026: P1 100tr (giảm thuế) + P2 50tr
        add_journal_px(&pool, "2026-05-10", "PX1", "P1", "PPHH", 100.0, 1_000_000.0, 100_000_000.0, 0.01, 0.005).await;
        add_journal_px(&pool, "2026-05-11", "PX2", "P2", "PPHH", 50.0, 1_000_000.0, 50_000_000.0, 0.01, 0.005).await;
        // Ngoài kỳ (06/2026) — không được tính vào tờ khai tháng 5
        add_journal_px(&pool, "2026-06-01", "PX3", "P2", "PPHH", 10.0, 1_000_000.0, 10_000_000.0, 0.01, 0.005).await;

        let out = build_tax_declaration(load_tax_agg(&pool, 2026, 5).await.unwrap());
        let pphh = out.iter().find(|r| r.industry_code == "PPHH").unwrap();
        assert_eq!(pphh.revenue_up, 150_000_000.0); // chỉ PX1 + PX2
        assert_eq!(pphh.vat_reduced, 200_000.0); // P1: 100tr x 1% x 20%
        assert_eq!(pphh.vat_tax, 1_500_000.0); // 150tr x 1%
        assert_eq!(pphh.vat_payable, 1_300_000.0); // 1.500.000 - 200.000
        assert_eq!(pphh.pit_tax, 750_000.0); // 150tr x 0,5%

        // Các ngành khác không phát sinh doanh thu
        for r in out.iter().filter(|r| r.industry_code != "PPHH") {
            assert_eq!(r.revenue_up, 0.0);
            assert_eq!(r.vat_tax, 0.0);
        }

        // Khai cả năm gồm cả PX3 → 160tr
        let year = build_tax_declaration(load_tax_agg(&pool, 2026, 0).await.unwrap());
        let pphh_year = year.iter().find(|r| r.industry_code == "PPHH").unwrap();
        assert_eq!(pphh_year.revenue_up, 160_000_000.0);
    }
}
