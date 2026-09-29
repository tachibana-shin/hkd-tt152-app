//! Phân trang server-side dùng chung cho các bảng dữ liệu lớn.
//!
//! DataTable ở chế độ lazy gửi một object `lazyEvent` (first/rows/sortField/
//! sortOrder/filters.global.value) mỗi lần người dùng đổi trang, sắp xếp hoặc lọc.
//! Các lệnh `*_page` nhận object đó dưới dạng JSON, dựng truy vấn LIMIT/OFFSET
//! kèm `COUNT(*)` để bảng hiển thị đúng số trang — thay vì kéo toàn bộ dữ liệu
//! về client rồi cắt trang ở trình duyệt (chậm khi dữ liệu phình to).
//!
//! Cách dùng (xem `get_audit_log_page` làm mẫu):
//! ```ignore
//! let ev: PageEvent = serde_json::from_str(&lazy_event)?;
//! let page = ev.page();
//! let (where_sql, params) = global_where(&["name", "code"], &ev);
//! let order = order_by(&[("id", "id"), ("name", "name")], &ev, "id DESC");
//! let sql = format!("SELECT … FROM t{} ORDER BY {} LIMIT ? OFFSET ?", where_sql, order);
//! let (rows, total) = fetch_page(&pool, &sql, &format!("SELECT COUNT(*) FROM t{}", where_sql), &params, &page).await?;
//! ```

use serde::Deserialize;
use sqlx::sqlite::{SqlitePool, SqliteRow};
use sqlx::{FromRow, Row};

/// Ô tìm kiếm toàn cục của DataTable (mọi bảng đều có, kiểu Products).
///
/// `rename_all = "camelCase"`: DataTable gửi `matchMode`/`sortField`/`sortOrder`
/// đúng kiểu camelCase. Không khai thì serde không khớp khoá và im lặng bỏ qua —
/// kiểu sắp xếp bị bỏ mà không có lỗi nào báo ra.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PageFilter {
    pub value: Option<String>,
    #[allow(dead_code)]
    pub match_mode: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PageFilters {
    pub global: Option<PageFilter>,
    /// Lọc theo từng cột (hàng filter của DataTable): khoá là tên field.
    #[serde(flatten)]
    pub columns: std::collections::HashMap<String, PageFilter>,
}

/// Sự kiện phân trang từ DataTable (lazy mode).
///
/// Mọi trường đều có mặc định: một client gửi thiếu `first`/`rows` vẫn được
/// hiểu (về trang đầu, kích thước mặc định) thay vì làm hỏng cả lệnh.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PageEvent {
    /// Số dòng bị bỏ qua (0-based).
    #[serde(default)]
    pub first: i64,
    /// Số dòng mỗi trang.
    #[serde(default = "default_rows")]
    pub rows: i64,
    pub sort_field: Option<String>,
    pub sort_order: Option<i64>,
    pub filters: Option<PageFilters>,
}

/// Kích thước trang khi client không gửi `rows`.
fn default_rows() -> i64 {
    50
}

/// Trang đã chuẩn hoá: kích thước trong 1..=200, offset không âm.
pub(crate) struct Page {
    pub size: i64,
    pub offset: i64,
}

impl PageEvent {
    /// Chặn kích thước trang hợp lý: trang quá nhỏ thì vẫn đọc được, quá lớn
    /// thì không ai cố tình kéo 10 nghìn dòng về.
    pub(crate) fn page(&self) -> Page {
        let size = self.rows.clamp(1, 200);
        Page {
            size,
            offset: self.first.max(0),
        }
    }

    /// Từ khoá tìm kiếm toàn cục (đã bỏ khoảng trắng, rỗng = không lọc).
    pub(crate) fn global_keyword(&self) -> Option<String> {
        let v = self
            .filters
            .as_ref()
            .and_then(|f| f.global.as_ref())
            .and_then(|g| g.value.as_ref())
            .map(|s| s.trim().to_string())?;
        if v.is_empty() {
            None
        } else {
            Some(v)
        }
    }
}

/// Giá trị ràng buộc cho truy vấn. Hiện mọi bộ lọc đều là chuỗi (ngày, từ
/// khoá LIKE) nên chỉ cần một biến thể — thêm biến thể số khi nào thật sự cần
/// ràng buộc số, kèm dấu hiệu `#[cfg(test)]` nếu chỉ dùng trong test.
#[derive(Clone)]
pub(crate) struct BindVal(pub(crate) String);

