#![allow(unused_imports)]

use crate::helpers::*;
use crate::models::*;
use serde_json::json;
use sqlx::sqlite::{SqlitePoolOptions, SqliteTransaction};
use sqlx::SqlitePool;
use tauri::{AppHandle, Manager, State};

/// Chấm công theo ngày trong kỳ (period = 'YYYY-MM'), khớp sheet Cham Cong.
/// Trả về các dòng đã chấm; lưới theo ngày do frontend dựng từ danh sách nhân viên.
#[tauri::command]
pub(crate) async fn get_attendance(
    state: State<'_, AppState>,
    period: String,
) -> Result<String, String> {
    if period.is_empty() {
        return Err("Thiếu kỳ chấm công (dạng YYYY-MM)".into());
    }
    let rows: Vec<AttendanceRow> = sqlx::query_as::<_, AttendanceRow>(
        "SELECT a.work_date, e.code AS employee_code, e.name AS employee_name, a.status
         FROM attendance a
         JOIN employee e ON e.id = a.employee_id
         WHERE a.work_date LIKE ? || '-%'
         ORDER BY a.work_date ASC, e.code ASC",
    )
    .bind(&period)
    .fetch_all(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    Ok(serde_json::to_string(&rows).unwrap_or_default())
}

/// Lưu chấm công một kỳ — thay thế TOÀN BỘ dữ liệu của kỳ (gửi đủ lưới).
/// entry có status rỗng (hoặc trống) sẽ bị bỏ qua (tức là nghỉ/chưa chấm).
#[tauri::command]
pub(crate) async fn save_attendance(
    state: State<'_, AppState>,
    period: String,
    entries: Vec<AttendanceEntryInput>,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    if period.is_empty() || period.len() < 7 {
        return Err("Thiếu kỳ chấm công (dạng YYYY-MM)".into());
    }
    let pool = state.pool.read().await;
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    // Xóa dữ liệu cũ của kỳ, sau đó ghi lại các ô đã chấm
    sqlx::query("DELETE FROM attendance WHERE work_date LIKE ? || '-%'")
        .bind(&period)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

    let mut saved = 0i64;
    for e in &entries {
        let status = e.status.trim();
        if status.is_empty() {
            continue;
        }
        if !e.work_date.starts_with(&period) {
            return Err(format!(
                "Ngày '{}' không thuộc kỳ '{}'",
                e.work_date, period
            ));
        }
        let emp_id: i64 = sqlx::query_scalar("SELECT id FROM employee WHERE code = ?")
            .bind(&e.employee_code)
            .fetch_one(&mut *tx)
            .await
            .map_err(|_| format!("Không tìm thấy nhân viên '{}'", e.employee_code))?;
        sqlx::query("INSERT INTO attendance (work_date, employee_id, status) VALUES (?, ?, ?)")
            .bind(&e.work_date)
            .bind(emp_id)
            .bind(status)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        saved += 1;
    }

    tx.commit().await.map_err(|e| e.to_string())?;
    audit(&state, "save", "attendance", &period).await;
    Ok(json!({ "ok": true, "period": period, "saved": saved }).to_string())
}

/// Số công mỗi nhân viên trong kỳ — nạp sẵn Số công khi lập bảng lương.
/// Quy ước công (khớp cột tổng của sheet Cham Cong):
/// X/P/H/CT/TS/O/CO = 1 ngày công; NC = 0,5 ngày; KL và ô trống = 0.
/// Trả về CẢ nhân viên không có công (work_days = 0) để nạp đúng 0 công
/// (nghỉ cả tháng / toàn KL) thay vì bỏ sót.
#[tauri::command]
pub(crate) async fn get_attendance_work_days(
    state: State<'_, AppState>,
    period: String,
) -> Result<String, String> {
    if period.is_empty() {
        return Err("Thiếu kỳ chấm công (dạng YYYY-MM)".into());
    }
    let rows: Vec<AttendanceWorkDayRow> = sqlx::query_as::<_, AttendanceWorkDayRow>(
        "SELECT e.code AS employee_code, e.name AS employee_name,
                ROUND(SUM(CASE WHEN a.status IN ('X','P','H','CT','TS','O','CO') THEN 1.0
                               WHEN a.status = 'NC' THEN 0.5 ELSE 0.0 END), 1) AS work_days
         FROM employee e
         LEFT JOIN attendance a ON a.employee_id = e.id AND a.work_date LIKE ? || '-%'
         GROUP BY e.id
         ORDER BY e.code",
    )
    .bind(&period)
    .fetch_all(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    Ok(serde_json::to_string(&rows).unwrap_or_default())
}
