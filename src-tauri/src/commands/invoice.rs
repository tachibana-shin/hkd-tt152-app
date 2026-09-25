#![allow(unused_imports)]

use crate::helpers::*;
use crate::models::*;
use serde_json::json;
use sqlx::sqlite::{SqlitePoolOptions, SqliteTransaction};
use sqlx::SqlitePool;
use tauri::{AppHandle, Manager, State};
#[tauri::command]
pub(crate) async fn get_invoices(state: State<'_, AppState>) -> Result<String, String> {
    let rows: Vec<InvoiceRow> = sqlx::query_as!(
        InvoiceRow,
        "SELECT id, number, date, customer, customer_tax_code, total, vat_amount,
                status, e_invoice_no, e_invoice_symbol, e_invoice_date, voucher_no,
                exported_at, cancel_reason, adjust_reason, ref_invoice, adjust_voucher_no
         FROM invoice ORDER BY date DESC, id DESC"
    )
    .fetch_all(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    Ok(serde_json::to_string(&rows).unwrap_or_default())
}

#[tauri::command]
pub(crate) async fn get_invoice_detail(
    state: State<'_, AppState>,
    id: i64,
) -> Result<String, String> {
    let inv: InvoiceRow = sqlx::query_as!(
        InvoiceRow,
        "SELECT id, number, date, customer, customer_tax_code, total, vat_amount,
                status, e_invoice_no, e_invoice_symbol, e_invoice_date, voucher_no,
                exported_at, cancel_reason, adjust_reason, ref_invoice, adjust_voucher_no
         FROM invoice WHERE id = ?",
        id
    )
    .fetch_one(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    let items: Vec<InvoiceItemRow> = sqlx::query_as::<_, InvoiceItemRow>(
        "SELECT ii.id, ii.invoice_id, p.code AS product_code, p.name AS product_name,
                p.unit, ii.quantity, ii.unit_price, ii.subtotal,
                ii.discount, ii.warehouse_code,
                ii.industry_code, ii.vat_rate, ii.pit_rate
         FROM invoice_item ii
         JOIN product p ON p.id = ii.product_id
         WHERE ii.invoice_id = ?
         ORDER BY ii.id",
    )
    .bind(id)
    .fetch_all(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "invoice": inv, "items": items }).to_string())
}