/// Bọc từ khoá thành mẫu LIKE, khoá ký tự đại diện để người dùng gõ `%`
/// cũng chỉ tìm đúng chữ đó.
fn like_value(raw: &str) -> String {
    format!("%{}%", raw.replace('%', "\\%").replace('_', "\\_"))
}

/// Ghép mệnh đề `WHERE` từ ô tìm kiếm toàn cục: mỗi cột cho phép được so
/// sánh `LIKE '%từ khoá%'`. Trả về chuỗi bắt đầu bằng " WHERE …" (rỗng nếu
/// không lọc) kèm danh sách giá trị ràng buộc theo đúng thứ tự.
pub(crate) fn global_where(keyword: Option<String>, columns: &[&str]) -> (String, Vec<BindVal>) {
    // Tự trim ở đây thay vì tin vào nơi gọi: khoảng trắng thừa = không lọc.
    let kw = keyword.unwrap_or_default().trim().to_string();
    if kw.is_empty() {
        return (String::new(), Vec::new());
    }
    let like = like_value(&kw);
    let conds = columns
        .iter()
        .map(|c| format!("{c} LIKE ? ESCAPE '\\'"))
        .collect::<Vec<_>>()
        .join(" OR ");
    // Mỗi cột một dấu `?` nên phải bind N lần — chỉ bind một lần sẽ ra
    // "datatype mismatch"/thiếu tham số và ô tìm kiếm luôn rỗng.
    (
        format!(" WHERE ({conds})"),
        vec![BindVal(like.clone()); columns.len()],
    )
}

/// Ghép thêm điều kiện AND cho các cột có lọc (hàng filter của DataTable).
/// Chỉ nhận cột trong danh sách cho phép — cùng lý do an toàn như `order_by`.
pub(crate) fn column_where(ev: &PageEvent, columns: &[&str]) -> (String, Vec<BindVal>) {
    let Some(filters) = ev.filters.as_ref() else {
        return (String::new(), Vec::new());
    };
    let mut conds: Vec<String> = Vec::new();
    let mut params: Vec<BindVal> = Vec::new();
    for col in columns {
        let Some(f) = filters.columns.get(*col) else {
            continue;
        };
        let v = f.value.as_deref().unwrap_or("").trim();
        if v.is_empty() {
            continue;
        }
        // Số → so sánh bằng; chuỗi → LIKE. Mã/tên/MST đều là chuỗi nên nhánh
        // số hiếm, nhưng để sẵn để sau này lọc theo số tiền khỏi sửa lại.
        if let Ok(n) = v.parse::<f64>() {
            conds.push(format!("CAST({col} AS REAL) = ?"));
            params.push(BindVal(n.to_string()));
        } else {
            conds.push(format!("{col} LIKE ? ESCAPE '\\'"));
            params.push(BindVal(like_value(v)));
        }
    }
    if conds.is_empty() {
        (String::new(), Vec::new())
    } else {
        (format!(" AND {}", conds.join(" AND ")), params)
    }
}

/// `ORDER BY` từ sortField/sortOrder của DataTable.
///
/// `allowed` liệt kê cặp (tên field JSON → tên cột SQL) được phép sắp xếp —
/// tên cột không bao giờ được ghép thẳng vào SQL từ dữ liệu người dùng, nên
/// danh sách này là lớp chống injection duy nhất. Không có field hợp lệ thì
/// dùng `fallback`.
pub(crate) fn order_by(allowed: &[(&str, &str)], ev: &PageEvent, fallback: &str) -> String {
    let field = ev.sort_field.as_deref().unwrap_or("");
    let dir = match ev.sort_order {
        Some(1) => "ASC",
        _ => "DESC",
    };
    match allowed.iter().find(|(json, _)| *json == field) {
        Some((_, col)) => format!("{col} {dir}"),
        None => fallback.to_string(),
    }
}

