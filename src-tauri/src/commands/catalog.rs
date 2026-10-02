#![allow(unused_imports)]

use crate::helpers::*;
use crate::models::*;
use serde_json::json;
use sqlx::sqlite::{SqlitePoolOptions, SqliteTransaction};
use sqlx::SqlitePool;
use tauri::{AppHandle, Manager, State};
#[tauri::command]
pub(crate) async fn get_accounts(state: State<'_, AppState>) -> Result<String, String> {
    let rows: Vec<AccountRow> = sqlx::query_as!(
        AccountRow,
        "SELECT id, code, name, opening_debit, opening_credit FROM account ORDER BY code"
    )
    .fetch_all(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    Ok(serde_json::to_string(&rows).unwrap_or_default())
}

/// Lưu tài khoản DMTK (thêm mới hoặc sửa theo mã — upsert).
#[tauri::command]
pub(crate) async fn save_account(
    state: State<'_, AppState>,
    code: String,
    name: String,
    opening_debit: f64,
    opening_credit: f64,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    sqlx::query!(
        "INSERT INTO account (code, name, opening_debit, opening_credit) VALUES (?, ?, ?, ?)
         ON CONFLICT(code) DO UPDATE SET
            name = excluded.name,
            opening_debit = excluded.opening_debit,
            opening_credit = excluded.opening_credit",
        code,
        name,
        opening_debit,
        opening_credit
    )
    .execute(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    audit(&state, "save", "account", &code).await;
    Ok("ok".into())
}

/// Xóa tài khoản khỏi DMTK. journal_entry lưu mã tài khoản dạng text (không
/// khóa ngoại) nên việc xóa dòng DMTK không làm hỏng bút toán đã ghi.
#[tauri::command]
pub(crate) async fn delete_account(state: State<'_, AppState>, id: i64) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let row: Option<String> = sqlx::query_scalar!("SELECT code FROM account WHERE id = ?", id)
        .fetch_optional(&*state.pool.read().await)
        .await
        .map_err(|e| e.to_string())?;
    let Some(code) = row else {
        return Err("Không tìm thấy tài khoản".into());
    };
    sqlx::query!("DELETE FROM account WHERE id = ?", id)
        .execute(&*state.pool.read().await)
        .await
        .map_err(|e| e.to_string())?;
    audit(&state, "delete", "account", &code).await;
    Ok("ok".into())
}

#[tauri::command]
pub(crate) async fn get_products(state: State<'_, AppState>) -> Result<String, String> {
    // `is_service` lưu INTEGER; ép kiểu bool để khớp ProductRow (macro check).
    let rows: Vec<ProductRow> = sqlx::query_as!(
        ProductRow,
        "SELECT id, code, name, unit, sale_price, cost_price, min_stock, vat_rate, import_tax_rate, is_service as \"is_service: bool\", industry_code
         FROM product ORDER BY code"
    )
    .fetch_all(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    Ok(serde_json::to_string(&rows).unwrap_or_default())
}

#[tauri::command]
pub(crate) async fn get_industry_groups(state: State<'_, AppState>) -> Result<String, String> {
    let rows: Vec<IndustryGroupRow> = sqlx::query_as!(
        IndustryGroupRow,
        "SELECT code, name, vat_rate, pit_rate FROM industry_group ORDER BY code"
    )
    .fetch_all(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    Ok(serde_json::to_string(&rows).unwrap_or_default())
}

/// Lưu nhóm ngành (thêm mới hoặc sửa theo mã). Tỷ lệ GTGT / TNCN là phần thập
/// phân (5% = 0.05) — đúng định dạng dùng khi tính thuế bán ra theo nhóm ngành.
#[tauri::command]
pub(crate) async fn save_industry_group(
    state: State<'_, AppState>,
    code: String,
    name: String,
    vat_rate: f64,
    pit_rate: f64,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let code = code.trim().to_string();
    let name = name.trim().to_string();
    if code.is_empty() || name.is_empty() {
        return Err("Mã và tên nhóm ngành là bắt buộc".into());
    }
    if !(0.0..=1.0).contains(&vat_rate) || !(0.0..=1.0).contains(&pit_rate) {
        return Err("Tỷ lệ thuế phải nằm trong khoảng 0 – 100%".into());
    }
    let pool = state.pool.read().await;
    sqlx::query!(
        "INSERT INTO industry_group (code, name, vat_rate, pit_rate) VALUES (?, ?, ?, ?)
         ON CONFLICT(code) DO UPDATE SET
            name = excluded.name, vat_rate = excluded.vat_rate, pit_rate = excluded.pit_rate",
        code,
        name,
        vat_rate,
        pit_rate
    )
    .execute(&*pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(json!({ "ok": true, "code": code }).to_string())
}

/// Xóa nhóm ngành — chỉ xóa khi chưa được tham chiếu (sản phẩm / bút toán /
/// hóa đơn), tránh làm hỏng báo cáo thuế và tờ khai.
#[tauri::command]
pub(crate) async fn delete_industry_group(
    state: State<'_, AppState>,
    code: String,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let code = code.trim().to_string();
    let pool = state.pool.read().await;
    let used: i64 = sqlx::query_scalar!(
        "SELECT
            (SELECT COUNT(*) FROM product WHERE industry_code = ?) +
            (SELECT COUNT(*) FROM journal_entry WHERE industry_code = ?) +
            (SELECT COUNT(*) FROM invoice_item WHERE industry_code = ?)",
        code,
        code,
        code
    )
    .fetch_one(&*pool)
    .await
    .map_err(|e| e.to_string())?;
    if used > 0 {
        return Err(format!(
            "Không xóa được nhóm ngành '{}' — đang được dùng cho sản phẩm / bút toán / hóa đơn",
            code
        ));
    }
    sqlx::query!("DELETE FROM industry_group WHERE code = ?", code)
        .execute(&*pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!({ "ok": true, "code": code }).to_string())
}

#[tauri::command]
pub(crate) async fn get_warehouses(state: State<'_, AppState>) -> Result<String, String> {
    let rows: Vec<WarehouseRow> = sqlx::query_as!(
        WarehouseRow,
        "SELECT id, code, name FROM warehouse ORDER BY code"
    )
    .fetch_all(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    Ok(serde_json::to_string(&rows).unwrap_or_default())
}

/// get_warehouses — phân trang server-side (danh sách dài theo thời gian).
#[tauri::command]
pub(crate) async fn get_warehouses_page(
    state: State<'_, AppState>,
    lazy_event: String,
) -> Result<String, String> {
    warehouses_page(&*state.pool.read().await, &lazy_event).await
}

/// Core của `get_warehouses_page` — tách ra nhận `&SqlitePool` để test được
/// (command nhận `State` không dựng nổi trong unit test).
pub(crate) async fn warehouses_page(pool: &SqlitePool, lazy_event: &str) -> Result<String, String> {
    let ev: crate::commands::page::PageEvent =
        serde_json::from_str(lazy_event).map_err(|e| format!("lazy_event lỗi: {}", e))?;
    let page = ev.page();
    // Ô tìm kiếm toàn cục + hàng lọc theo cột (các điều kiện AND với nhau).
    let (mut where_sql, params) =
        crate::commands::page::global_where(ev.global_keyword(), &["name", "code"]);
    let (extra_sql, extra_params) = crate::commands::page::column_where(&ev, &["name", "code"]);
    where_sql.push_str(&extra_sql);
    let mut params = params;
    params.extend(extra_params);
    let order = crate::commands::page::order_by(
        &[("id", "id"), ("code", "code"), ("name", "name")],
        &ev,
        "code ASC",
    );
    let sql = format!(
        "SELECT id, code, name FROM warehouse{} ORDER BY {} LIMIT ? OFFSET ?",
        where_sql, order
    );
    let count_sql = format!("SELECT COUNT(*) FROM warehouse{where_sql}");
    // sqlx runtime (không macro) không kiểm tra cột lúc build → struct phải khớp
    // đúng bảng: warehouse chỉ có id/code/name, dùng SupplierRow sẽ lỗi ColumnNotFound.
    let (rows, total): (Vec<WarehouseRow>, i64) =
        crate::commands::page::fetch_page(pool, &sql, &count_sql, &params, &page).await?;
    Ok(
        serde_json::to_string(&crate::commands::audit::PageResult { rows, total })
            .unwrap_or_default(),
    )
}

#[tauri::command]
pub(crate) async fn save_warehouse(
    state: State<'_, AppState>,
    code: String,
    name: String,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan", "kho"]).await?;
    sqlx::query!(
        "INSERT INTO warehouse (code, name) VALUES (?, ?)
         ON CONFLICT(code) DO UPDATE SET name = excluded.name",
        code,
        name
    )
    .execute(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    audit(&state, "save", "warehouse", &code).await;
    Ok("ok".into())
}

#[tauri::command]
pub(crate) async fn get_suppliers(state: State<'_, AppState>) -> Result<String, String> {
    let rows: Vec<SupplierRow> = sqlx::query_as!(
        SupplierRow,
        "SELECT id, code, name, address, tax_code, phone FROM supplier ORDER BY code"
    )
    .fetch_all(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    Ok(serde_json::to_string(&rows).unwrap_or_default())
}

/// get_suppliers — phân trang server-side (danh sách dài theo thời gian).
#[tauri::command]
pub(crate) async fn get_suppliers_page(
    state: State<'_, AppState>,
    lazy_event: String,
) -> Result<String, String> {
    suppliers_page(&*state.pool.read().await, &lazy_event).await
}

/// Core của `get_suppliers_page` — tách ra nhận `&SqlitePool` để test được.
pub(crate) async fn suppliers_page(pool: &SqlitePool, lazy_event: &str) -> Result<String, String> {
    let ev: crate::commands::page::PageEvent =
        serde_json::from_str(lazy_event).map_err(|e| format!("lazy_event lỗi: {}", e))?;
    let page = ev.page();
    // Ô tìm kiếm toàn cục + hàng lọc theo cột (các điều kiện AND với nhau).
    let (mut where_sql, params) = crate::commands::page::global_where(
        ev.global_keyword(),
        &["name", "code", "address", "tax_code", "phone"],
    );
    let (extra_sql, extra_params) =
        crate::commands::page::column_where(&ev, &["name", "code", "address", "tax_code", "phone"]);
    where_sql.push_str(&extra_sql);
    let mut params = params;
    params.extend(extra_params);
    let order = crate::commands::page::order_by(
        &[("id", "id"), ("code", "code"), ("name", "name")],
        &ev,
        "code ASC",
    );
    let sql = format!(
        "SELECT id, code, name, address, tax_code, phone FROM supplier{} ORDER BY {} LIMIT ? OFFSET ?",
        where_sql, order
    );
    let count_sql = format!("SELECT COUNT(*) FROM supplier{where_sql}");
    // Cùng hình dạng cột với CustomerRow nhưng dùng đúng kiểu bảng (đồng nhất, chống lệch sau này).
    let (rows, total): (Vec<SupplierRow>, i64) =
        crate::commands::page::fetch_page(pool, &sql, &count_sql, &params, &page).await?;
    Ok(
        serde_json::to_string(&crate::commands::audit::PageResult { rows, total })
            .unwrap_or_default(),
    )
}

#[tauri::command]
pub(crate) async fn save_supplier(
    state: State<'_, AppState>,
    code: String,
    name: String,
    address: String,
    tax_code: String,
    phone: String,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan", "kho"]).await?;
    sqlx::query!(
        "INSERT INTO supplier (code, name, address, tax_code, phone) VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(code) DO UPDATE SET
            name = excluded.name, address = excluded.address,
            tax_code = excluded.tax_code, phone = excluded.phone",
        code,
        name,
        address,
        tax_code,
        phone
    )
    .execute(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    audit(&state, "save", "supplier", &code).await;
    Ok("ok".into())
}

#[tauri::command]
pub(crate) async fn get_customers(state: State<'_, AppState>) -> Result<String, String> {
    let rows: Vec<CustomerRow> = sqlx::query_as!(
        CustomerRow,
        "SELECT id, code, name, address, tax_code, phone FROM customer ORDER BY code"
    )
    .fetch_all(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    Ok(serde_json::to_string(&rows).unwrap_or_default())
}

/// get_customers — phân trang server-side (danh sách dài theo thời gian).
#[tauri::command]
pub(crate) async fn get_customers_page(
    state: State<'_, AppState>,
    lazy_event: String,
) -> Result<String, String> {
    let ev: crate::commands::page::PageEvent =
        serde_json::from_str(&lazy_event).map_err(|e| format!("lazy_event lỗi: {}", e))?;
    let page = ev.page();
    // Ô tìm kiếm toàn cục + hàng lọc theo cột (các điều kiện AND với nhau).
    let (mut where_sql, params) = crate::commands::page::global_where(
        ev.global_keyword(),
        &["name", "code", "address", "tax_code", "phone"],
    );
    let (extra_sql, extra_params) =
        crate::commands::page::column_where(&ev, &["name", "code", "address", "tax_code", "phone"]);
    where_sql.push_str(&extra_sql);
    let mut params = params;
    params.extend(extra_params);
    let order = crate::commands::page::order_by(
        &[("id", "id"), ("code", "code"), ("name", "name")],
        &ev,
        "code ASC",
    );
    let sql = format!(
        "SELECT id, code, name, address, tax_code, phone FROM customer{} ORDER BY {} LIMIT ? OFFSET ?",
        where_sql, order
    );
    let count_sql = format!("SELECT COUNT(*) FROM customer{where_sql}");
    let (rows, total): (Vec<CustomerRow>, i64) = crate::commands::page::fetch_page(
        &*state.pool.read().await,
        &sql,
        &count_sql,
        &params,
        &page,
    )
    .await?;
    Ok(
        serde_json::to_string(&crate::commands::audit::PageResult { rows, total })
            .unwrap_or_default(),
    )
}

#[tauri::command]
pub(crate) async fn save_customer(
    state: State<'_, AppState>,
    code: String,
    name: String,
    address: String,
    tax_code: String,
    phone: String,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan", "kho"]).await?;
    sqlx::query!(
        "INSERT INTO customer (code, name, address, tax_code, phone) VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(code) DO UPDATE SET
            name = excluded.name, address = excluded.address,
            tax_code = excluded.tax_code, phone = excluded.phone",
        code,
        name,
        address,
        tax_code,
        phone
    )
    .execute(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    audit(&state, "save", "customer", &code).await;
    Ok("ok".into())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn save_product(
    state: State<'_, AppState>,
    code: String,
    name: String,
    unit: String,
    sale_price: f64,
    cost_price: f64,
    min_stock: f64,
    vat_rate: f64,
    import_tax_rate: f64,
    is_service: bool,
    industry_code: String,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan", "kho"]).await?;
    // Không cho đổi tên sản phẩm đã tồn tại (tên cũ đã ghi trong phiếu kho /
    // hóa đơn / bút toán) — tránh sự cố lệch số liệu. Người dùng cần đổi tên
    // thì xóa (nếu chưa dùng) và tạo sản phẩm mới.
    {
        let pool = state.pool.read().await;
        let existing: Option<String> =
            sqlx::query_scalar!("SELECT name FROM product WHERE code = ?", code)
                .fetch_optional(&*pool)
                .await
                .map_err(|e| e.to_string())?;
        if let Some(old_name) = existing {
            if old_name != name {
                return Err(format!(
                    "Không được đổi tên sản phẩm '{}' — tên gốc đã ghi trong sổ: '{}'",
                    code, old_name
                ));
            }
        }
    }
    sqlx::query!(
        "INSERT INTO product (code, name, unit, sale_price, cost_price, min_stock, vat_rate, import_tax_rate, is_service, industry_code)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(code) DO UPDATE SET
            name = excluded.name, unit = excluded.unit,
            sale_price = excluded.sale_price, cost_price = excluded.cost_price,
            min_stock = excluded.min_stock, vat_rate = excluded.vat_rate,
            import_tax_rate = excluded.import_tax_rate, is_service = excluded.is_service,
            industry_code = excluded.industry_code",
        code,
        name,
        unit,
        sale_price,
        cost_price,
        min_stock,
        vat_rate,
        import_tax_rate,
        is_service,
        industry_code
    )
    .execute(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    audit(&state, "save", "product", &code).await;
    Ok("ok".into())
}

/// Kiểm tra sản phẩm có đang được dùng ở nghiệp vụ nào không (phiếu kho / lô
/// FIFO, hóa đơn, kiểm kê, bút toán nhập liệu). Trả về chuỗi mô tả nơi sử dụng;
/// None = chưa nơi nào dùng → được phép xóa.
async fn find_product_usage(
    pool: &sqlx::SqlitePool,
    id: i64,
    code: &str,
) -> Result<Option<String>, String> {
    let usage = sqlx::query!(
        "SELECT
             (SELECT COUNT(*) FROM stock_lot WHERE product_id = ?) AS lots,
             (SELECT COUNT(*) FROM invoice_item WHERE product_id = ?) AS invoices,
             (SELECT COUNT(*) FROM inventory_count_item WHERE product_id = ?) AS counts,
             (SELECT COUNT(*) FROM journal_entry WHERE product_code = ?) AS entries",
        id,
        id,
        id,
        code
    )
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;
    let (lots, invoices, counts, entries) =
        (usage.lots, usage.invoices, usage.counts, usage.entries);

    let mut parts: Vec<String> = Vec::new();
    if lots > 0 {
        parts.push(format!("{} phiếu kho (tồn kho / lô)", lots));
    }
    if invoices > 0 {
        parts.push(format!("{} dòng hóa đơn", invoices));
    }
    if counts > 0 {
        parts.push(format!("{} lần kiểm kê", counts));
    }
    if entries > 0 {
        parts.push(format!("{} bút toán nhập liệu", entries));
    }
    if parts.is_empty() {
        Ok(None)
    } else {
        Ok(Some(parts.join(", ")))
    }
}

/// Xóa 1 sản phẩm — chỉ xóa khi chưa được dùng ở bất kỳ nghiệp vụ nào
/// (phiếu kho, hóa đơn, kiểm kê, nhập liệu), tránh làm hỏng sổ sách đã ghi.
#[tauri::command]
pub(crate) async fn delete_product(state: State<'_, AppState>, id: i64) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan", "kho"]).await?;
    let pool = state.pool.read().await;
    let row = sqlx::query!("SELECT code, name FROM product WHERE id = ?", id)
        .fetch_optional(&*pool)
        .await
        .map_err(|e| e.to_string())?;
    let Some(row) = row else {
        return Err("Không tìm thấy sản phẩm".into());
    };
    let (code, name) = (row.code, row.name);
    if let Some(used) = find_product_usage(&pool, id, &code).await? {
        return Err(format!(
            "Không xóa được sản phẩm '{}' ({}) — đang được dùng: {}",
            name, code, used
        ));
    }
    sqlx::query!("DELETE FROM product WHERE id = ?", id)
        .execute(&*pool)
        .await
        .map_err(|e| format!("Không thể xóa sản phẩm '{}': {}", code, e))?;
    audit(&state, "delete", "product", &code).await;
    Ok("ok".into())
}

/// Xóa hàng loạt nhiều sản phẩm (theo checkbox). Kiểm tra TẤT CẢ trước — nếu có
/// bất kỳ sản phẩm nào đang được dùng thì KHÔNG xóa sản phẩm nào cả
/// (all-or-nothing), tránh xóa cục bộ gây sổ sách không nhất quán.
#[tauri::command]
pub(crate) async fn delete_products(
    state: State<'_, AppState>,
    ids: Vec<i64>,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan", "kho"]).await?;
    let pool = state.pool.read().await;

    let mut blocked: Vec<String> = Vec::new();
    let mut delete_targets: Vec<(i64, String)> = Vec::new();
    for id in &ids {
        let row = sqlx::query!("SELECT code, name FROM product WHERE id = ?", id)
            .fetch_optional(&*pool)
            .await
            .map_err(|e| e.to_string())?;
        let Some(row) = row else { continue };
        let (code, name) = (row.code, row.name);
        match find_product_usage(&pool, *id, &code).await? {
            Some(used) => blocked.push(format!("'{}' ({}) — {}", name, code, used)),
            None => delete_targets.push((*id, code)),
        }
    }

    if !blocked.is_empty() {
        let mut msg = format!(
            "Không xóa được {} sản phẩm (đang được dùng ở nghiệp vụ khác): ",
            blocked.len()
        );
        for b in blocked.iter().take(3) {
            msg.push_str(b);
            msg.push_str("; ");
        }
        if blocked.len() > 3 {
            msg.push_str(&format!("và {} sản phẩm khác.", blocked.len() - 3));
        }
        return Err(msg);
    }

    // Không sản phẩm nào bị chặn → xóa hết trong 1 transaction.
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    for (id, _) in &delete_targets {
        sqlx::query!("DELETE FROM product WHERE id = ?", id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("Không thể xóa sản phẩm: {}", e))?;
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    drop(pool);
    for (_, code) in &delete_targets {
        audit(&state, "delete", "product", code).await;
    }
    Ok("ok".into())
}

/// Nhóm ngành mặc định cho hàng hóa (có sẵn từ migration khởi tạo). Mặt hàng tạo
/// từ hóa đơn HĐĐT cũng dùng mã này — xem `hddt::sync`.
pub(crate) const GOODS_INDUSTRY_CODE: &str = "PPHH";

/// Gán nhóm ngành "Phân phối, cung cấp hàng hóa" (PPHH) cho MỌI sản phẩm loại
/// "Hàng hóa" (`is_service = 0`) đang chưa có nhóm ngành — dùng để vá dữ liệu
/// cũ sau khi đồng bộ hóa đơn tạo mặt hàng trước khi nhóm ngành có mặc định.
///
/// Không đụng: sản phẩm dịch vụ (tỷ lệ thuế theo ngành thực tế) và sản phẩm đã
/// được gán nhóm khác (ý của người dùng). Trả về (số đã gán, số hàng hóa đã có
/// nhóm khác, tên nhóm) để UI báo rõ; lỗi nếu danh mục không còn nhóm PPHH.
pub(crate) async fn assign_goods_industry_core(
    pool: &SqlitePool,
) -> Result<(i64, i64, String), String> {
    let group = sqlx::query!(
        "SELECT code, name FROM industry_group WHERE code = ?",
        GOODS_INDUSTRY_CODE
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;
    let Some(group) = group else {
        return Err(format!(
            "Nhóm ngành '{}' không có trong danh mục — hãy tạo lại ở màn Đối tác & Kho trước khi gán hàng loạt",
            GOODS_INDUSTRY_CODE
        ));
    };
    let group_name = group.name;

    // Hàng hóa đã có nhóm khác → bỏ qua, đếm TRƯỚC khi gán để báo đúng số bị
    // bỏ qua (không tính những dòng vừa được gán PPHH).
    let kept: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM product WHERE is_service = 0 AND industry_code <> ''"
    )
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;

    let res = sqlx::query!(
        "UPDATE product SET industry_code = ?
          WHERE is_service = 0 AND (industry_code IS NULL OR industry_code = '')",
        GOODS_INDUSTRY_CODE
    )
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    let updated = res.rows_affected() as i64;
    Ok((updated, kept, group_name))
}

#[tauri::command]
pub(crate) async fn assign_goods_industry(state: State<'_, AppState>) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan", "kho"]).await?;
    let (updated, kept, name) = {
        let pool = state.pool.read().await;
        assign_goods_industry_core(&pool).await?
    };
    if updated > 0 {
        audit(&state, "update", "product", "industry").await;
    }
    Ok(serde_json::json!({
        "updated": updated,
        "kept": kept,
        "code": GOODS_INDUSTRY_CODE,
        "name": name,
    })
    .to_string())
}

/// Đọc một giá trị cấu hình từ bảng app_setting (trả về rỗng nếu chưa có).
async fn get_setting(pool: &sqlx::SqlitePool, key: &str) -> Result<String, String> {
    let row = sqlx::query!("SELECT value FROM app_setting WHERE key = ?", key)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(row.map(|r| r.value).unwrap_or_default())
}

/// Tự sinh mã sản phẩm tiếp theo theo cấu hình:
/// prefix (vd "SP") + số tự tăng (bắt đầu từ product_code_start) + đệm 0.
#[tauri::command]
pub(crate) async fn next_product_code(state: State<'_, AppState>) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan", "kho"]).await?;
    let pool = state.pool.read().await;
    let prefix = get_setting(&pool, "product_code_prefix").await?;
    let start: i64 = get_setting(&pool, "product_code_start")
        .await?
        .trim()
        .parse()
        .unwrap_or(1)
        .max(1);
    let digits: usize = get_setting(&pool, "product_code_digits")
        .await?
        .trim()
        .parse()
        .unwrap_or(4)
        .clamp(1, 12);

    let pattern = format!("{}%", prefix);
    let rows = sqlx::query!("SELECT code FROM product WHERE code LIKE ?", pattern)
        .fetch_all(&*pool)
        .await
        .map_err(|e| e.to_string())?;

    let mut max_num: i64 = start - 1;
    for r in &rows {
        if let Some(num) = r.code.strip_prefix(&prefix) {
            if let Ok(n) = num.trim_start_matches('0').trim().parse::<i64>() {
                max_num = max_num.max(n);
            }
        }
    }
    let next = max_num + 1;
    Ok(format!("{}{:0>width$}", prefix, next, width = digits))
}

/// Tự sinh mã tiếp theo: prefix + số tự tăng + đệm 0 — ví dụ KHO001, KH001,
/// NCC001... Dò theo mã đang có trong bảng nên không bao giờ trùng.
fn next_code_of(codes: Vec<String>, prefix: &str, digits: usize) -> String {
    let mut max_num: i64 = 0;
    for code in &codes {
        if let Some(num) = code.strip_prefix(prefix) {
            if let Ok(n) = num.trim_start_matches('0').trim().parse::<i64>() {
                max_num = max_num.max(n);
            }
        }
    }
    format!("{}{:0>width$}", prefix, max_num + 1, width = digits)
}

#[tauri::command]
pub(crate) async fn next_warehouse_code(state: State<'_, AppState>) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan", "kho"]).await?;
    let pool = state.pool.read().await;
    let codes: Vec<String> = sqlx::query_scalar!("SELECT code FROM warehouse")
        .fetch_all(&*pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(next_code_of(codes, "KHO", 3))
}

#[tauri::command]
pub(crate) async fn next_customer_code(state: State<'_, AppState>) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let pool = state.pool.read().await;
    let codes: Vec<String> = sqlx::query_scalar!("SELECT code FROM customer")
        .fetch_all(&*pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(next_code_of(codes, "KH", 3))
}

#[tauri::command]
pub(crate) async fn next_supplier_code(state: State<'_, AppState>) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let pool = state.pool.read().await;
    let codes: Vec<String> = sqlx::query_scalar!("SELECT code FROM supplier")
        .fetch_all(&*pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(next_code_of(codes, "NCC", 3))
}

#[tauri::command]
pub(crate) async fn next_employee_code(state: State<'_, AppState>) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let pool = state.pool.read().await;
    let codes: Vec<String> = sqlx::query_scalar!("SELECT code FROM employee")
        .fetch_all(&*pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(next_code_of(codes, "NV", 3))
}

// ─── LAZY LOAD SẢN PHẨM ───
// Nhận event lazy của PrimeVue DataTable (first/rows/sortField/sortOrder/multiSortMeta/filters)
// rồi trả về đúng 1 trang + tổng số dòng — tránh tải toàn bộ vài nghìn sản phẩm về frontend.
// Dùng runtime query (không phải macro) vì WHERE/ORDER BY dựng động → không cần .sqlx cache.

/// Giá trị bind vào query — SQLite encode cả chuỗi lẫn số.
enum BindVal {
    Str(String),
    Num(f64),
}

/// Query trang sản phẩm đã dựng sẵn (WHERE/ORDER/LIMIT/OFFSET) + tham số bind.
struct ProductPageQuery {
    sql: String,
    count_sql: String,
    params: Vec<BindVal>,
    page_size: i64,
    offset: i64,
}

/// Dựng SQL động từ event lazy của PrimeVue — tách hàm thuần để dễ test.
fn build_product_page_query(ev: &ProductPageEvent) -> Result<ProductPageQuery, String> {
    let page_size = ev.rows.unwrap_or(50).clamp(1, 500);
    let offset = ev.first.unwrap_or(0).max(0);

    // Cột được phép lọc / sắp xếp (whitelist — tránh SQL injection từ client).
    const TEXT_FIELDS: &[&str] = &["code", "name", "unit", "industry_code"];
    const NUM_FIELDS: &[&str] = &[
        "sale_price",
        "cost_price",
        "min_stock",
        "vat_rate",
        "import_tax_rate",
    ];
    const SORTABLE: &[&str] = &[
        "id",
        "code",
        "name",
        "unit",
        "sale_price",
        "cost_price",
        "min_stock",
        "vat_rate",
        "import_tax_rate",
        "is_service",
        "industry_code",
    ];

    let mut clauses: Vec<String> = Vec::new();
    let mut params: Vec<BindVal> = Vec::new();

    // Filter toàn cục: tìm khắp mã / tên / đơn vị.
    if let Some(g) = ev.filters.get("global") {
        let needle = g
            .value
            .as_ref()
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty());
        if let Some(needle) = needle {
            clauses.push("(code LIKE ? OR name LIKE ? OR unit LIKE ?)".to_string());
            let pat = format!("%{}%", needle);
            params.push(BindVal::Str(pat.clone()));
            params.push(BindVal::Str(pat));
            params.push(BindVal::Str(format!("%{}%", needle)));
        }
    }

    // Lấy giá trị + matchMode của 1 cột (ưu tiên constraint đầu tiên).
    let filter_value = |f: &ProductPageFilter| -> Option<serde_json::Value> {
        if let Some(c) = f.constraints.as_ref().and_then(|cs| cs.first()) {
            if c.value.is_some() {
                return c.value.clone();
            }
        }
        f.value.clone()
    };
    let filter_mode = |f: &ProductPageFilter| -> String {
        if let Some(c) = f.constraints.as_ref().and_then(|cs| cs.first()) {
            if let Some(m) = &c.match_mode {
                return m.to_lowercase();
            }
        }
        f.match_mode
            .clone()
            .unwrap_or_else(|| "contains".into())
            .to_lowercase()
    };

    // Filter từng cột với matchMode → SQL.
    for field in TEXT_FIELDS.iter().chain(NUM_FIELDS.iter()) {
        let Some(f) = ev.filters.get(*field) else {
            continue;
        };
        let Some(v) = filter_value(f) else { continue };
        let is_empty = match &v {
            serde_json::Value::String(s) => s.trim().is_empty(),
            _ => false,
        };
        if is_empty {
            continue;
        }
        let mode = filter_mode(f);
        let expr = if NUM_FIELDS.contains(field) {
            format!("CAST({} AS TEXT)", field) // cho phép LIKE số kiểu "50000.0"
        } else {
            (*field).to_string()
        };
        match mode.as_str() {
            "startswith" | "starts" => {
                clauses.push(format!("{} LIKE ? || '%'", expr));
                let s = v
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| v.to_string());
                params.push(BindVal::Str(s));
            }
            "endswith" | "ends" => {
                clauses.push(format!("{} LIKE '%' || ?", expr));
                let s = v
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| v.to_string());
                params.push(BindVal::Str(s));
            }
            "equals" | "eq" => {
                // vat_rate / import_tax_rate lưu dạng tỷ lệ (0.05) còn filter hiển thị % (5).
                let is_pct = *field == "vat_rate" || *field == "import_tax_rate";
                if NUM_FIELDS.contains(field) {
                    let n = v.as_f64().unwrap_or(0.0) / if is_pct { 100.0 } else { 1.0 };
                    clauses.push(format!("{} = ?", field));
                    params.push(BindVal::Num(n));
                } else {
                    clauses.push(format!("{} = ?", field));
                    let s = v.as_str().unwrap_or("").to_string();
                    params.push(BindVal::Str(s));
                }
            }
            _ => {
                // contains (mặc định)
                clauses.push(format!("{} LIKE '%' || ? || '%'", expr));
                let s = v
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| v.to_string());
                params.push(BindVal::Str(s));
            }
        }
    }

    let where_sql = if clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", clauses.join(" AND "))
    };

    // Sắp xếp: ưu tiên multiSortMeta, nếu rỗng dùng sortField/sortOrder, mặc định theo code.
    let mut sorts: Vec<String> = Vec::new();
    if let Some(meta) = &ev.multi_sort_meta {
        for m in meta.iter().take(4) {
            if let Some(f) = &m.field {
                if SORTABLE.contains(&f.as_str()) {
                    let dir = if m.order.unwrap_or(1) < 0 {
                        "DESC"
                    } else {
                        "ASC"
                    };
                    sorts.push(format!("{} {}", f, dir));
                }
            }
        }
    }
    if sorts.is_empty() {
        if let Some(f) = &ev.sort_field {
            if SORTABLE.contains(&f.as_str()) {
                let dir = if ev.sort_order.unwrap_or(1) < 0 {
                    "DESC"
                } else {
                    "ASC"
                };
                sorts.push(format!("{} {}", f, dir));
            }
        }
    }
    if sorts.is_empty() {
        sorts.push("code ASC".to_string());
    }

    let sql = format!(
        "SELECT id, code, name, unit, sale_price, cost_price, min_stock, vat_rate, import_tax_rate, is_service, industry_code
         FROM product{} ORDER BY {} LIMIT ? OFFSET ?",
        where_sql,
        sorts.join(", ")
    );
    let count_sql = format!("SELECT COUNT(*) FROM product{}", where_sql);

    Ok(ProductPageQuery {
        sql,
        count_sql,
        params,
        page_size,
        offset,
    })
}

#[tauri::command]
pub(crate) async fn get_products_page(
    state: State<'_, AppState>,
    lazy_event: String,
) -> Result<String, String> {
    let ev: ProductPageEvent =
        serde_json::from_str(&lazy_event).map_err(|e| format!("lazy_event lỗi: {}", e))?;
    let q = build_product_page_query(&ev)?;
    let pool = state.pool.read().await;

    let mut rows_q = sqlx::query_as::<_, ProductRow>(&q.sql);
    for p in &q.params {
        match p {
            BindVal::Str(s) => {
                rows_q = rows_q.bind(s.clone());
            }
            BindVal::Num(n) => {
                rows_q = rows_q.bind(*n);
            }
        }
    }
    let rows = rows_q
        .bind(q.page_size)
        .bind(q.offset)
        .fetch_all(&*pool)
        .await
        .map_err(|e| e.to_string())?;

    let mut count_q = sqlx::query_scalar::<_, i64>(&q.count_sql);
    for p in &q.params {
        match p {
            BindVal::Str(s) => {
                count_q = count_q.bind(s.clone());
            }
            BindVal::Num(n) => {
                count_q = count_q.bind(*n);
            }
        }
    }
    let total = count_q.fetch_one(&*pool).await.map_err(|e| e.to_string())?;

    let result = ProductPageResult { rows, total };
    Ok(serde_json::to_string(&result).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;

    async fn run_products(pool: &SqlitePool, q: &ProductPageQuery) -> Vec<ProductRow> {
        let mut query = sqlx::query_as::<_, ProductRow>(&q.sql);
        for p in &q.params {
            match p {
                BindVal::Str(s) => {
                    query = query.bind(s.clone());
                }
                BindVal::Num(n) => {
                    query = query.bind(*n);
                }
            }
        }
        query
            .bind(q.page_size)
            .bind(q.offset)
            .fetch_all(pool)
            .await
            .expect("run products page")
    }

    async fn run_count(pool: &SqlitePool, q: &ProductPageQuery) -> i64 {
        let mut query = sqlx::query_scalar::<_, i64>(&q.count_sql);
        for p in &q.params {
            match p {
                BindVal::Str(s) => {
                    query = query.bind(s.clone());
                }
                BindVal::Num(n) => {
                    query = query.bind(*n);
                }
            }
        }
        query.fetch_one(pool).await.expect("run count")
    }

    #[tokio::test]
    async fn products_page_lazy_filters_sorts_paginates() {
        let pool = test_pool().await;
        seed_product(&pool, "SP001", "Táo đỏ", 0.05).await;
        seed_product(&pool, "SP002", "Chuối", 0.10).await;
        seed_product(&pool, "SP003", "Táo xanh", 0.05).await;

        // (1) Filter toàn cục + sort giảm dần theo tên, trang đầu 2 dòng.
        let ev: ProductPageEvent = serde_json::from_str(
            r#"{"first":0,"rows":2,"multiSortMeta":[{"field":"name","order":-1}],
               "filters":{"global":{"value":"táo","matchMode":"contains"}}}"#,
        )
        .unwrap();
        let q = build_product_page_query(&ev).unwrap();
        assert_eq!(run_count(&pool, &q).await, 2);
        let rows = run_products(&pool, &q).await;
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name, "Táo đỏ");
        assert_eq!(rows[1].name, "Táo xanh");

        // (2) Filter vat_rate theo % (5 → 0.05 lưu trong DB).
        let ev: ProductPageEvent = serde_json::from_str(
            r#"{"first":0,"rows":10,"filters":{"vat_rate":{"value":5,"matchMode":"equals"}}}"#,
        )
        .unwrap();
        let q = build_product_page_query(&ev).unwrap();
        assert_eq!(run_count(&pool, &q).await, 2);

        // (3) Phân trang: mặc định sort theo code, trang 2 (mỗi trang 2).
        let ev: ProductPageEvent = serde_json::from_str(r#"{"first":2,"rows":2}"#).unwrap();
        let q = build_product_page_query(&ev).unwrap();
        assert_eq!(run_count(&pool, &q).await, 3);
        let rows = run_products(&pool, &q).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].code, "SP003");

        // (4) Whitelist chặn sort field độc hại → rơi về mặc định code ASC.
        let ev: ProductPageEvent =
            serde_json::from_str(r#"{"sortField":"name; DROP TABLE product","sortOrder":1}"#)
                .unwrap();
        let q = build_product_page_query(&ev).unwrap();
        assert!(!q.sql.to_lowercase().contains("drop table"));
        assert!(q.sql.contains("ORDER BY code ASC"));
    }

    #[tokio::test]
    async fn warehouse_page_dung_kieu_row_cua_warehouse() {
        let pool = test_pool().await;
        // init_database đã tạo sẵn kho mặc định — thêm 1 kho nữa để có nhiều dòng.
        sqlx::query("INSERT OR IGNORE INTO warehouse (code, name) VALUES ('KHO-TEST', 'Kho test')")
            .execute(&pool)
            .await
            .unwrap();

        // SELECT chỉ có id/code/name → decode bằng struct thiếu/thừa cột sẽ lỗi
        // ColumnNotFound lúc runtime (sqlx không kiểm tra được khi SQL dựng bằng format!).
        let json = warehouses_page(&pool, "{}").await.expect("warehouses_page");
        let page: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(page["total"].as_i64().unwrap() >= 1);
        let rows = page["rows"].as_array().unwrap();
        assert!(rows.iter().any(|r| r["code"] == "KHO-TEST"));
        // Đúng cấu trúc: warehouse không có address/tax_code/phone.
        assert!(rows.iter().all(|r| r.get("address").is_none()));
    }

    #[tokio::test]
    async fn supplier_page_dung_kieu_row_cua_supplier() {
        let pool = test_pool().await;
        sqlx::query(
            "INSERT INTO supplier (code, name, address, tax_code, phone)
             VALUES ('NCC001', 'Nhà cung cấp A', 'Hà Nội', '0123456789', '0900000000')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let json = suppliers_page(&pool, "{}").await.expect("suppliers_page");
        let page: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(page["total"], 1);
        assert_eq!(page["rows"][0]["code"], "NCC001");
        assert_eq!(page["rows"][0]["address"], "Hà Nội");
    }
}
