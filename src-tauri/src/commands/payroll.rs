#![allow(unused_imports)]

use crate::helpers::*;
use crate::models::*;
use serde_json::json;
use sqlx::sqlite::{SqlitePoolOptions, SqliteTransaction};
use sqlx::SqlitePool;
use tauri::{State, Manager, AppHandle};

/// Tỷ lệ trích bảo hiểm theo bộ mẫu Excel (sheet Bang Luong / So Luong-BH):
/// - Trích tính vào chi phí HKD (NSDLĐ): BHXH 17,5% + BHYT 3,0% + BHTN 1,0% = 21,5%
/// - Trích khấu trừ vào lương NLĐ:       BHXH 8,0%  + BHYT 1,5% + BHTN 1,0% = 10,5%
const EMPLOYER_BH_RATE: f64 = 0.215;
const EMPLOYEE_BH_RATE: f64 = 0.105;
/// Số công chuẩn trong tháng (bộ mẫu Excel dùng 26 ngày)
const STANDARD_MONTH_DAYS: f64 = 26.0;

/// Kết quả tính 1 dòng lương.
#[derive(Debug)]
pub(crate) struct PayrollCalc {
    pub(crate) gross: f64,
    pub(crate) bh_employer: f64,
    pub(crate) bh_employee: f64,
    pub(crate) net: f64,
}

/// Công thức lương 1 dòng (khớp bộ mẫu Excel):
///   Lương thời gian = basic_salary / 26 x work_days
///   gross = Lương thời gian + phụ cấp (CV, xăng xe, ĐT) + thưởng
///   Căn cứ đóng BH = bh_salary nếu > 0, ngược lại lấy gross
///   net = gross - BH người LĐ (10,5%) - thuế TNCN - tạm ứng
pub(crate) fn calc_payroll_line(
    basic_salary: f64,
    allowance_cv: f64,
    allowance_xx: f64,
    allowance_phone: f64,
    bh_salary: f64,
    work_days: f64,
    bonus: f64,
    pit_amount: f64,
    advance: f64,
) -> PayrollCalc {
    let time_salary = basic_salary / STANDARD_MONTH_DAYS * work_days;
    let gross = round2(time_salary + allowance_cv + allowance_xx + allowance_phone + bonus);
    let bh_base = if bh_salary > 0.0 { bh_salary } else { gross };
    let bh_employer = round2(bh_base * EMPLOYER_BH_RATE);
    let bh_employee = round2(bh_base * EMPLOYEE_BH_RATE);
    let net = round2(gross - bh_employee - pit_amount - advance);
    PayrollCalc {
        gross,
        bh_employer,
        bh_employee,
        net,
    }
}

/// Kết quả lưu bảng lương 1 kỳ.
#[derive(Debug)]
pub(crate) struct PayrollSaveResult {
    pub(crate) rows: i64,
    pub(crate) total_net: f64,
    pub(crate) total_employer: f64,
}

