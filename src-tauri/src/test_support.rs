//! Tiện ích dùng chung cho test (`cargo test`): DB SQLite in-memory đã chạy
//! migration + seed, và các hàm seed dữ liệu nghiệp vụ tối thiểu.
#![allow(dead_code)]

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;

/// Pool SQLite in-memory đã chạy đủ migration + seed người dùng mặc định.
pub(crate) async fn test_pool() -> SqlitePool {
    let opts = SqliteConnectOptions::from_str("sqlite::memory:")
        .expect("parse sqlite url")
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .connect_with(opts)
        .await
        .expect("open in-memory sqlite");
    crate::commands::profile::init_database(&pool).await;
    pool
}

pub(crate) async fn seed_product(pool: &SqlitePool, code: &str, name: &str, vat_rate: f64) -> i64 {
    sqlx::query!(
        "INSERT INTO product (code, name, unit, vat_rate) VALUES (?, ?, 'Cái', ?)",
        code,
        name,
        vat_rate
    )
    .execute(pool)
    .await
    .expect("seed product");
    sqlx::query_scalar!(r#"SELECT id as "id!" FROM product WHERE code = ?"#, code)
        .fetch_one(pool)
        .await
        .expect("product id")
}

pub(crate) async fn seed_warehouse(pool: &SqlitePool, code: &str, name: &str) -> i64 {
    sqlx::query!(
        "INSERT INTO warehouse (code, name) VALUES (?, ?)",
        code,
        name
    )
    .execute(pool)
    .await
    .expect("seed warehouse");
    sqlx::query_scalar!(r#"SELECT id as "id!" FROM warehouse WHERE code = ?"#, code)
        .fetch_one(pool)
        .await
        .expect("warehouse id")
}

pub(crate) async fn seed_customer(pool: &SqlitePool, code: &str, name: &str) -> i64 {
    let tax_code = format!("MST-{}", code);
    sqlx::query!(
        "INSERT INTO customer (code, name, tax_code) VALUES (?, ?, ?)",
        code,
        name,
        tax_code
    )
    .execute(pool)
    .await
    .expect("seed customer");
    sqlx::query_scalar!(r#"SELECT id as "id!" FROM customer WHERE code = ?"#, code)
        .fetch_one(pool)
        .await
        .expect("customer id")
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn seed_employee(
    pool: &SqlitePool,
    code: &str,
    name: &str,
    basic_salary: f64,
    allowance_cv: f64,
    allowance_xx: f64,
    allowance_phone: f64,
    bh_salary: f64,
) -> i64 {
    sqlx::query!(
        "INSERT INTO employee (code, name, department, position, job, basic_salary,
             allowance_cv, allowance_xx, allowance_phone, bh_salary, hired_on, left_on, dependents)
         VALUES (?, ?, '', '', '', ?, ?, ?, ?, ?, '', '', 0)",
        code,
        name,
        basic_salary,
        allowance_cv,
        allowance_xx,
        allowance_phone,
        bh_salary
    )
    .execute(pool)
    .await
    .expect("seed employee");
    sqlx::query_scalar!(r#"SELECT id as "id!" FROM employee WHERE code = ?"#, code)
        .fetch_one(pool)
        .await
        .expect("employee id")
}

pub(crate) async fn add_stock_lot(
    pool: &SqlitePool,
    product_id: i64,
    warehouse_id: i64,
    quantity: f64,
    unit_cost: f64,
    received_at: &str,
) {
    sqlx::query!(
        "INSERT INTO stock_lot (product_id, warehouse_id, quantity, unit_cost, received_at, depleted)
         VALUES (?, ?, ?, ?, ?, 0)",
        product_id,
        warehouse_id,
        quantity,
        unit_cost,
        received_at
    )
    .execute(pool)
    .await
    .expect("add stock lot");
}

/// Chèn 1 dòng bán hàng (PX) vào sổ nhật ký — dùng cho test tổng hợp thuế.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn add_journal_px(
    pool: &SqlitePool,
    posting_date: &str,
    voucher_no: &str,
    product_code: &str,
    industry_code: &str,
    quantity: f64,
    unit_price: f64,
    amount: f64,
    vat_rate: f64,
    pit_rate: f64,
) {
    sqlx::query!(
        "INSERT INTO journal_entry
         (posting_date, voucher_no, doc_date, entry_type, description, product_code,
          quantity, unit_price, amount, debit_account, credit_account,
          industry_code, vat_rate, pit_rate, unit_code, adjust_code, note)
         VALUES (?, ?, ?, 'PX', 'test', ?, ?, ?, ?, '131', '511', ?, ?, ?, 'HKD', '', '')",
        posting_date,
        voucher_no,
        posting_date,
        product_code,
        quantity,
        unit_price,
        amount,
        industry_code,
        vat_rate,
        pit_rate
    )
    .execute(pool)
    .await
    .expect("add journal px");
}

/// Đếm/scalar với SQL do test truyền vào → runtime query (macro cần SQL hằng số).
pub(crate) async fn scalar_i64(pool: &SqlitePool, sql: &str) -> i64 {
    sqlx::query_scalar(sql)
        .fetch_one(pool)
        .await
        .expect("scalar i64")
}

/// Xem `scalar_i64`.
pub(crate) async fn scalar_f64(pool: &SqlitePool, sql: &str) -> f64 {
    sqlx::query_scalar(sql)
        .fetch_one(pool)
        .await
        .expect("scalar f64")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helpers::verify_password;

    #[tokio::test]
    async fn migrations_create_schema_and_seed_admin() {
        let pool = test_pool().await;
        let tables = scalar_i64(
            &pool,
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table'",
        )
        .await;
        assert!(tables >= 20, "chỉ có {tables} bảng — migration thiếu?");

        // Cấu hình + seed danh mục từ migration
        let groups = scalar_i64(&pool, "SELECT COUNT(*) FROM industry_group").await;
        assert_eq!(groups, 5);
        assert_eq!(
            scalar_i64(&pool, "SELECT COUNT(*) FROM business_unit").await,
            1
        );

        // seed_default_users tạo admin/admin123 (băm SHA-256, không seed tĩnh được)
        let hash: String =
            sqlx::query_scalar!("SELECT password_hash FROM app_user WHERE username = 'admin'")
                .fetch_one(&pool)
                .await
                .expect("admin user seeded");
        assert!(verify_password("admin123", &hash));
        assert!(!verify_password("sai-mat-khau", &hash));
    }
}