/// Chạy truy vấn 1 trang + đếm tổng số dòng (cùng mệnh đề WHERE).
///
/// Hai truy vấn dùng chung `params` nên thứ tự `?` phải khớp nhau: đặt tất cả
/// điều kiện WHERE trước, LIMIT/OFFSET luôn ở cuối.
pub(crate) async fn fetch_page<T>(
    pool: &SqlitePool,
    sql: &str,
    count_sql: &str,
    params: &[BindVal],
    page: &Page,
) -> Result<(Vec<T>, i64), String>
where
    T: for<'r> FromRow<'r, SqliteRow> + Send + Unpin,
{
    let mut rows_q = sqlx::query_as::<_, T>(sql);
    for p in params {
        rows_q = rows_q.bind(p.0.clone());
    }
    let rows = rows_q
        .bind(page.size)
        .bind(page.offset)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

    let mut count_q = sqlx::query_scalar::<_, i64>(count_sql);
    for p in params {
        count_q = count_q.bind(p.0.clone());
    }
    let total = count_q.fetch_one(pool).await.map_err(|e| e.to_string())?;
    Ok((rows, total))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(json: &str) -> PageEvent {
        serde_json::from_str(json).expect("lazy_event hợp lệ")
    }

    #[test]
    fn kich_thuoc_trang_luon_hop_le() {
        // first âm (DataTable đôi khi trả -1) và rows quá lớn đều bị chặn lại.
        let p = ev(r#"{"first": -10, "rows": 9999}"#).page();
        assert_eq!(p.offset, 0);
        assert_eq!(p.size, 200);
        // rows = 0 (chưa chọn) → tối thiểu 1 để không hỏng LIMIT.
        assert_eq!(ev(r#"{"rows": 0}"#).page().size, 1);
    }

    #[test]
    fn tu_khoa_giu_ky_tu_dai_dien() {
        // Người dùng gõ % hoặc _ không được khớp mọi thứ.
        let (sql, params) = global_where(Some("50%_off".into()), &["name"]);
        assert!(sql.contains("LIKE ?"));
        assert_eq!(params[0].0, "%50\\%\\_off%");
        // Rỗng/khoảng trắng = không lọc, không sinh mệnh đề.
        assert_eq!(global_where(None, &["name"]).0, "");
        assert_eq!(global_where(Some("   ".into()), &["name"]).0, "");
    }

    #[test]
    fn sap_xep_chi_nhan_cot_trong_danh_sach_cho_phep() {
        let allowed = [("id", "id"), ("name", "name")];
        let ok = ev(r#"{"sortField": "name", "sortOrder": 1}"#);
        assert_eq!(order_by(&allowed, &ok, "id DESC"), "name ASC");
        let desc = ev(r#"{"sortField": "name", "sortOrder": -1}"#);
        assert_eq!(order_by(&allowed, &desc, "id DESC"), "name DESC");
        // Field lạ (thử chèn SQL) → rơi về sắp xếp mặc định.
        let evil = ev(r#"{"sortField": "id; DROP TABLE audit_log", "sortOrder": 1}"#);
        assert_eq!(order_by(&allowed, &evil, "id DESC"), "id DESC");
        // sortOrder thiếu/lạ → mặc định DESC.
        assert_eq!(
            order_by(&allowed, &ev(r#"{"sortField": "name"}"#), "id DESC"),
            "name DESC"
        );
    }

    #[test]
    fn loc_theo_cot_bang_and_bo_qua_o_rong() {
        let e = ev(
            r#"{"filters": {"code": {"value": "SP0"}, "name": {"value": "  "},
                        "address": {"value": "Hà Nội"}, "id": {"value": "12"}}}"#,
        );
        let (sql, params) = column_where(&e, &["code", "name", "address", "id"]);
        // Ô rỗng bị bỏ qua; id là số nên so sánh bằng, chuỗi thì LIKE.
        assert_eq!(
            sql,
            " AND code LIKE ? ESCAPE '\\' AND address LIKE ? ESCAPE '\\' AND CAST(id AS REAL) = ?"
        );
        assert_eq!(params.len(), 3);
        assert_eq!(params[0].0, "%SP0%");
        assert_eq!(params[1].0, "%Hà Nội%");
        assert_eq!(params[2].0, "12");
        // Cột không khai trong danh sách cho phép thì bị bỏ qua.
        assert_eq!(column_where(&e, &["name"]).0, "");
    }
}
