#![allow(unused_imports)]

use crate::helpers::*;
use crate::models::*;
use serde_json::json;
use sqlx::sqlite::{SqlitePoolOptions, SqliteTransaction};
use sqlx::SqlitePool;
use tauri::{AppHandle, Manager, State};

/// Cặp tài khoản mặc định theo loại phiếu khi file Excel không khai Nợ/Có.
///
/// Sheet "NHAP LIEU" của file mẫu HKD không có cột tài khoản, mà một dòng sổ
/// thiếu tài khoản sẽ **biến mất khỏi bảng cân đối** (GROUP BY theo Nợ/Có và chỉ
/// in mã có trong danh mục TK) nên phải gán — đúng quy ước sẵn có của màn
/// Thu/Chi tiền (`cash.rs`) và Nhập/Xuất kho (`stock.rs`). Loại phiểu không
/// đoán được (`OTHER`) thì giữ nguyên.
fn default_accounts(entry_type: &str) -> Option<(&'static str, &'static str)> {
    match entry_type {
        "PN" => Some(("152", "331")), // mua ngoài: Nợ hàng hóa / Có phải trả
        "PX" => Some(("131", "511")), // bán hàng: Nợ phải thu / Có doanh thu
        "PT" => Some(("111", "511")), // thu tiền: Nợ tiền mặt / Có doanh thu
        "PC" => Some(("642", "111")), // chi tiền: Nợ chi phí / Có tiền mặt
        _ => None,
    }
}

/// Nhập khối NHAP LIEU (sheet NHAP LIEU của file mẫu HKD) vào journal_entry.
/// Không tạo stock_lot như PNK/PXK thủ công — dữ liệu lịch sử chỉ phục vụ sổ sách
/// (tồn kho tổng hợp theo bút toán PN/PX; giá vốn FIFO cho các phiếu xuất SAU đó
/// vẫn dựa trên stock_lot được tạo từ màn hình Nhập kho).
#[tauri::command]
pub(crate) async fn import_nhap_lieu(
    state: State<'_, AppState>,
    items: Vec<ImportEntryInput>,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    if items.is_empty() {
        return Err("Không có dòng dữ liệu nào để nhập".into());
    }
    let pool = state.pool.read().await;
    let (imported, skipped) = import_nhap_lieu_core(&pool, &items).await?;
    audit(
        &state,
        "import",
        "nhap_lieu",
        &format!("{} dòng (bỏ qua {})", imported, skipped),
    )
    .await;
    Ok(json!({ "ok": true, "imported": imported, "skipped": skipped }).to_string())
}