/// Lõi lập/sửa bảng lương 1 kỳ (không phụ thuộc Tauri State → test trực tiếp):
/// tự tính lương thời gian, BH, thực lĩnh và ghi 1 phiếu chi lương (PC) tổng
/// vào sổ nhật ký nếu kỳ chưa có.
pub(crate) async fn save_payroll_core(
    pool: &SqlitePool,
    period: &str,
    items: &[PayrollLineInput],
) -> Result<PayrollSaveResult, String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    let mut total_net = 0.0;
    let mut total_employer = 0.0;
    let mut count = 0i64;
    for item in items {
        let emp: (i64, f64, f64, f64, f64, f64) = sqlx::query_as(
            "SELECT id, basic_salary, allowance_cv, allowance_xx, allowance_phone, bh_salary
             FROM employee WHERE code = ?",
        )
        .bind(&item.employee_code)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| format!("Không tìm thấy nhân viên '{}'", item.employee_code))?;

        // Lương thời gian = lương HĐ / 26 x Số công; phụ cấp lấy từ thẻ nhân viên
        let c = calc_payroll_line(
            emp.1,
            emp.2,
            emp.3,
            emp.4,
            emp.5,
            item.work_days,
            item.bonus,
            item.pit_amount,
            item.advance,
        );

        sqlx::query(
            "INSERT INTO payroll (period, employee_id, work_days, bonus, advance, pit_amount,
                     gross_salary, bh_employer, bh_employee, net_pay, note)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(period, employee_id) DO UPDATE SET
                work_days = excluded.work_days, bonus = excluded.bonus,
                advance = excluded.advance, pit_amount = excluded.pit_amount,
                gross_salary = excluded.gross_salary, bh_employer = excluded.bh_employer,
                bh_employee = excluded.bh_employee, net_pay = excluded.net_pay,
                note = excluded.note",
        )
        .bind(period)
        .bind(emp.0)
        .bind(item.work_days)
        .bind(item.bonus)
        .bind(item.advance)
        .bind(item.pit_amount)
        .bind(c.gross)
        .bind(c.bh_employer)
        .bind(c.bh_employee)
        .bind(c.net)
        .bind(&item.note)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

        total_net += c.net;
        total_employer += c.bh_employer;
        count += 1;
    }

    // Phiếu chi lương tổng (PC: Nợ 642 / Có 111) — tự sinh nếu kỳ chưa có
    let voucher = format!("LUONG-{}", period);
    let existing: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM journal_entry WHERE voucher_no = ?",
    )
    .bind(&voucher)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    let desc = format!("Chi lương tháng {}", period);
    if existing == 0 {
        insert_journal_entry(
            &mut tx,
            &format!("{}-28", period),
            &voucher,
            "PC",
            &desc,
            "",
            "",
            "",
            1.0,
            round2(total_net),
            round2(total_net),
            "642",
            "111",
            "",
            0.0,
            0.0,
            "HKD",
            "",
            "Tự sinh từ bảng lương",
        )
        .await?;
    } else {
        sqlx::query(
            "UPDATE journal_entry SET amount = ?, unit_price = ?, description = ?
             WHERE voucher_no = ?",
        )
        .bind(round2(total_net))
        .bind(round2(total_net))
        .bind(&desc)
        .bind(&voucher)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    }

    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(PayrollSaveResult {
        rows: count,
        total_net: round2(total_net),
        total_employer: round2(total_employer),
    })
}

