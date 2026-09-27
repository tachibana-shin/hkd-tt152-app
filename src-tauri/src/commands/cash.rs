#![allow(unused_imports)]

use crate::helpers::*;
use crate::models::*;
use serde_json::json;
use sqlx::sqlite::{SqlitePoolOptions, SqliteTransaction};
use sqlx::SqlitePool;
use tauri::{AppHandle, Manager, State};
/// Phiếu thu (PT) / phiếu chi (PC) — ghi bút toán tiền vào `journal_entry`
/// (đối chiếu sheet P.THU / P.CHI của bộ mẫu Excel HKD).
/// Khác với PNK/PXK: không tạo `stock_lot`, chỉ ghi sổ nhật ký.
#[tauri::command]
pub(crate) async fn save_cash_entry(
    state: State<'_, AppState>,
    input: CashEntryInput,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    if input.entry_type != "PT" && input.entry_type != "PC" {
        return Err("Loại phiếu phải là PT (phiếu thu) hoặc PC (phiếu chi)".into());
    }
    if input.amount <= 0.0 {
        return Err("Số tiền phải lớn hơn 0".into());
    }
    if input.posting_date.is_empty() || input.voucher_no.is_empty() {
        return Err("Thiếu ngày ghi sổ hoặc số phiếu".into());
    }
    let unit_code = if input.unit_code.is_empty() {
        "HKD"
    } else {
        &input.unit_code
    };

    let pool = state.pool.read().await;

    // Tài khoản Nợ/Có (kể cả mặc định theo loại phiếu) phải có trong danh mục
    // tài khoản (màn Tài khoản) — đảm bảo hạch toán dùng đúng DMTK của hộ.
    let debit: &str = if input.debit_account.trim().is_empty() {
        if input.entry_type == "PT" {
            "111"
        } else {
            "642"
        }
    } else {
        &input.debit_account
    };
    let credit: &str = if input.credit_account.trim().is_empty() {
        if input.entry_type == "PT" {
            "511"
        } else {
            "111"
        }
    } else {
        &input.credit_account
    };
    for code in [debit, credit] {
        let exists: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM account WHERE code = ?", code)
            .fetch_one(&*pool)
            .await
            .map_err(|e| e.to_string())?;
        if exists == 0 {
            return Err(format!(
                "Tài khoản '{}' chưa có trong danh mục tài khoản — hãy thêm ở màn Tài khoản trước khi lập phiếu",
                code
            ));
        }
    }

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    // Số phiếu thu/chi là định danh chứng từ — trùng số thì báo lỗi thay vì gộp
    // hai khoản tiền vào một số.
    if voucher_no_taken(&mut tx, &input.entry_type, &input.voucher_no).await? {
        let loai = if input.entry_type == "PT" {
            "phiếu thu"
        } else {
            "phiếu chi"
        };
        return Err(format!(
            "Số {} {} đã tồn tại",
            loai,
            input.voucher_no.trim()
        ));
    }
    insert_journal_entry(
        &mut tx,
        &input.posting_date,
        &input.voucher_no,
        &input.entry_type,
        &input.description,
        "",
        &input.supplier_code,
        &input.customer_code,
        1.0,
        input.amount,
        input.amount,
        debit,
        credit,
        &input.industry_code,
        input.vat_rate,
        input.pit_rate,
        unit_code,
        "",
        &input.note,
    )
    .await?;
    tx.commit().await.map_err(|e| e.to_string())?;
    audit(
        &state,
        "save",
        "cash",
        &format!("{} {} đ", input.entry_type, input.amount),
    )
    .await;
    Ok(json!({ "ok": true, "amount": input.amount, "entry_type": input.entry_type }).to_string())
}
