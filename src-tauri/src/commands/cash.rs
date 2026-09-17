#![allow(unused_imports)]

use crate::helpers::*;
use crate::models::*;
use serde_json::json;
use sqlx::sqlite::{SqlitePoolOptions, SqliteTransaction};
use sqlx::SqlitePool;
use tauri::{State, Manager, AppHandle};
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
    let unit_code = if input.unit_code.is_empty() { "HaNoi-01" } else { &input.unit_code };

    let pool = state.pool.read().await;
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
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
        if input.debit_account.is_empty() {
            if input.entry_type == "PT" { "111" } else { "642" }
        } else {
            &input.debit_account
        },
        if input.credit_account.is_empty() {
            if input.entry_type == "PT" { "511" } else { "111" }
        } else {
            &input.credit_account
        },
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