/// Danh sách nhân viên (sheet Nhan Vien)
#[tauri::command]
pub(crate) async fn get_employees(state: State<'_, AppState>) -> Result<String, String> {
    let rows: Vec<EmployeeRow> = sqlx::query_as::<_, EmployeeRow>(
        "SELECT id, code, name, department, position, job, basic_salary,
                allowance_cv, allowance_xx, allowance_phone, bh_salary,
                hired_on, left_on, dependents
         FROM employee ORDER BY code",
    )
    .fetch_all(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    Ok(serde_json::to_string(&rows).unwrap_or_default())
}

/// Lưu / sửa nhân viên (upsert theo mã)
#[tauri::command]
pub(crate) async fn save_employee(
    state: State<'_, AppState>,
    input: EmployeeInput,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    if input.code.is_empty() || input.name.is_empty() {
        return Err("Thiếu mã hoặc tên nhân viên".into());
    }
    sqlx::query(
        "INSERT INTO employee (code, name, department, position, job, basic_salary,
                allowance_cv, allowance_xx, allowance_phone, bh_salary,
                hired_on, left_on, dependents)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(code) DO UPDATE SET
            name = excluded.name, department = excluded.department,
            position = excluded.position, job = excluded.job,
            basic_salary = excluded.basic_salary,
            allowance_cv = excluded.allowance_cv,
            allowance_xx = excluded.allowance_xx,
            allowance_phone = excluded.allowance_phone,
            bh_salary = excluded.bh_salary,
            hired_on = excluded.hired_on, left_on = excluded.left_on,
            dependents = excluded.dependents",
    )
    .bind(&input.code)
    .bind(&input.name)
    .bind(&input.department)
    .bind(&input.position)
    .bind(&input.job)
    .bind(input.basic_salary)
    .bind(input.allowance_cv)
    .bind(input.allowance_xx)
    .bind(input.allowance_phone)
    .bind(input.bh_salary)
    .bind(&input.hired_on)
    .bind(&input.left_on)
    .bind(input.dependents)
    .execute(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    audit(&state, "save", "employee", &input.code).await;
    Ok("ok".into())
}

/// Các kỳ lương đã lập (YYYY-MM), mới nhất trước
#[tauri::command]
pub(crate) async fn get_payroll_periods(state: State<'_, AppState>) -> Result<String, String> {
    let rows: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT period FROM payroll ORDER BY period DESC")
            .fetch_all(&*state.pool.read().await)
            .await
            .map_err(|e| e.to_string())?;
    Ok(serde_json::to_string(&rows).unwrap_or_default())
}

/// Bảng lương một kỳ (sheet Bang Luong)
#[tauri::command]
pub(crate) async fn get_payroll(
    state: State<'_, AppState>,
    period: String,
) -> Result<String, String> {
    let rows: Vec<PayrollRow> = sqlx::query_as::<_, PayrollRow>(
        "SELECT p.period, e.code AS employee_code, e.name AS employee_name,
                e.department, p.work_days, p.bonus, p.advance, p.pit_amount,
                p.gross_salary, p.bh_employer, p.bh_employee, p.net_pay, p.note
         FROM payroll p
         JOIN employee e ON e.id = p.employee_id
         WHERE p.period = ?
         ORDER BY e.code",
    )
    .bind(&period)
    .fetch_all(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    Ok(serde_json::to_string(&rows).unwrap_or_default())
}

/// Lập / sửa bảng lương một kỳ — tự tính lương thời gian, BH, thực lĩnh
/// và ghi 1 phiếu chi lương (PC) vào sổ nhật ký nếu kỳ chưa có.
#[tauri::command]
pub(crate) async fn save_payroll(
    state: State<'_, AppState>,
    period: String,
    items: Vec<PayrollLineInput>,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    if period.is_empty() {
        return Err("Thiếu kỳ lương (dạng YYYY-MM)".into());
    }
    if items.is_empty() {
        return Err("Chưa có nhân viên nào trong bảng lương".into());
    }
    let out = {
        let pool = state.pool.read().await;
        save_payroll_core(&pool, &period, &items).await?
    };
    audit(
        &state,
        "save",
        "payroll",
        &format!("{} ({} NV)", period, out.rows),
    )
    .await;
    Ok(json!({
        "ok": true,
        "period": period,
        "rows": out.rows,
        "total_net": out.total_net,
        "total_employer": out.total_employer,
    })
    .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;

    fn line(code: &str, work_days: f64) -> PayrollLineInput {
        PayrollLineInput {
            employee_code: code.into(),
            work_days,
            bonus: 0.0,
            advance: 0.0,
            pit_amount: 0.0,
            note: String::new(),
        }
    }

    #[test]
    fn payroll_formula_matches_excel() {
        // Lương HĐ 5.000.000, đủ 26 công, phụ cấp 500k+300k+200k, lương đóng BH 5.000.000
        let c = calc_payroll_line(5_000_000.0, 500_000.0, 300_000.0, 200_000.0, 5_000_000.0, 26.0, 0.0, 0.0, 0.0);
        assert_eq!(c.gross, 6_000_000.0);
        assert_eq!(c.bh_employer, 1_075_000.0); // 5.000.000 x 21,5%
        assert_eq!(c.bh_employee, 525_000.0); // 5.000.000 x 10,5%
        assert_eq!(c.net, 5_475_000.0); // 6.000.000 - 525.000
    }

    #[test]
    fn payroll_bh_base_falls_back_to_gross() {
        // bh_salary = 0 → căn cứ đóng BH = gross (1.300.000)
        let c = calc_payroll_line(2_600_000.0, 0.0, 0.0, 0.0, 0.0, 13.0, 0.0, 0.0, 0.0);
        assert_eq!(c.gross, 1_300_000.0); // 2.600.000 / 26 x 13
        assert_eq!(c.bh_employer, 279_500.0); // 1.300.000 x 21,5%
        assert_eq!(c.bh_employee, 136_500.0); // 1.300.000 x 10,5%
        assert_eq!(c.net, 1_163_500.0);
    }

    #[test]
    fn payroll_deducts_pit_and_advance() {
        let c = calc_payroll_line(5_000_000.0, 0.0, 0.0, 0.0, 0.0, 26.0, 1_000_000.0, 200_000.0, 500_000.0);
        assert_eq!(c.gross, 6_000_000.0); // 5.000.000 + thưởng 1.000.000
        assert_eq!(c.bh_employee, 630_000.0); // 6.000.000 x 10,5%
        assert_eq!(c.net, 4_670_000.0); // 6.000.000 - 630.000 - 200.000 - 500.000
    }

    #[tokio::test]
    async fn save_payroll_persists_row_and_auto_voucher() {
        let pool = test_pool().await;
        seed_employee(&pool, "E1", "Nguyễn Văn A", 5_000_000.0, 500_000.0, 300_000.0, 200_000.0, 5_000_000.0).await;

        let out = save_payroll_core(&pool, "2026-05", &[line("E1", 26.0)])
            .await
            .expect("save payroll");

        assert_eq!(out.rows, 1);
        assert_eq!(out.total_net, 5_475_000.0);
        assert_eq!(out.total_employer, 1_075_000.0);

        let (gross, bh_emp, net): (f64, f64, f64) = sqlx::query_as(
            "SELECT gross_salary, bh_employee, net_pay FROM payroll WHERE period = '2026-05'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!((gross, bh_emp, net), (6_000_000.0, 525_000.0, 5_475_000.0));

        // Tự sinh 1 phiếu chi lương tổng: PC Nợ 642 / Có 111
        let (etype, debit, credit, amount, date): (String, String, String, f64, String) = sqlx::query_as(
            "SELECT entry_type, debit_account, credit_account, amount, posting_date
             FROM journal_entry WHERE voucher_no = 'LUONG-2026-05'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(etype, "PC");
        assert_eq!(debit, "642");
        assert_eq!(credit, "111");
        assert_eq!(amount, 5_475_000.0);
        assert_eq!(date, "2026-05-28");
    }

    #[tokio::test]
    async fn save_payroll_is_idempotent_per_period() {
        let pool = test_pool().await;
        seed_employee(&pool, "E1", "Nguyễn Văn A", 5_000_000.0, 500_000.0, 300_000.0, 200_000.0, 5_000_000.0).await;

        save_payroll_core(&pool, "2026-05", &[line("E1", 26.0)]).await.unwrap();
        // Lần 2 với số công khác → cập nhật, KHÔNG thêm dòng/phiếu mới
        let out = save_payroll_core(&pool, "2026-05", &[line("E1", 13.0)]).await.unwrap();
        assert_eq!(out.rows, 1);

        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM payroll WHERE period = '2026-05'")
            .fetch_one(&pool).await.unwrap();
        let vouchers: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM journal_entry WHERE voucher_no = 'LUONG-2026-05'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(rows, 1);
        assert_eq!(vouchers, 1);

        // Số công 13 → gross 2.500.000 + 1.000.000 phụ cấp = 3.500.000; net = 3.500.000 - 525.000
        let (net, amount): (f64, f64) = (
            sqlx::query_scalar("SELECT net_pay FROM payroll WHERE period = '2026-05'")
                .fetch_one(&pool).await.unwrap(),
            sqlx::query_scalar("SELECT amount FROM journal_entry WHERE voucher_no = 'LUONG-2026-05'")
                .fetch_one(&pool).await.unwrap(),
        );
        assert_eq!(net, 2_975_000.0);
        assert_eq!(amount, 2_975_000.0);
    }

    #[tokio::test]
    async fn save_payroll_rejects_unknown_employee() {
        let pool = test_pool().await;
        let err = save_payroll_core(&pool, "2026-05", &[line("NOPE", 26.0)])
            .await
            .unwrap_err();
        assert!(err.contains("Không tìm thấy"));
    }
}