/// Nghiệp vụ lập hóa đơn nháp — tách riêng để test trực tiếp (không cần tauri::State).
/// Kiểm tra tồn kho theo ĐÚNG kho xuất trên từng dòng (rỗng → tổng mọi kho),
/// giá trị dòng = Thành tiền (SL × Đơn giá) − Tiền CK (chiết khấu).
async fn save_invoice_core(
    pool: &SqlitePool,
    number: &str,
    date: &str,
    customer: &str,
    customer_tax_code: &str,
    items: &[InvoiceItemInput],
) -> Result<serde_json::Value, String> {
    if items.is_empty() {
        return Err("Chưa có mặt hàng nào trong hóa đơn".into());
    }
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    let mut basis: Vec<(String, f64, f64)> = Vec::with_capacity(items.len());
    let mut shortage: Vec<String> = Vec::new();
    for item in items {
        let product = resolve_product(&mut tx, &item.product_code).await?;

        // Nhóm ngành trên dòng: ưu tiên khai trên dòng, rỗng → nhóm ngành
        // mặc định của sản phẩm (cơ sở tỷ lệ thuế bán ra).
        let industry = if item.industry_code.trim().is_empty() {
            product.industry_code.clone()
        } else {
            item.industry_code.trim().to_string()
        };
        if industry.is_empty() {
            return Err(format!(
                "Sản phẩm '{}' chưa có nhóm ngành — hãy gán nhóm ngành cho sản phẩm (màn Sản phẩm) hoặc chọn nhóm ngành trên dòng",
                item.product_code
            ));
        }
        let group = sqlx::query!(
            "SELECT vat_rate, pit_rate FROM industry_group WHERE code = ?",
            industry
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| format!("Nhóm ngành '{}' không hợp lệ", industry))?;
        let (vat_rate, pit_rate) = (group.vat_rate, group.pit_rate);
        basis.push((industry, vat_rate, pit_rate));

        // Sản phẩm dịch vụ (nhân công...) không theo dõi tồn kho → bỏ qua kiểm tra.
        if product.is_service {
            continue;
        }
        // Kiểm tra tồn theo ĐÚNG kho xuất trên dòng (rỗng → tổng mọi kho).
        let available: f64 = if item.warehouse_code.trim().is_empty() {
            sqlx::query_scalar!(
                r#"SELECT COALESCE(SUM(quantity), 0.0) as "qty!: f64" FROM stock_lot
                   WHERE product_id = ? AND depleted = 0"#,
                product.id
            )
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| e.to_string())?
        } else {
            let wh_id =
                resolve_warehouse(&mut tx, &item.warehouse_code, &item.product_code).await?;
            sqlx::query_scalar!(
                r#"SELECT COALESCE(SUM(quantity), 0.0) as "qty!: f64" FROM stock_lot
                   WHERE product_id = ? AND warehouse_id = ? AND depleted = 0"#,
                product.id,
                wh_id
            )
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| e.to_string())?
        };
        let where_str = if item.warehouse_code.trim().is_empty() {
            "tổng các kho"
        } else {
            "kho đã chọn"
        };
        if item.quantity - available > 1e-9 {
            shortage.push(format!(
                "{}: cần {:.2}, tồn {} {:.2}",
                item.product_code, item.quantity, where_str, available
            ));
        }
    }
    if !shortage.is_empty() {
        return Err(format!("Không đủ tồn kho: {}", shortage.join("; ")));
    }

    // Hóa đơn bán hàng của HKD không tách thuế GTGT (không ghi thuế trên hóa đơn).
    // Số thuế phải nộp tính theo tỷ lệ nhóm ngành × doanh thu (tax_payable) để
    // theo dõi; trường "Thuế suất GTGT" của sản phẩm là thuế đầu vào, không dùng.
    let mut total = 0.0;
    let mut tax_payable = 0.0;
    for (i, item) in items.iter().enumerate() {
        // Giá trị dòng = Thành tiền (SL × Đơn giá) − Tiền CK (chiết khấu).
        let subtotal = round2(item.quantity * item.unit_price - item.discount);
        total += subtotal;
        tax_payable += subtotal * basis[i].1;
    }
    let vat_amount = 0.0;

    let res = sqlx::query!(
        "INSERT INTO invoice (number, date, customer, customer_tax_code, total, vat_amount, status)
         VALUES (?, ?, ?, ?, ?, ?, 'draft')",
        number,
        date,
        customer,
        customer_tax_code,
        total,
        vat_amount
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    let invoice_id = res.last_insert_rowid();

    for (i, item) in items.iter().enumerate() {
        let product = resolve_product(&mut tx, &item.product_code).await?;
        let subtotal = round2(item.quantity * item.unit_price - item.discount);
        let (basis_code, basis_vat, basis_pit) = basis[i].clone();
        sqlx::query!(
            "INSERT INTO invoice_item (invoice_id, product_id, quantity, unit_price, subtotal,
                                       industry_code, vat_rate, pit_rate, discount, warehouse_code)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            invoice_id,
            product.id,
            item.quantity,
            item.unit_price,
            subtotal,
            basis_code,
            basis_vat,
            basis_pit,
            item.discount,
            item.warehouse_code
        )
        .bind(item.discount)
        .bind(&item.warehouse_code)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    }

    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "ok": true,
        "invoice_id": invoice_id,
        "total": round2(total),
        "vat_amount": vat_amount,
        "tax_payable": round2(tax_payable),
    }))
}

/// Tạo hóa đơn nháp — kiểm tra tồn kho trước khi lưu
#[tauri::command]
pub(crate) async fn save_invoice(
    state: State<'_, AppState>,
    number: String,
    date: String,
    customer: String,
    customer_tax_code: String,
    items: Vec<InvoiceItemInput>,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let pool = state.pool.read().await;
    let res =
        save_invoice_core(&pool, &number, &date, &customer, &customer_tax_code, &items).await?;
    audit(&state, "save", "invoice", &number).await;
    Ok(res.to_string())
}

