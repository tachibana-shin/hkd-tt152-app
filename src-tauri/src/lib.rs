#![allow(unused_imports)]

mod commands;
mod helpers;
mod models;
#[cfg(test)]
mod test_support;

use crate::models::AppState;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use std::str::FromStr;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_dir = app
                .path()
                .app_data_dir()
                .expect("failed to get app data dir");
            std::fs::create_dir_all(&app_dir).ok();
            let profiles_dir = app_dir.join("profiles");
            std::fs::create_dir_all(&profiles_dir).ok();

            // Đa hồ sơ (multi-profile): mỗi hộ kinh doanh = 1 thư mục riêng chứa 1 file DB riêng.
            // Mở hồ sơ đang chọn trong active_profile.json; lần chạy đầu tiên → tạo "HKD mặc định".
            let active_key = commands::profile::read_active_key(&app_dir)
                .filter(|k| profiles_dir.join(k).join("hkd.db").is_file());
            let key = match active_key {
                Some(k) => k,
                None => {
                    let key = "default".to_string();
                    let dir = profiles_dir.join(&key);
                    std::fs::create_dir_all(&dir).ok();
                    let name = "HKD mặc định".to_string();
                    commands::profile::write_profile_meta(&dir, &name);
                    commands::profile::set_active(&app_dir, &key, &name);
                    key
                }
            };

            let db_path = profiles_dir.join(&key).join("hkd.db");
            // WAL + FK được set qua builder (SQLx chỉ nhận mode/cache/immutable/vfs trong URL)
            let opts = SqliteConnectOptions::from_str(&format!("sqlite:{}", db_path.display()))
                .expect("bad database path")
                .create_if_missing(true)
                .journal_mode(SqliteJournalMode::Wal)
                .foreign_keys(true);

            let pool = tauri::async_runtime::block_on(async {
                let pool = SqlitePoolOptions::new()
                    .max_connections(1)
                    .connect_with(opts)
                    .await
                    .expect("failed to connect to database");

                // Migration + seed admin mặc định cho hồ sơ đang mở
                commands::profile::init_database(&pool).await;

                pool
            });

            app.manage(AppState {
                pool: tokio::sync::RwLock::new(pool),
                current_user: tokio::sync::Mutex::new(None),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::accounting::get_ledger,
            commands::accounting::get_tax_summary,
            commands::accounting::get_tax_declaration,
            commands::accounting::get_revenue_expense,
            commands::accounting::get_inventory_summary,
            commands::accounting::get_vat_reduction_list,
            commands::admin::create_backup,
            commands::admin::list_backups,
            commands::admin::restore_backup,
            commands::attendance::get_attendance,
            commands::attendance::save_attendance,
            commands::attendance::get_attendance_work_days,
            commands::audit::log_audit,
            commands::audit::get_audit_log,
            commands::auth::login,
            commands::auth::logout,
            commands::auth::get_current_user,
            commands::auth::list_users,
            commands::auth::save_user,
            commands::auth::delete_user,
            commands::auth::change_password,
            commands::business::get_business_info,
            commands::business::get_business_config,
            commands::business::save_business_config,
            commands::cash::save_cash_entry,
            commands::catalog::get_accounts,
            commands::catalog::get_products,
            commands::catalog::get_industry_groups,
            commands::catalog::get_warehouses,
            commands::catalog::save_warehouse,
            commands::catalog::get_suppliers,
            commands::catalog::save_supplier,
            commands::catalog::get_customers,
            commands::catalog::save_customer,
            commands::catalog::save_product,
            commands::catalog::delete_product,
            commands::hddt::hddt_status,
            commands::hddt::hddt_send_simulated,
            commands::import::import_nhap_lieu,
            commands::inventory::get_inventory_counts,
            commands::inventory::get_inventory_count_detail,
            commands::inventory::save_inventory_count,
            commands::invoice::get_invoices,
            commands::invoice::get_invoice_detail,
            commands::invoice::save_invoice,
            commands::invoice::link_hddt,
            commands::payroll::get_employees,
            commands::payroll::save_employee,
            commands::payroll::get_payroll_periods,
            commands::payroll::get_payroll,
            commands::payroll::save_payroll,
            commands::profile::get_profiles,
            commands::profile::create_profile,
            commands::profile::rename_profile,
            commands::profile::delete_profile,
            commands::profile::switch_profile,
            commands::stock::save_inbound,
            commands::stock::save_outbound,
            commands::stock::get_journal_entries,
            commands::stock::get_voucher,
            commands::stock::get_stock_lots,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
