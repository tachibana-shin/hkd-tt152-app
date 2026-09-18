#![allow(unused_imports)]

use crate::helpers::*;
use crate::models::*;
use serde_json::json;
use sqlx::sqlite::{SqlitePoolOptions, SqliteTransaction};
use sqlx::SqlitePool;
use tauri::{State, Manager, AppHandle};
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
    sqlx::query(
        "INSERT INTO account (code, name, opening_debit, opening_credit) VALUES (?, ?, ?, ?)
         ON CONFLICT(code) DO UPDATE SET
            name = excluded.name,
            opening_debit = excluded.opening_debit,
            opening_credit = excluded.opening_credit",
    )
    .bind(&code)
    .bind(&name)
    .bind(opening_debit)
    .bind(opening_credit)
    .execute(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    audit(&state, "save", "account", &code).await;
    Ok("ok".into())
}

/// Xóa tài khoản khỏi DMTK. journal_entry lưu mã tài khoản dạng text (không
/// khóa ngoại) nên việc xóa dòng DMTK không làm hỏng bút toán đã ghi.
#[tauri::command]
pub(crate) async fn delete_account(
    state: State<'_, AppState>,
    id: i64,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let row: Option<(String,)> = sqlx::query_as("SELECT code FROM account WHERE id = ?")
        .bind(id)
        .fetch_optional(&*state.pool.read().await)
        .await
        .map_err(|e| e.to_string())?;
    let Some((code,)) = row else {
        return Err("Không tìm thấy tài khoản".into());
    };
    sqlx::query("DELETE FROM account WHERE id = ?")
        .bind(id)
        .execute(&*state.pool.read().await)
        .await
        .map_err(|e| e.to_string())?;
    audit(&state, "delete", "account", &code).await;
    Ok("ok".into())
}

#[tauri::command]
pub(crate) async fn get_products(state: State<'_, AppState>) -> Result<String, String> {
    let rows: Vec<ProductRow> = sqlx::query_as::<_, ProductRow>(
        "SELECT id, code, name, unit, sale_price, cost_price, min_stock, vat_rate, import_tax_rate, is_service, industry_code
         FROM product ORDER BY code",
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
    sqlx::query(
        "INSERT INTO product (code, name, unit, sale_price, cost_price, min_stock, vat_rate, import_tax_rate, is_service, industry_code)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(code) DO UPDATE SET
            name = excluded.name, unit = excluded.unit,
            sale_price = excluded.sale_price, cost_price = excluded.cost_price,
            min_stock = excluded.min_stock, vat_rate = excluded.vat_rate,
            import_tax_rate = excluded.import_tax_rate, is_service = excluded.is_service,
            industry_code = excluded.industry_code",
    )
    .bind(&code)
    .bind(&name)
    .bind(&unit)
    .bind(sale_price)
    .bind(cost_price)
    .bind(min_stock)
    .bind(vat_rate)
    .bind(import_tax_rate)
    .bind(is_service)
    .bind(&industry_code)
    .execute(&*state.pool.read().await)
    .await
    .map_err(|e| e.to_string())?;
    audit(&state, "save", "product", &code).await;
    Ok("ok".into())
}

#[tauri::command]
pub(crate) async fn delete_product(state: State<'_, AppState>, id: i64) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan", "kho"]).await?;
    sqlx::query!("DELETE FROM product WHERE id = ?", id)
        .execute(&*state.pool.read().await)
        .await
        .map_err(|e| {
            format!("Không thể xóa sản phẩm (đã phát sinh nghiệp vụ): {}", e)
        })?;
    Ok("ok".into())
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
    const NUM_FIELDS: &[&str] = &["sale_price", "cost_price", "min_stock", "vat_rate", "import_tax_rate"];
    const SORTABLE: &[&str] = &[
        "id", "code", "name", "unit", "sale_price", "cost_price",
        "min_stock", "vat_rate", "import_tax_rate", "is_service", "industry_code",
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
        f.match_mode.clone().unwrap_or_else(|| "contains".into()).to_lowercase()
    };

    // Filter từng cột với matchMode → SQL.
    for field in TEXT_FIELDS.iter().chain(NUM_FIELDS.iter()) {
        let Some(f) = ev.filters.get(*field) else { continue };
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
                let s = v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string());
                params.push(BindVal::Str(s));
            }
            "endswith" | "ends" => {
                clauses.push(format!("{} LIKE '%' || ?", expr));
                let s = v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string());
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
                let s = v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string());
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
                    let dir = if m.order.unwrap_or(1) < 0 { "DESC" } else { "ASC" };
                    sorts.push(format!("{} {}", f, dir));
                }
            }
        }
    }
    if sorts.is_empty() {
        if let Some(f) = &ev.sort_field {
            if SORTABLE.contains(&f.as_str()) {
                let dir = if ev.sort_order.unwrap_or(1) < 0 { "DESC" } else { "ASC" };
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
            BindVal::Str(s) => { rows_q = rows_q.bind(s.clone()); }
            BindVal::Num(n) => { rows_q = rows_q.bind(*n); }
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
            BindVal::Str(s) => { count_q = count_q.bind(s.clone()); }
            BindVal::Num(n) => { count_q = count_q.bind(*n); }
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
                BindVal::Str(s) => { query = query.bind(s.clone()); }
                BindVal::Num(n) => { query = query.bind(*n); }
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
                BindVal::Str(s) => { query = query.bind(s.clone()); }
                BindVal::Num(n) => { query = query.bind(*n); }
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
        let ev: ProductPageEvent = serde_json::from_str(
            r#"{"sortField":"name; DROP TABLE product","sortOrder":1}"#,
        )
        .unwrap();
        let q = build_product_page_query(&ev).unwrap();
        assert!(!q.sql.to_lowercase().contains("drop table"));
        assert!(q.sql.contains("ORDER BY code ASC"));
    }
}