/// Ghi nhận số HĐĐT cho hóa đơn nháp → trạng thái 'official'
#[tauri::command]
pub(crate) async fn link_hddt(
    state: State<'_, AppState>,
    invoice_id: i64,
    hddt_no: String,
    hddt_symbol: String,
    hddt_date: String,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    sqlx::query!(
        "UPDATE invoice SET status = 'official', e_invoice_no = ?, e_invoice_symbol = ?, e_invoice_date = ? WHERE id = ?",
        hddt_no,
        hddt_symbol,
        hddt_date,
        invoice_id
    )
    .execute(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    audit(
        &state,
        "link_hddt",
        "invoice",
        &format!("invoice_id={} → {}/{}", invoice_id, hddt_symbol, hddt_no),
    )
    .await;
    Ok("ok".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;

    fn line(
        code: &str,
        qty: f64,
        price: f64,
        discount: f64,
        warehouse_code: &str,
    ) -> InvoiceItemInput {
        InvoiceItemInput {
            product_code: code.into(),
            quantity: qty,
            unit_price: price,
            industry_code: "PPHH".into(),
            discount,
            warehouse_code: warehouse_code.into(),
        }
    }

    #[tokio::test]
    async fn hoa_don_tru_tien_ck_va_tinh_thue_theo_gia_tri_thuc() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "HD-A", "Hàng bán", 0.01).await;
        let w = seed_warehouse(&pool, "W1", "Kho 1").await;
        add_stock_lot(&pool, p, w, 100.0, 1000.0, "2026-01-01").await;

        // 10 x 2000 = 20000 − CK 5000 → giá trị dòng 15000.
        let res = save_invoice_core(
            &pool,
            "HD0001",
            "2026-03-10",
            "Khách X",
            "MST-X",
            &[line("HD-A", 10.0, 2000.0, 5000.0, "W1")],
        )
        .await
        .expect("lưu hóa đơn có chiết khấu");

        assert_eq!(res["total"].as_f64().unwrap(), 15_000.0);
        // Thuế theo nhóm ngành PPHH (vat_rate 1%): 15000 × 0.01 = 150
        assert_eq!(res["tax_payable"].as_f64().unwrap(), 150.0);

        // Dòng hóa đơn lưu đủ discount + kho xuất
        let row = sqlx::query!("SELECT subtotal, discount, warehouse_code FROM invoice_item")
            .fetch_one(&pool)
            .await
            .unwrap();
        let (subtotal, discount, wh) = (row.subtotal, row.discount, row.warehouse_code);
        assert_eq!((subtotal, discount), (15_000.0, 5_000.0));
        assert_eq!(wh, "W1");
    }

    #[tokio::test]
    async fn hoa_don_kiem_tra_ton_theo_dung_kho_xuat_tren_dong() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "HD-B", "Hàng B", 0.01).await;
        let w1 = seed_warehouse(&pool, "W1", "Kho 1").await;
        // W2 chỉ tồn tại trong DB — bài test khai kho bằng chuỗi "W2", không cần giữ biến.
        seed_warehouse(&pool, "W2", "Kho 2").await;
        // W1 có 5, W2 không có gì
        add_stock_lot(&pool, p, w1, 5.0, 1000.0, "2026-01-01").await;

        // Khoai trừ W1 nhưng khai kho W2 → thiếu dù tổng kho đủ
        let err = save_invoice_core(
            &pool,
            "HD0002",
            "2026-03-10",
            "Khách Y",
            "MST-Y",
            &[line("HD-B", 4.0, 2000.0, 0.0, "W2")],
        )
        .await
        .unwrap_err();
        assert!(err.contains("Không đủ tồn kho"), "err: {err}");

        // Đủ tồn ở đúng kho W1 → lưu được
        save_invoice_core(
            &pool,
            "HD0003",
            "2026-03-10",
            "Khách Y",
            "MST-Y",
            &[line("HD-B", 4.0, 2000.0, 0.0, "W1")],
        )
        .await
        .expect("đủ tồn kho đúng kho");
    }

    #[tokio::test]
    async fn hoa_don_khong_khai_kho_kiem_theo_tong_cac_kho() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "HD-C", "Hàng C", 0.01).await;
        let w1 = seed_warehouse(&pool, "W1", "Kho 1").await;
        let w2 = seed_warehouse(&pool, "W2", "Kho 2").await;
        add_stock_lot(&pool, p, w1, 3.0, 1000.0, "2026-01-01").await;
        add_stock_lot(&pool, p, w2, 2.0, 1000.0, "2026-01-01").await;

        // Không chọn kho → cộng dồn 3 + 2 = 5, đủ xuất 5
        let res = save_invoice_core(
            &pool,
            "HD0004",
            "2026-03-10",
            "Khách Z",
            "MST-Z",
            &[line("HD-C", 5.0, 1000.0, 0.0, "")],
        )
        .await
        .expect("đủ tổng tồn các kho");
        assert_eq!(res["total"].as_f64().unwrap(), 5_000.0);
        // Kho xuất để trống → lưu chuỗi rỗng
        let wh: String = sqlx::query_scalar!("SELECT warehouse_code FROM invoice_item")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(wh, "");
    }
}
