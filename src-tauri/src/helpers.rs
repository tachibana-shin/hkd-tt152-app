#![allow(unused_imports)]

use crate::models::{AppState, ProductInfo};
use serde_json::json;
use sqlx::sqlite::{SqlitePoolOptions, SqliteTransaction};
use sqlx::SqlitePool;
use tauri::Manager;
use tauri::State;
pub(crate) async fn resolve_product(
    tx: &mut SqliteTransaction<'_>,
    code: &str,
) -> Result<ProductInfo, String> {
    let row = sqlx::query!(
        r#"SELECT id as "id!", is_service as "is_service: bool", industry_code
             FROM product WHERE code = ?"#,
        code
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(|_| format!("Không tìm thấy sản phẩm có mã '{}'", code))?;
    Ok(ProductInfo {
        id: row.id,
        is_service: row.is_service,
        industry_code: row.industry_code,
    })
}

pub(crate) async fn resolve_warehouse(
    tx: &mut SqliteTransaction<'_>,
    warehouse_code: &str,
    product_code: &str,
) -> Result<i64, String> {
    if !warehouse_code.is_empty() {
        let row = sqlx::query_scalar!(
            r#"SELECT id as "id!" FROM warehouse WHERE code = ?"#,
            warehouse_code
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(|_| format!("Không tìm thấy kho có mã '{}'", warehouse_code))?;
        return Ok(row);
    }
    // fallback: default_warehouse_id của sản phẩm, hoặc kho đầu tiên
    // COALESCE không suy được kiểu từ schema → ép `!: i64` (rỗng sẽ lỗi ở fetch_one).
    let row = sqlx::query_scalar!(
        r#"SELECT COALESCE(p.default_warehouse_id, (SELECT id FROM warehouse ORDER BY id LIMIT 1)) as "wh!: i64"
           FROM product p WHERE p.code = ?"#,
        product_code
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(|_| "Không có kho nào. Vui lòng tạo kho trước.".to_string())?;
    Ok(row)
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn insert_journal_entry(
    tx: &mut SqliteTransaction<'_>,
    posting_date: &str,
    voucher_no: &str,
    entry_type: &str,
    description: &str,
    product_code: &str,
    supplier_code: &str,
    customer_code: &str,
    quantity: f64,
    unit_price: f64,
    amount: f64,
    debit_account: &str,
    credit_account: &str,
    industry_code: &str,
    vat_rate: f64,
    pit_rate: f64,
    unit_code: &str,
    adjust_code: &str,
    note: &str,
    // Số tiền chiết khấu thương mại của dòng (đ). `amount` đã là giá trị SAU chiết
    // khấu; tham số này chỉ để ghi nhận mục chiết khấu trên chứng từ.
    discount: f64,
) -> Result<(), String> {
    sqlx::query!(
        "INSERT INTO journal_entry
         (posting_date, voucher_no, doc_date, description, product_code,
          supplier_code, customer_code, quantity, unit_price, amount, entry_type,
          debit_account, credit_account, industry_code, vat_rate, pit_rate,
          unit_code, adjust_code, note, discount)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        posting_date,
        voucher_no,
        posting_date,
        description,
        product_code,
        supplier_code,
        customer_code,
        quantity,
        unit_price,
        amount,
        entry_type,
        debit_account,
        credit_account,
        industry_code,
        vat_rate,
        pit_rate,
        unit_code,
        adjust_code,
        note,
        discount
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Làm tròn tiền tệ 2 chữ số (dùng cho các phép tính lương/thuế).
pub(crate) fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// Băm mật khẩu SHA-256 với salt ngẫu nhiên 16 byte → định dạng "salt_hex:hash_hex".
pub(crate) fn hash_password(password: &str) -> String {
    use rand::RngCore;
    use sha2::{Digest, Sha256};
    let mut salt = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut salt);
    let salt_hex = hex_encode(&salt);
    let mut hasher = Sha256::new();
    hasher.update(salt_hex.as_bytes());
    hasher.update(password.as_bytes());
    let hash_hex = hex_encode(&hasher.finalize());
    format!("{}:{}", salt_hex, hash_hex)
}

/// Kiểm tra mật khẩu so với chuỗi băm lưu trong DB.
pub(crate) fn verify_password(password: &str, stored: &str) -> bool {
    use sha2::{Digest, Sha256};
    let Some((salt_hex, _)) = stored.split_once(':') else {
        return false;
    };
    let mut hasher = Sha256::new();
    hasher.update(salt_hex.as_bytes());
    hasher.update(password.as_bytes());
    let hash_hex = hex_encode(&hasher.finalize());
    stored == format!("{}:{}", salt_hex, hash_hex)
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Số chứng từ đã dùng chưa — mọi phiếu đều đánh số theo ký hiệu riêng (PN, PX,
/// PT, PC) và số đó là định danh để tra cứu / in / đối chiếu với cơ quan thuế,
/// nên trùng số là dữ liệu sai.
///
/// Phiếu nhập có bảng đầu riêng (`inbound_voucher`, đã có UNIQUE); các phiếu
/// còn lại chỉ tồn tại trên sổ nhật ký nên tra theo `journal_entry`.
pub(crate) async fn voucher_no_taken(
    tx: &mut SqliteTransaction<'_>,
    entry_type: &str,
    voucher_no: &str,
) -> Result<bool, String> {
    let voucher_no = voucher_no.trim();
    let found: Option<i64> = if entry_type == "PN" {
        sqlx::query_scalar!(
            "SELECT id as \"id!\" FROM inbound_voucher WHERE voucher_no = ? COLLATE NOCASE",
            voucher_no
        )
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| e.to_string())?
    } else {
        sqlx::query_scalar!(
            "SELECT id as \"id!\" FROM journal_entry WHERE entry_type = ? AND voucher_no = ? COLLATE NOCASE LIMIT 1",
            entry_type,
            voucher_no
        )
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| e.to_string())?
    };
    Ok(found.is_some())
}

/// Kiểm tra vai trò của người dùng hiện tại cho thao tác ghi.
/// Trả lỗi nếu chưa đăng nhập hoặc vai trò không nằm trong danh sách cho phép.
pub(crate) async fn require_role(
    state: &State<'_, AppState>,
    roles: &[&str],
) -> Result<(), String> {
    let current = state.current_user.lock().await.clone();
    match current {
        None => Err("Chưa đăng nhập. Vui lòng đăng nhập để thực hiện thao tác này.".into()),
        Some(u) if roles.contains(&u.role.as_str()) => Ok(()),
        Some(_) => Err("Bạn không có quyền thực hiện thao tác này.".into()),
    }
}

/// Ghi dấu vết vào bảng `audit_log` (gọi sau khi thao tác thành công),
/// kèm tên người dùng đang đăng nhập (rỗng nếu chưa đăng nhập).
/// Không làm fail nghiệp vụ chính nếu ghi log lỗi (bỏ qua lỗi).
pub(crate) async fn audit(app: &AppState, action: &str, entity: &str, detail: &str) {
    let username = app
        .current_user
        .lock()
        .await
        .as_ref()
        .map(|u| u.username.clone())
        .unwrap_or_default();
    let _ = sqlx::query!(
        "INSERT INTO audit_log (action, entity, detail, username) VALUES (?, ?, ?, ?)",
        action,
        entity,
        detail,
        username
    )
    .execute(&*app.pool.read().await)
    .await;
}

/// Số tiền viết bằng chữ tiếng Việt — cho ô "Số tiền viết bằng chữ" trên hóa đơn.
///
/// Làm tròn tới đồng (hóa đơn không tách lẻ) rồi đọc theo nhóm 3 chữ số, đúng
/// văn phong cổng HĐĐT đang dùng trong DB: *"Một triệu không trăm tám mươi hai
/// nghìn năm trăm năm mươi đồng"* — "không trăm" khi hàng trăm của nhóm bằng 0
/// nhưng còn nhóm cao hơn, "linh" khi hàng chục bằng 0, "mốt"/"lăm" sau hàng chục.
pub(crate) fn vietnamese_amount_in_words(amount: f64) -> String {
    const UNITS: [&str; 10] = [
        "không", "một", "hai", "ba", "bốn", "năm", "sáu", "bảy", "tám", "chín",
    ];
    const SCALES: [&str; 4] = ["", "nghìn", "triệu", "tỷ"];

    /// Đọc 1 nhóm 3 chữ số; `has_higher` = còn nhóm cao hơn nên mới nói "không trăm".
    fn read_group(value: i64, has_higher: bool) -> String {
        /// Nối bằng 1 khoảng trắng, trừ khi đang ở đầu chuỗi.
        fn sep(out: &mut String) {
            if !out.is_empty() {
                out.push(' ');
            }
        }

        let (hundreds, tens, ones) = (value / 100, (value / 10) % 10, value % 10);
        let mut out = String::new();
        if hundreds > 0 {
            out.push_str(UNITS[hundreds as usize]);
            out.push_str(" trăm");
        } else if has_higher && (tens > 0 || ones > 0) {
            out.push_str("không trăm");
        }
        if tens == 0 {
            if ones > 0 {
                // "Ba trăm linh bảy" / "…không trăm linh sáu".
                if hundreds > 0 || has_higher {
                    out.push_str(" linh");
                }
                sep(&mut out);
                out.push_str(UNITS[ones as usize]);
            }
        } else if tens == 1 {
            sep(&mut out);
            out.push_str("mười");
            if ones > 0 {
                sep(&mut out);
                out.push_str(if ones == 5 {
                    "lăm"
                } else {
                    UNITS[ones as usize]
                });
            }
        } else {
            sep(&mut out);
            out.push_str(UNITS[tens as usize]);
            out.push_str(" mươi");
            if ones > 0 {
                out.push(' ');
                out.push_str(match ones {
                    1 => "mốt",
                    5 => "lăm",
                    _ => UNITS[ones as usize],
                });
            }
        }
        out
    }

    let total = amount.round() as i64;
    if total <= 0 {
        return "Không đồng".into();
    }
    // Chia thành nhóm 3 chữ số (số thấp ở đầu danh sách).
    let mut groups: Vec<i64> = Vec::new();
    let mut rest = total;
    while rest > 0 {
        groups.push(rest % 1000);
        rest /= 1000;
    }
    let highest = groups.len() - 1;
    let mut words = Vec::new();
    for i in (0..groups.len()).rev() {
        if groups[i] == 0 {
            continue;
        }
        words.push(read_group(groups[i], i != highest));
        if let Some(scale) = SCALES.get(i).filter(|s| !s.is_empty()) {
            words.push(scale.to_string());
        }
    }
    let sentence = words.join(" ");
    format!("{}{} đồng", sentence[..1].to_uppercase(), &sentence[1..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round2_lam_tron_2_chu_so() {
        assert_eq!(round2(1.234), 1.23);
        assert_eq!(round2(1.236), 1.24);
        assert_eq!(round2(0.0), 0.0);
        assert_eq!(round2(1234.567), 1234.57);
        // Giá trị tiền tệ điển hình vẫn giữ nguyên
        assert_eq!(round2(5_000_000.0), 5_000_000.0);
        assert_eq!(round2(1_075_000.0), 1_075_000.0);
    }

    #[test]
    fn hash_password_co_salt_va_verify_dung() {
        let stored = hash_password("admin123");
        assert!(stored.contains(':'), "định dạng phải là salt:hash");
        assert!(verify_password("admin123", &stored));
        assert!(!verify_password("admin124", &stored));
        assert!(!verify_password("", &stored));
    }

    #[test]
    fn hash_password_salt_ngau_nhien() {
        let a = hash_password("cung-mat-khau");
        let b = hash_password("cung-mat-khau");
        assert_ne!(a, b, "hai lần băm phải khác nhau nhờ salt ngẫu nhiên");
        assert!(verify_password("cung-mat-khau", &a));
        assert!(verify_password("cung-mat-khau", &b));
    }

    #[test]
    fn verify_password_tu_choi_chuoi_hong() {
        assert!(!verify_password("x", ""));
        assert!(!verify_password("x", "khong-co-dau-hai-cham"));
        assert!(!verify_password("x", "abc:def"));
    }

    #[test]
    fn so_tien_bang_chu_theo_van_phong_cua_cong() {
        assert_eq!(vietnamese_amount_in_words(0.0), "Không đồng");
        assert_eq!(vietnamese_amount_in_words(-5.0), "Không đồng");
        assert_eq!(vietnamese_amount_in_words(15.0), "Mười lăm đồng");
        assert_eq!(vietnamese_amount_in_words(21.0), "Hai mươi mốt đồng");
        assert_eq!(vietnamese_amount_in_words(100.0), "Một trăm đồng");
        assert_eq!(
            vietnamese_amount_in_words(307_485.0),
            "Ba trăm linh bảy nghìn bốn trăm tám mươi lăm đồng"
        );
        assert_eq!(
            vietnamese_amount_in_words(6_949_800.0),
            "Sáu triệu chín trăm bốn mươi chín nghìn tám trăm đồng"
        );
        // Hai mẫu dưới đây lấy nguyên văn từ hóa đơn thật trong DB dev.
        assert_eq!(
            vietnamese_amount_in_words(1_082_550.0),
            "Một triệu không trăm tám mươi hai nghìn năm trăm năm mươi đồng"
        );
        assert_eq!(
            vietnamese_amount_in_words(1_992_006.0),
            "Một triệu chín trăm chín mươi hai nghìn không trăm linh sáu đồng"
        );
        // Làm tròn tới đồng.
        assert_eq!(
            vietnamese_amount_in_words(1_082_550.4),
            "Một triệu không trăm tám mươi hai nghìn năm trăm năm mươi đồng"
        );
    }
}