/// Phần lõi của [`import_nhap_lieu`] — tách ra để test được với `test_pool`.
pub(crate) async fn import_nhap_lieu_core(
    pool: &SqlitePool,
    items: &[ImportEntryInput],
) -> Result<(i64, i64), String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let mut imported = 0i64;
    let mut skipped = 0i64;
    for it in items {
        if it.voucher_no.is_empty() || it.posting_date.is_empty() {
            skipped += 1;
            continue;
        }
        // Số phiếu là định danh chứng từ (xem `helpers::voucher_no_taken`): nhập
        // lại file lần hai không được nhân đôi mọi bút toán trong sổ.
        let taken: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM journal_entry WHERE voucher_no = ? COLLATE NOCASE",
            it.voucher_no
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        if taken > 0 {
            skipped += 1;
            continue;
        }
        // insert trực tiếp để lưu được cả Ngày chứng từ (doc_date) và Kỳ khai thuế (tax_period)
        let doc_date = if it.doc_date.is_empty() {
            it.posting_date.clone()
        } else {
            it.doc_date.clone()
        };
        let unit_code = if it.unit_code.is_empty() {
            "HKD".to_string()
        } else {
            it.unit_code.clone()
        };
        // TK rỗng → cặp mặc định theo loại phiếu (xem `default_accounts`).
        let (debit, credit) = match default_accounts(&it.entry_type) {
            Some((d, c)) => (
                if it.debit_account.trim().is_empty() {
                    d.to_string()
                } else {
                    it.debit_account.clone()
                },
                if it.credit_account.trim().is_empty() {
                    c.to_string()
                } else {
                    it.credit_account.clone()
                },
            ),
            None => (it.debit_account.clone(), it.credit_account.clone()),
        };
        sqlx::query!(
            "INSERT INTO journal_entry
             (posting_date, voucher_no, doc_date, description, product_code,
              supplier_code, customer_code, quantity, unit_price, amount, entry_type,
              debit_account, credit_account, industry_code, vat_rate, pit_rate,
              tax_period, unit_code, adjust_code, note)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            it.posting_date,
            it.voucher_no,
            doc_date,
            it.description,
            it.product_code,
            it.supplier_code,
            it.customer_code,
            it.quantity,
            it.unit_price,
            it.amount,
            it.entry_type,
            debit,
            credit,
            it.industry_code,
            it.vat_rate,
            it.pit_rate,
            it.tax_period,
            unit_code,
            it.adjust_code,
            it.note
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        imported += 1;
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok((imported, skipped))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;

    /// Một dòng dữ liệu đúng như sheet NHAP LIEU: không khai tài khoản.
    fn item(voucher_no: &str, entry_type: &str) -> ImportEntryInput {
        ImportEntryInput {
            posting_date: "2026-01-06".into(),
            voucher_no: voucher_no.into(),
            doc_date: "".into(),
            entry_type: entry_type.into(),
            description: "Nhập từ Excel".into(),
            product_code: "".into(),
            supplier_code: "".into(),
            customer_code: "KH001".into(),
            quantity: 2.0,
            unit_price: 100_000.0,
            amount: 200_000.0,
            debit_account: "".into(),
            credit_account: "".into(),
            industry_code: "PPHH".into(),
            vat_rate: 0.0,
            pit_rate: 0.0,
            tax_period: 202601,
            unit_code: "".into(),
            adjust_code: "".into(),
            note: "".into(),
        }
    }

    async fn accounts(pool: &SqlitePool, voucher_no: &str) -> (String, String) {
        let debit: String =
            sqlx::query_scalar("SELECT debit_account FROM journal_entry WHERE voucher_no = ?")
                .bind(voucher_no)
                .fetch_one(pool)
                .await
                .expect("dòng sổ");
        let credit: String =
            sqlx::query_scalar("SELECT credit_account FROM journal_entry WHERE voucher_no = ?")
                .bind(voucher_no)
                .fetch_one(pool)
                .await
                .expect("dòng sổ");
        (debit, credit)
    }

    #[test]
    fn default_accounts_theo_loai_phieu() {
        assert_eq!(default_accounts("PN"), Some(("152", "331")));
        assert_eq!(default_accounts("PX"), Some(("131", "511")));
        assert_eq!(default_accounts("PT"), Some(("111", "511")));
        assert_eq!(default_accounts("PC"), Some(("642", "111")));
        // Không đoán được loại lạ → giữ nguyên để người dùng tự sửa.
        assert_eq!(default_accounts("OTHER"), None);
    }

    #[tokio::test]
    async fn import_gan_tai_khoan_mac_dinh_khi_file_khong_khai() {
        let pool = test_pool().await;
        let items = vec![
            item("PN900", "PN"),
            item("PX900", "PX"),
            item("PT900", "PT"),
            item("PC900", "PC"),
        ];
        let (imported, skipped) = import_nhap_lieu_core(&pool, &items).await.expect("nhập");
        assert_eq!((imported, skipped), (4, 0));

        // Dòng thiếu TK trước đây rơi khỏi bảng cân đối → nay có cặp TK đúng.
        assert_eq!(accounts(&pool, "PN900").await, ("152".into(), "331".into()));
        assert_eq!(accounts(&pool, "PX900").await, ("131".into(), "511".into()));
        assert_eq!(accounts(&pool, "PT900").await, ("111".into(), "511".into()));
        assert_eq!(accounts(&pool, "PC900").await, ("642".into(), "111".into()));
    }

    #[tokio::test]
    async fn import_lan_hai_bi_bo_qua_khong_nhan_doi_so() {
        let pool = test_pool().await;
        let items = vec![item("PN901", "PN"), item("PX901", "PX")];
        assert_eq!(
            import_nhap_lieu_core(&pool, &items)
                .await
                .expect("nhập lần 1"),
            (2, 0)
        );
        // Nhập lại cùng file: không thêm dòng nào, tất cả bị bỏ qua.
        assert_eq!(
            import_nhap_lieu_core(&pool, &items)
                .await
                .expect("nhập lần 2"),
            (0, 2)
        );
        assert_eq!(
            scalar_i64(
                &pool,
                "SELECT COUNT(*) FROM journal_entry WHERE voucher_no IN ('PN901', 'PX901')"
            )
            .await,
            2
        );
    }

    #[tokio::test]
    async fn import_bo_qua_dong_thieu_so_phieu_hoac_ngay() {
        let pool = test_pool().await;
        let mut blank = item("", "PX");
        blank.amount = 0.0;
        let mut no_date = item("PX902", "PX");
        no_date.posting_date = "".into();
        let (imported, skipped) = import_nhap_lieu_core(&pool, &[blank, no_date])
            .await
            .expect("nhập");
        assert_eq!((imported, skipped), (0, 2));
    }
}
