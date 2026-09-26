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

/// 1 dòng hóa đơn đã qua kiểm tra (có sẵn mã sản phẩm, nhóm ngành, tỷ lệ thuế).
struct PreparedLine {
    product_id: i64,
    quantity: f64,
    unit_price: f64,
    /// Giá trị dòng = Thành tiền (SL × Đơn giá) − Tiền CK (chiết khấu).
    subtotal: f64,
    industry_code: String,
    vat_rate: f64,
    pit_rate: f64,
    discount: f64,
    warehouse_code: String,
}

struct PreparedInvoice {
    total: f64,
    tax_payable: f64,
    lines: Vec<PreparedLine>,
}

/// Kiểm tra + dựng dòng hóa đơn từ input người dùng: nhóm ngành phải hợp lệ, tồn
/// kho phải đủ theo ĐÚNG kho xuất trên từng dòng (rỗng → tổng mọi kho), và tính
/// tổng tiền. Dùng chung cho lập mới và sửa hóa đơn nháp.
async fn prepare_invoice(
    tx: &mut SqliteTransaction<'_>,
    items: &[InvoiceItemInput],
) -> Result<PreparedInvoice, String> {
    if items.is_empty() {
        return Err("Chưa có mặt hàng nào trong hóa đơn".into());
    }

    let mut basis: Vec<(String, f64, f64)> = Vec::with_capacity(items.len());
    let mut shortage: Vec<String> = Vec::new();
    for item in items {
        let product = resolve_product(tx, &item.product_code).await?;

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
        .fetch_one(&mut **tx)
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
            .fetch_one(&mut **tx)
            .await
            .map_err(|e| e.to_string())?
        } else {
            let wh_id = resolve_warehouse(tx, &item.warehouse_code, &item.product_code).await?;
            sqlx::query_scalar!(
                r#"SELECT COALESCE(SUM(quantity), 0.0) as "qty!: f64" FROM stock_lot
                   WHERE product_id = ? AND warehouse_id = ? AND depleted = 0"#,
                product.id,
                wh_id
            )
            .fetch_one(&mut **tx)
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
    let mut lines = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        let product = resolve_product(tx, &item.product_code).await?;
        let subtotal = round2(item.quantity * item.unit_price - item.discount);
        total += subtotal;
        tax_payable += subtotal * basis[i].1;
        let (basis_code, basis_vat, basis_pit) = basis[i].clone();
        lines.push(PreparedLine {
            product_id: product.id,
            quantity: item.quantity,
            unit_price: item.unit_price,
            subtotal,
            industry_code: basis_code,
            vat_rate: basis_vat,
            pit_rate: basis_pit,
            discount: item.discount,
            warehouse_code: item.warehouse_code.clone(),
        });
    }

    Ok(PreparedInvoice {
        total,
        tax_payable,
        lines,
    })
}

/// Ghi dòng hóa đơn đã chuẩn bị vào bảng `invoice_item`.
async fn insert_invoice_items(
    tx: &mut SqliteTransaction<'_>,
    invoice_id: i64,
    lines: &[PreparedLine],
) -> Result<(), String> {
    for l in lines {
        sqlx::query!(
            "INSERT INTO invoice_item (invoice_id, product_id, quantity, unit_price, subtotal,
                                       industry_code, vat_rate, pit_rate, discount, warehouse_code)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            invoice_id,
            l.product_id,
            l.quantity,
            l.unit_price,
            l.subtotal,
            l.industry_code,
            l.vat_rate,
            l.pit_rate,
            l.discount,
            l.warehouse_code
        )
        .execute(&mut **tx)
        .await
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Nghiệp vụ lập hóa đơn nháp — tách riêng để test trực tiếp (không cần tauri::State).
async fn save_invoice_core(
    pool: &SqlitePool,
    number: &str,
    date: &str,
    customer: &str,
    customer_tax_code: &str,
    items: &[InvoiceItemInput],
) -> Result<serde_json::Value, String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let prepared = prepare_invoice(&mut tx, items).await?;

    // `vat_amount` luôn 0: hộ không tách thuế trên hóa đơn (tính cuối kỳ).
    let res = sqlx::query!(
        "INSERT INTO invoice (number, date, customer, customer_tax_code, total, vat_amount, status)
         VALUES (?, ?, ?, ?, ?, 0.0, 'draft')",
        number,
        date,
        customer,
        customer_tax_code,
        prepared.total
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    let invoice_id = res.last_insert_rowid();
    insert_invoice_items(&mut tx, invoice_id, &prepared.lines).await?;

    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "ok": true,
        "invoice_id": invoice_id,
        "total": round2(prepared.total),
        "vat_amount": 0.0,
        "tax_payable": round2(prepared.tax_payable),
    }))
}

/// Sửa hóa đơn NHÁP: thay toàn bộ dòng hàng và cập nhật thông tin chung.
///
/// Chỉ nháp được sửa — đã chép sang bên kia (`exported`) hoặc đã phát hành thì
/// phải đi luồng hủy / điều chỉnh để giữ dấu vết đối chiếu.
async fn update_invoice_core(
    pool: &SqlitePool,
    id: i64,
    number: &str,
    date: &str,
    customer: &str,
    customer_tax_code: &str,
    items: &[InvoiceItemInput],
) -> Result<serde_json::Value, String> {
    let inv = sqlx::query!("SELECT number, status FROM invoice WHERE id = ?", id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Không tìm thấy hóa đơn".to_string())?;
    if inv.status != "draft" {
        return Err(format!(
            "Chỉ sửa được hóa đơn nháp — {} đang ở trạng thái đã xử lý",
            inv.number
        ));
    }

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let prepared = prepare_invoice(&mut tx, items).await?;
    sqlx::query!(
        "UPDATE invoice SET number = ?, date = ?, customer = ?, customer_tax_code = ?, total = ?
          WHERE id = ?",
        number,
        date,
        customer,
        customer_tax_code,
        prepared.total,
        id
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    // Thay trọn danh sách dòng: bỏ dòng cũ rồi ghi lại (hóa đơn bán không trừ
    // kho/ghi bút toán nên không cần điều chỉnh gì khác).
    sqlx::query!("DELETE FROM invoice_item WHERE invoice_id = ?", id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    insert_invoice_items(&mut tx, id, &prepared.lines).await?;
    tx.commit().await.map_err(|e| e.to_string())?;

    Ok(serde_json::json!({
        "ok": true,
        "invoice_id": id,
        "total": round2(prepared.total),
        "vat_amount": 0.0,
        "tax_payable": round2(prepared.tax_payable),
    }))
}

#[tauri::command]
pub(crate) async fn update_invoice(
    state: State<'_, AppState>,
    id: i64,
    number: String,
    date: String,
    customer: String,
    customer_tax_code: String,
    items: Vec<InvoiceItemInput>,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let res = update_invoice_core(
        &*state.pool.read().await,
        id,
        &number,
        &date,
        &customer,
        &customer_tax_code,
        &items,
    )
    .await?;
    audit(&state, "update", "invoice", &number).await;
    Ok(res.to_string())
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

/// Xoá hóa đơn NHÁP. An toàn về sổ sách: hóa đơn bán chỉ ghi bảng `invoice` +
/// `invoice_item` (không trừ kho, không ghi bút toán — phiếu xuất là chứng tử riêng),
/// nên xoá nháp không đụng tồn kho hay kế toán.
///
/// Chỉ xoá được `draft`: đã chép sang bên kia (`exported`) hoặc đã phát hành
/// (`official`/`adjusted`) thì phải đi theo luồng hủy / điều chỉnh để còn dấu vết.
async fn delete_invoice_core(pool: &SqlitePool, id: i64) -> Result<String, String> {
    let inv = sqlx::query!(
        "SELECT number, status, e_invoice_no FROM invoice WHERE id = ?",
        id
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "Không tìm thấy hóa đơn".to_string())?;
    if inv.status != "draft" {
        return Err(format!(
            "Chỉ xoá được hóa đơn nháp — {} đang ở trạng thái đã xử lý. Hãy dùng 'ghi nhận hủy' hoặc 'bị sửa bên kia' để giữ dấu vết đối chiếu",
            inv.number
        ));
    }
    if !inv.e_invoice_no.trim().is_empty() {
        return Err(format!(
            "{} đã có số HĐĐT {} — không xoá được",
            inv.number, inv.e_invoice_no
        ));
    }

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    // Nháp chưa từng chép/hủy nên 2 bảng này thường rỗng; xoá phòng khi có để không
    // còn dòng mồ côi.
    sqlx::query!("DELETE FROM invoice_export WHERE invoice_id = ?", id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query!("DELETE FROM invoice_event WHERE invoice_id = ?", id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query!("DELETE FROM invoice_item WHERE invoice_id = ?", id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query!("DELETE FROM invoice WHERE id = ?", id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(inv.number)
}

#[tauri::command]
pub(crate) async fn delete_invoice(state: State<'_, AppState>, id: i64) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let number = delete_invoice_core(&*state.pool.read().await, id).await?;
    audit(&state, "delete", "invoice", &number).await;
    Ok("ok".into())
}

/// Ghi nhận số HĐĐT cho hóa đơn nháp → trạng thái 'official'.
///
/// Ký hiệu gõ ở đây cũng được lưu vào hồ sơ HKD (`business.hddt_symbol`) làm
/// mặc định cho lần sau — mỗi hộ có một ký hiệu riêng và đổi mẫu số theo năm
/// không cần sửa lại từng hóa đơn.
async fn link_hddt_core(
    pool: &SqlitePool,
    invoice_id: i64,
    hddt_no: &str,
    hddt_symbol: &str,
    hddt_date: &str,
) -> Result<String, String> {
    let symbol = hddt_symbol.trim();
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    sqlx::query!(
        "UPDATE invoice SET status = 'official', e_invoice_no = ?, e_invoice_symbol = ?, e_invoice_date = ? WHERE id = ?",
        hddt_no,
        symbol,
        hddt_date,
        invoice_id
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    // Ký hiệu rỗng thì giữ nguyên ký hiệu đang lưu trong hồ sơ.
    if !symbol.is_empty() {
        sqlx::query!("UPDATE business SET hddt_symbol = ? WHERE id = 1", symbol)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(symbol.to_string())
}

#[tauri::command]
pub(crate) async fn link_hddt(
    state: State<'_, AppState>,
    invoice_id: i64,
    hddt_no: String,
    hddt_symbol: String,
    hddt_date: String,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let symbol = link_hddt_core(
        &*state.pool.read().await,
        invoice_id,
        &hddt_no,
        &hddt_symbol,
        &hddt_date,
    )
    .await?;
    audit(
        &state,
        "link_hddt",
        "invoice",
        &format!("invoice_id={invoice_id} → {symbol}/{hddt_no}"),
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

    /// Lập 1 hóa đơn nháp có dòng hàng, trả về id. Mã hàng suy ra từ số hóa đơn
    /// để gọi nhiều lần trong 1 test không đụng ràng buộc UNIQUE.
    async fn seed_draft(pool: &SqlitePool, number: &str) -> i64 {
        let code = format!("HD-{}", number);
        let p = seed_product(pool, &code, "Hàng nháp", 0.01).await;
        let w = seed_warehouse(pool, &format!("W-{number}"), "Kho nháp").await;
        add_stock_lot(pool, p, w, 100.0, 1000.0, "2026-01-01").await;
        save_invoice_core(
            pool,
            number,
            "2026-03-10",
            "Khách X",
            "MST-X",
            &[line(&code, 1.0, 1000.0, 0.0, &format!("W-{number}"))],
        )
        .await
        .expect("lập được hóa đơn nháp");
        let id: i64 = sqlx::query_scalar!(
            r#"SELECT id as "id!" FROM invoice WHERE number = ?"#,
            number
        )
        .fetch_one(pool)
        .await
        .unwrap();
        id
    }

    #[tokio::test]
    async fn xoa_hoa_don_nhap_xoa_ca_dong_hang() {
        let pool = test_pool().await;
        let id = seed_draft(&pool, "HDDEL1").await;

        assert_eq!(delete_invoice_core(&pool, id).await.unwrap(), "HDDEL1");

        let left: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM invoice WHERE id = ?", id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(left, 0, "hóa đơn phải bị xoá");
        let lines: i64 =
            sqlx::query_scalar!("SELECT COUNT(*) FROM invoice_item WHERE invoice_id = ?", id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            lines, 0,
            "dòng hàng phải xoá theo, không để lại dòng mồ côi"
        );
    }

    #[tokio::test]
    async fn xoa_hoa_don_nhap_khong_dung_ton_kho() {
        // Hóa đơn bán không trừ kho: xoá nháp phải không đụng tồn (phiếu xuất mới
        // làm thay đổi tồn), nên số lô vẫn nguyên.
        let pool = test_pool().await;
        let id = seed_draft(&pool, "HDDEL2").await;
        let before: f64 = sqlx::query_scalar!("SELECT quantity FROM stock_lot")
            .fetch_one(&pool)
            .await
            .unwrap();

        delete_invoice_core(&pool, id).await.unwrap();

        let after: f64 = sqlx::query_scalar!("SELECT quantity FROM stock_lot")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(after, before);
    }

    #[tokio::test]
    async fn khong_xoa_duoc_hoa_don_da_xuat_hoac_da_phat_hanh() {
        let pool = test_pool().await;
        let id = seed_draft(&pool, "HDDEL3").await;
        sqlx::query!("UPDATE invoice SET status = 'exported' WHERE id = ?", id)
            .execute(&pool)
            .await
            .unwrap();

        let err = delete_invoice_core(&pool, id).await.unwrap_err();
        assert!(err.contains("Chỉ xoá được hóa đơn nháp"), "{err}");

        // Còn số HĐĐT thì chặn dù trạng thái vẫn là nháp.
        sqlx::query!(
            "UPDATE invoice SET status = 'draft', e_invoice_no = '00000001' WHERE id = ?",
            id
        )
        .execute(&pool)
        .await
        .unwrap();
        let err = delete_invoice_core(&pool, id).await.unwrap_err();
        assert!(err.contains("đã có số HĐĐT"), "{err}");

        let still: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM invoice WHERE id = ?", id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(still, 1, "hóa đơn bị chặn phải còn nguyên");
    }

    #[tokio::test]
    async fn sua_hoa_don_nhap_thay_dong_cu_du_dong_hang() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "HD-EDIT", "Hàng sửa", 0.01).await;
        let w = seed_warehouse(&pool, "W1", "Kho 1").await;
        add_stock_lot(&pool, p, w, 100.0, 1000.0, "2026-01-01").await;
        save_invoice_core(
            &pool,
            "HDE1",
            "2026-03-10",
            "Khách X",
            "MST-X",
            &[line("HD-EDIT", 2.0, 1000.0, 0.0, "W1")],
        )
        .await
        .unwrap();
        let id: i64 =
            sqlx::query_scalar!(r#"SELECT id as "id!" FROM invoice WHERE number = 'HDE1'"#)
                .fetch_one(&pool)
                .await
                .unwrap();

        // Sửa thành 2 dòng, đổi khách/SL/giá.
        let res = update_invoice_core(
            &pool,
            id,
            "HDE1",
            "2026-03-11",
            "Khách Y",
            "MST-Y",
            &[
                line("HD-EDIT", 3.0, 2000.0, 500.0, "W1"),
                line("HD-EDIT", 1.0, 1000.0, 0.0, "W1"),
            ],
        )
        .await
        .unwrap();
        // 3×2000 − 500 = 5500, cộng 1000 = 6500.
        assert_eq!(res["total"].as_f64().unwrap(), 6_500.0);

        let inv = sqlx::query!(
            "SELECT customer, customer_tax_code, date, total FROM invoice WHERE id = ?",
            id
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(inv.customer, "Khách Y");
        assert_eq!(inv.customer_tax_code, "MST-Y");
        assert_eq!(inv.date, "2026-03-11");
        assert_eq!(inv.total, 6_500.0);

        // 2 dòng mới, không dòng cũ sót lại.
        let rows: Vec<(f64, f64, f64, f64)> = sqlx::query!(
            "SELECT quantity, unit_price, discount, subtotal FROM invoice_item
              WHERE invoice_id = ? ORDER BY id",
            id
        )
        .fetch_all(&pool)
        .await
        .unwrap()
        .into_iter()
        .map(|r| (r.quantity, r.unit_price, r.discount, r.subtotal))
        .collect();
        assert_eq!(
            rows,
            vec![(3.0, 2000.0, 500.0, 5500.0), (1.0, 1000.0, 0.0, 1000.0)]
        );
    }

    #[tokio::test]
    async fn sua_hoa_don_nhap_giu_nguyen_trang_thai_nhap() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "HD-EDIT2", "Hàng giữ", 0.01).await;
        let w = seed_warehouse(&pool, "W1", "Kho 1").await;
        add_stock_lot(&pool, p, w, 100.0, 1000.0, "2026-01-01").await;
        save_invoice_core(
            &pool,
            "HDE2",
            "2026-03-10",
            "Khách X",
            "MST-X",
            &[line("HD-EDIT2", 1.0, 1000.0, 0.0, "W1")],
        )
        .await
        .unwrap();
        let id: i64 =
            sqlx::query_scalar!(r#"SELECT id as "id!" FROM invoice WHERE number = 'HDE2'"#)
                .fetch_one(&pool)
                .await
                .unwrap();

        update_invoice_core(
            &pool,
            id,
            "HDE2",
            "2026-03-10",
            "Khách X",
            "MST-X",
            &[line("HD-EDIT2", 5.0, 1000.0, 0.0, "W1")],
        )
        .await
        .unwrap();
        let status: String = sqlx::query_scalar!("SELECT status FROM invoice WHERE id = ?", id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status, "draft", "sửa nháp không được đổi trạng thái");
    }

    #[tokio::test]
    async fn khong_sua_duoc_hoa_don_da_xuat_hoac_da_phat_hanh() {
        let pool = test_pool().await;
        let p = seed_product(&pool, "HD-EDIT3", "Hàng chặn", 0.01).await;
        let w = seed_warehouse(&pool, "W1", "Kho 1").await;
        add_stock_lot(&pool, p, w, 100.0, 1000.0, "2026-01-01").await;
        save_invoice_core(
            &pool,
            "HDE3",
            "2026-03-10",
            "Khách X",
            "MST-X",
            &[line("HD-EDIT3", 1.0, 1000.0, 0.0, "W1")],
        )
        .await
        .unwrap();
        let id: i64 =
            sqlx::query_scalar!(r#"SELECT id as "id!" FROM invoice WHERE number = 'HDE3'"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        sqlx::query!("UPDATE invoice SET status = 'exported' WHERE id = ?", id)
            .execute(&pool)
            .await
            .unwrap();

        let err = update_invoice_core(
            &pool,
            id,
            "HDE3",
            "2026-03-10",
            "Khách X",
            "MST-X",
            &[line("HD-EDIT3", 9.0, 1000.0, 0.0, "W1")],
        )
        .await
        .unwrap_err();
        assert!(err.contains("Chỉ sửa được hóa đơn nháp"), "{err}");

        // Dữ liệu phải còn nguyên sau khi bị chặn.
        let qty: f64 =
            sqlx::query_scalar!("SELECT quantity FROM invoice_item WHERE invoice_id = ?", id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(qty, 1.0);
    }

    #[tokio::test]
    async fn sua_hoa_don_nhap_van_kiem_tra_ton_kho() {
        // Sửa số lượng vượt tồn phải bị chặn như lúc lập mới.
        let pool = test_pool().await;
        let p = seed_product(&pool, "HD-EDIT4", "Hàng ít tồn", 0.01).await;
        let w = seed_warehouse(&pool, "W1", "Kho 1").await;
        add_stock_lot(&pool, p, w, 5.0, 1000.0, "2026-01-01").await;
        save_invoice_core(
            &pool,
            "HDE4",
            "2026-03-10",
            "Khách X",
            "MST-X",
            &[line("HD-EDIT4", 2.0, 1000.0, 0.0, "W1")],
        )
        .await
        .unwrap();
        let id: i64 =
            sqlx::query_scalar!(r#"SELECT id as "id!" FROM invoice WHERE number = 'HDE4'"#)
                .fetch_one(&pool)
                .await
                .unwrap();

        let err = update_invoice_core(
            &pool,
            id,
            "HDE4",
            "2026-03-10",
            "Khách X",
            "MST-X",
            &[line("HD-EDIT4", 99.0, 1000.0, 0.0, "W1")],
        )
        .await
        .unwrap_err();
        assert!(err.contains("Không đủ tồn kho"), "{err}");
    }

    #[tokio::test]
    async fn lien_ket_hddt_luu_ky_hieu_vao_ho_so_hoi_kinh_doanh() {
        let pool = test_pool().await;
        let id = seed_draft(&pool, "HDSYM1").await;

        link_hddt_core(&pool, id, "00000123", "1C26TT152", "2026-09-26")
            .await
            .unwrap();

        let inv = sqlx::query!(
            "SELECT status, e_invoice_no, e_invoice_symbol FROM invoice WHERE id = ?",
            id
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(inv.status, "official");
        assert_eq!(inv.e_invoice_no, "00000123");
        assert_eq!(inv.e_invoice_symbol, "1C26TT152");

        // Ký hiệu được lưu vào hồ sơ làm mặc định cho lần sau.
        let symbol: String = sqlx::query_scalar!("SELECT hddt_symbol FROM business WHERE id = 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(symbol, "1C26TT152");

        // Đổi mẫu số theo năm: ký hiệu mới ghi đè ký hiệu cũ (đã cắt khoảng trắng).
        let id2 = seed_draft(&pool, "HDSYM2").await;
        link_hddt_core(&pool, id2, "00000001", " 1C27TT152 ", "2027-01-05")
            .await
            .unwrap();
        let symbol: String = sqlx::query_scalar!("SELECT hddt_symbol FROM business WHERE id = 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(symbol, "1C27TT152");

        // Gõ ký hiệu rỗng thì không đụng ký hiệu đang lưu trong hồ sơ.
        let id3 = seed_draft(&pool, "HDSYM3").await;
        link_hddt_core(&pool, id3, "00000002", "", "2027-01-06")
            .await
            .unwrap();
        let symbol: String = sqlx::query_scalar!("SELECT hddt_symbol FROM business WHERE id = 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            symbol, "1C27TT152",
            "ký hiệu rỗng không được xoá ký hiệu đã lưu"
        );
    }
}
