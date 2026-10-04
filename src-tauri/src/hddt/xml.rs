//! Lưu trữ file XML hóa đơn điện tử tải từ cổng HĐĐT.
//!
//! Quy định: người bán, người mua phải lưu trữ hóa đơn điện tử ở dạng file
//! XML. Cổng có endpoint `GET /api/{query|sco-query}/invoices/export-xml?nbmst&
//! khhdon&shdon&khmshdon` trả về **ZIP** chứa `invoice.xml` + `invoice.html` +
//! ảnh phụ trợ (bắt từ bundle portal 10/2026, kiểm chứng live 04/10/2026 —
//! 3/3 tab tải được ZIP ~300KB, bên trong `invoice.xml` 2–18KB).
//!
//! App chỉ giữ `invoice.xml` (đúng phần bắt buộc lưu trữ — xem hỏi người dùng
//! 04/10/2026), bỏ phần render HTML/ ảnh vì app tự render PDF được từ
//! `detail_json`.
//!
//! **Nơi lưu**: nội dung nằm trong cột `hddt_invoice_xml.xml_body` (nguồn sự
//! thật — đọc/nộp/gửi không phụ thuộc file còn trên đĩa, và backup đi theo
//! DB). Song song đó vẫn ghi bản sao `.xml` vào `…/profiles/<key>/hddt_xml/`
//! để mở/tải ra tay; file mất thì ghi lại từ blob chứ không tải lại cổng.
//!
//! Bốn đường vào:
//!   * [`fill_missing_after_scan`] — chạy sau "Quét cổng" (màn Đồng bộ);
//!   * [`fill_missing_before_import`] — chạy **sau** khi "Nhập kho" đã tạo
//!     phiếu xong (backfill hóa đơn quét từ trước khi có tính năng này) để
//!     việc lưu trữ không được làm chậm hay làm hỏng việc tạo phiếu;
//!   * [`save_manual`] — nút "Tải XML" từng dòng (màn Tra cứu / Đồng bộ);
//!   * [`export_zip`] — nút "Tải ZIP XML" gộp nhiều hóa đơn để nộp/gửi.

use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use crate::hddt::{HddtClient, InvoiceDirection, InvoiceKind};

/// Khóa danh tính 1 hóa đơn — đúng 4 tham số bắt buộc của endpoint export-xml
/// cộng chiều (bán/vào) và loại (HĐĐT/máy tính tiền).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct XmlInvoiceKey {
    pub(crate) direction: InvoiceDirection,
    pub(crate) kind: InvoiceKind,
    pub(crate) nbmst: String,
    pub(crate) khmshdon: i64,
    pub(crate) khhdon: String,
    pub(crate) shdon: String,
}

/// Kết quả đã lưu 1 file XML.
#[derive(Debug, serde::Serialize)]
pub(crate) struct SavedXml {
    /// Tên file trong thư mục `hddt_xml/` (frontend hiển thị + mở file).
    pub(crate) file_name: String,
    pub(crate) byte_size: i64,
    /// `true` nếu đã lưu từ trước — lần gọi này không tải lại.
    pub(crate) already: bool,
}

/// Một dòng đã lưu (để frontend tra "hóa đơn này có file XML chưa" + chọn ra
/// ZIP nộp/gửi).
#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub(crate) struct SavedXmlRow {
    /// Id trong `hddt_invoice_xml` — frontend gửi lại danh sách này cho
    /// [`export_zip`] khi người dùng chọn hóa đơn để nộp/gửi.
    pub(crate) id: i64,
    pub(crate) portal_id: String,
    pub(crate) direction: String,
    pub(crate) kind: String,
    pub(crate) nbmst: String,
    pub(crate) khmshdon: i64,
    pub(crate) khhdon: String,
    pub(crate) shdon: String,
    pub(crate) file_name: String,
    pub(crate) byte_size: i64,
    /// `auto` = tải lúc quét/nhập, `manual` = người dùng bấm nút.
    pub(crate) source: String,
    pub(crate) saved_at: String,
}

/// Số liệu tự động lưu XML sau khi quét/nhập.
#[derive(Debug, Default, serde::Serialize)]
pub(crate) struct FillOutcome {
    /// Đã có file từ trước (không đụng tới).
    pub(crate) skipped: usize,
    /// Tải + lưu mới trong lượt này.
    pub(crate) saved: usize,
    /// Lỗi (cổng không có hồ sơ gốc, hết phiên, mạng…) — không chặn luồng chính.
    pub(crate) failed: usize,
    /// Lỗi đầu tiên, báo ra UI cho người biết vì sao file thiếu.
    pub(crate) first_error: String,
}

impl FillOutcome {
    fn fail(&mut self, msg: impl Into<String>) {
        self.failed += 1;
        if self.first_error.is_empty() {
            self.first_error = msg.into();
        }
    }
}

// ─── Đường dẫn & tên file ───

/// Thư mục chứa XML của hồ sơ đang mở: `<profile>/hddt_xml/`.
///
/// Nhận sẵn đường dẫn (không lấy từ `AppHandle`) để test dùng tempdir.
pub(crate) fn xml_dir_under(profile_dir: &Path) -> PathBuf {
    profile_dir.join("hddt_xml")
}

/// Tên file = đúng khóa danh tính → deterministic, không cần cột để dò lại.
///
/// Chỉ giữ ký tự an toàn cho tên file; rỗng → gán `-` để không ra tên trống.
fn safe_part(s: &str) -> String {
    let out: String = s
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    if out.is_empty() {
        "-".to_string()
    } else {
        out
    }
}

fn file_name_of(key: &XmlInvoiceKey) -> String {
    format!(
        "{}_{}_{}_{}_{}.xml",
        key.direction.as_str(),
        key.kind.as_str(),
        safe_part(&key.nbmst),
        safe_part(&key.khhdon),
        safe_part(&key.shdon),
    )
}

// ─── Đọc ZIP ───

/// Trích `invoice.xml` ra khỏi ZIP do cổng trả về.
///
/// Cổng đặt tên entry là `invoice.xml` (đã đối chiếu 3 file live 04/10/2026);
/// nếu lệch tên thì lấy file `.xml` đầu tiên thay vì fail.
pub(crate) fn extract_invoice_xml(zip_bytes: &[u8]) -> Result<Vec<u8>, String> {
    let cursor = std::io::Cursor::new(zip_bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| format!("File ZIP không hợp lệ: {e}"))?;

    let mut target: Option<String> = None;
    let mut fallback: Option<String> = None;
    for i in 0..archive.len() {
        let entry = archive
            .by_index(i)
            .map_err(|e| format!("Đọc mục trong ZIP lỗi: {e}"))?;
        let name = entry.name().to_string();
        drop(entry);
        if name.eq_ignore_ascii_case("invoice.xml") {
            target = Some(name);
            break;
        }
        if fallback.is_none() && name.to_ascii_lowercase().ends_with(".xml") {
            fallback = Some(name);
        }
    }
    let name = target
        .or(fallback)
        .ok_or_else(|| "ZIP không chứa file XML nào (cổng chỉ trả HTML/ảnh)".to_string())?;
    let mut entry = archive
        .by_name(&name)
        .map_err(|e| format!("Không mở được {name} trong ZIP: {e}"))?;
    let mut out = Vec::new();
    std::io::Read::read_to_end(&mut entry, &mut out)
        .map_err(|e| format!("Đọc {name} trong ZIP lỗi: {e}"))?;
    if out.is_empty() {
        return Err(format!("{name} trong ZIP rỗng"));
    }
    Ok(out)
}

// ─── Ghi file + bảng ───

/// Ghi nội dung XML vào CSDL (`xml_body` — nguồn sự thật) + bản sao file vào
/// thư mục hồ sơ.
///
/// Idempotent: trùng khóa thì thay file và bản ghi cũ (nội dung hóa đơn không
/// đổi giữa hai lần tải, nhưng tránh tích tụ file rác nếu đổi tên file sau này).
pub(crate) async fn save_from_zip(
    pool: &SqlitePool,
    dir: &Path,
    key: &XmlInvoiceKey,
    zip_bytes: &[u8],
    portal_id: &str,
    source: &str,
) -> Result<SavedXml, String> {
    let xml = extract_invoice_xml(zip_bytes)?;
    std::fs::create_dir_all(dir).map_err(|e| format!("Tạo thư mục XML lỗi: {e}"))?;

    let file_name = file_name_of(key);
    let path = dir.join(&file_name);
    std::fs::write(&path, &xml).map_err(|e| format!("Ghi file {file_name} lỗi: {e}"))?;

    let byte_size = xml.len() as i64;
    let saved_at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let direction = key.direction.as_str();
    let kind = key.kind.as_str();
    let nbmst = key.nbmst.clone();
    let khhdon = key.khhdon.clone();
    let shdon = key.shdon.clone();
    let khmshdon = key.khmshdon;
    let file_name2 = file_name.clone();
    let body: &[u8] = xml.as_slice();
    sqlx::query!(
        r#"INSERT INTO hddt_invoice_xml
               (portal_id, direction, kind, nbmst, khmshdon, khhdon, shdon,
                file_name, byte_size, source, saved_at, xml_body)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
           ON CONFLICT (direction, kind, nbmst, khmshdon, khhdon, shdon)
           DO UPDATE SET portal_id = excluded.portal_id,
                         file_name = excluded.file_name,
                         byte_size = excluded.byte_size,
                         source = excluded.source,
                         saved_at = excluded.saved_at,
                         xml_body = excluded.xml_body"#,
        portal_id,
        direction,
        kind,
        nbmst,
        khmshdon,
        khhdon,
        shdon,
        file_name2,
        byte_size,
        source,
        saved_at,
        body,
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Ghi bản ghi XML lỗi: {e}"))?;

    Ok(SavedXml {
        file_name,
        byte_size,
        already: false,
    })
}

/// Đã lưu chưa — `xml_body` trong CSDL là nguồn sự thật, không phụ thuộc file
/// còn trên đĩa.
///
/// Trả `(tên file, cỡ byte)` hoặc `None` khi chưa có. Dòng lưu trước khi thêm
/// cột `xml_body` sẽ được nạp blob lại từ bản sao `.xml` còn trên đĩa (không
/// mất một request cổng nào); cả file cũng mất thì mới coi là chưa lưu.
async fn existing(
    pool: &SqlitePool,
    dir: &Path,
    key: &XmlInvoiceKey,
) -> Result<Option<(String, i64)>, String> {
    let direction = key.direction.as_str();
    let kind = key.kind.as_str();
    let row: Option<(i64, String, i64, Option<Vec<u8>>)> = sqlx::query_as(
        r#"SELECT id, file_name, byte_size, xml_body FROM hddt_invoice_xml
            WHERE direction = ? AND kind = ? AND nbmst = ?
              AND khmshdon = ? AND khhdon = ? AND shdon = ?"#,
    )
    .bind(direction)
    .bind(kind)
    .bind(&key.nbmst)
    .bind(key.khmshdon)
    .bind(&key.khhdon)
    .bind(&key.shdon)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Đọc bản ghi XML lỗi: {e}"))?;

    let Some((id, name, size, body)) = row else {
        return Ok(None);
    };
    if body.as_deref().is_some_and(|b| !b.is_empty()) {
        return Ok(Some((name, size)));
    }

    // Chưa có blob: đọc bản sao file rồi ghi vào CSDL để lần sau khỏi đụng đĩa.
    match body_from_disk(pool, dir, id, &name).await? {
        Some(bytes) => Ok(Some((name, bytes.len() as i64))),
        None => Ok(None),
    }
}

/// Dòng chưa có `xml_body` → đọc bản sao file `.xml` rồi ghi vào CSDL.
///
/// Trả `None` khi file cũng mất (lúc đó mới thật sự phải tải lại cổng). Dùng
/// chung cho lúc kiểm "đã lưu chưa" và lúc gộp ZIP nộp/gửi.
async fn body_from_disk(
    pool: &SqlitePool,
    dir: &Path,
    id: i64,
    file_name: &str,
) -> Result<Option<Vec<u8>>, String> {
    let bytes = std::fs::read(dir.join(file_name)).unwrap_or_default();
    if bytes.is_empty() {
        return Ok(None);
    }
    // Biểu thức tạm bind trước: macro giữ tham số cho tới hết `.await`.
    let body: &[u8] = bytes.as_slice();
    let size = bytes.len() as i64;
    sqlx::query!(
        "UPDATE hddt_invoice_xml SET xml_body = ?, byte_size = ? WHERE id = ?",
        body,
        size,
        id,
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Bổ sung nội dung XML vào CSDL lỗi: {e}"))?;
    Ok(Some(bytes))
}

/// Tải XML 1 hóa đơn từ cổng rồi lưu (trừ khi đã có trong CSDL).
///
/// `source` = `auto` (quét/nhập) hoặc `manual` (nút tay).
pub(crate) async fn download_one(
    pool: &SqlitePool,
    dir: &Path,
    client: &HddtClient,
    token: &str,
    key: &XmlInvoiceKey,
    portal_id: &str,
    source: &str,
) -> Result<SavedXml, String> {
    if let Some((name, size)) = existing(pool, dir, key).await? {
        return Ok(SavedXml {
            file_name: name,
            byte_size: size,
            already: true,
        });
    }
    let zip = client.export_invoice_xml(token, key).await?;
    save_from_zip(pool, dir, key, zip.as_slice(), portal_id, source).await
}

/// Nút tay: tải XML 1 hóa đơn (kể cả khi đã có → trả `already = true`).
pub(crate) async fn save_manual(
    pool: &SqlitePool,
    dir: &Path,
    client: &HddtClient,
    token: &str,
    key: &XmlInvoiceKey,
    portal_id: &str,
) -> Result<SavedXml, String> {
    download_one(pool, dir, client, token, key, portal_id, "manual").await
}

// ─── Tự động (chạy cùng lúc quét / nhập) ───

/// Một hóa đơn cần có XML (đọc từ 2 bảng: cache + đã nhập kho).
#[derive(sqlx::FromRow)]
struct PendingXml {
    portal_id: String,
    direction: String,
    kind: String,
    nbmst: String,
    khmshdon: Option<i64>,
    khhdon: String,
    shdon: String,
}

fn to_key(row: &PendingXml) -> Option<XmlInvoiceKey> {
    let direction = InvoiceDirection::parse(&row.direction).ok()?;
    // Không lưu loại hóa đơn → suy từ ký hiệu (chỉ hóa đơn bán ra mới rỗng;
    // hóa đơn mua lấy `portal_kind` từ cổng). Nhầm loại là cổng trả 404 nên
    // thà đoán đúng theo chữ `CG` còn hơn mặc định cả về `regular`.
    let kind = if row.kind.trim().is_empty() {
        InvoiceKind::from_symbol(&row.khhdon)
    } else {
        InvoiceKind::parse(&row.kind).ok()?
    };
    let khmshdon = row.khmshdon.unwrap_or(0);
    // Thiếu mã mẫu số thì endpoint export-xml vẫn nhận được (khác endpoint chi
    // tiết bắt buộc số) — giữ 0, cổng tự khớp theo 3 tham số còn lại.
    if row.nbmst.trim().is_empty() || row.khhdon.trim().is_empty() {
        return None;
    }
    Some(XmlInvoiceKey {
        direction,
        kind,
        nbmst: row.nbmst.clone(),
        khmshdon,
        khhdon: row.khhdon.clone(),
        shdon: row.shdon.clone(),
    })
}

/// Hóa đơn chưa có file XML — quét bảng cache + bảng hóa đơn đã nhập kho.
async fn pending_rows(pool: &SqlitePool) -> Result<Vec<PendingXml>, String> {
    let rows: Vec<PendingXml> = sqlx::query_as(
        r#"SELECT c.portal_id AS portal_id, 'purchase' AS direction,
                  c.portal_kind AS kind, c.nbmst AS nbmst, c.khmshdon AS khmshdon,
                  c.khhdon AS khhdon, c.shdon AS shdon
             FROM hddt_purchase_invoice c
            WHERE NOT EXISTS (
                    SELECT 1 FROM hddt_invoice_xml x
                     WHERE x.direction = 'purchase' AND x.kind = c.portal_kind
                       AND x.nbmst = c.nbmst
                       AND x.khmshdon = COALESCE(c.khmshdon, 0)
                       AND x.khhdon = c.khhdon AND x.shdon = c.shdon)
           UNION ALL
           SELECT h.portal_id, 'purchase', h.portal_kind, h.nbmst, h.khmshdon,
                  h.khhdon, h.shdon
             FROM hddt_imported_invoice h
            WHERE NOT EXISTS (
                    SELECT 1 FROM hddt_invoice_xml x
                     WHERE x.direction = 'purchase' AND x.kind = h.portal_kind
                       AND x.nbmst = h.nbmst
                       AND x.khmshdon = COALESCE(h.khmshdon, 0)
                       AND x.khhdon = h.khhdon AND x.shdon = h.shdon)"#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Danh sách hóa đơn thiếu XML lỗi: {e}"))?;
    Ok(rows)
}

/// Tự động tải XML cho mọi hóa đơn mua chưa có file — chạy sau "Quét cổng".
///
/// Lỗi từng hóa đơn không chặn luồng (cổng có HĐ chưa có hồ sơ gốc, hết phiên…)
/// mà cộng vào [`FillOutcome`] để UI báo.
pub(crate) async fn fill_missing_after_scan(
    pool: &SqlitePool,
    dir: &Path,
    client: &HddtClient,
    token: &str,
) -> FillOutcome {
    fill_missing(pool, dir, client, token).await
}

/// Tự động tải XML cho hóa đơn vừa nhập kho — chạy **sau** khi phiếu đã tạo
/// để backfill hóa đơn quét từ trước khi app có tính năng này.
pub(crate) async fn fill_missing_before_import(
    pool: &SqlitePool,
    dir: &Path,
    client: &HddtClient,
    token: &str,
    ids: &[i64],
) -> FillOutcome {
    // Ids rỗng = nhập tất cả → không lọc (đúng ngữ nghĩa của lệnh import).
    if ids.is_empty() {
        return fill_missing(pool, dir, client, token).await;
    }
    let mut out = FillOutcome::default();
    for id in ids {
        let row: Option<PendingXml> = sqlx::query_as(
            r#"SELECT portal_id, 'purchase' AS direction, portal_kind AS kind,
                      nbmst, khmshdon, khhdon, shdon
                 FROM hddt_purchase_invoice WHERE id = ?"#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await
        .unwrap_or(None);
        let Some(row) = row else {
            out.skipped += 1;
            continue;
        };
        run_one(pool, dir, client, token, &row, &mut out).await;
    }
    out
}

async fn fill_missing(
    pool: &SqlitePool,
    dir: &Path,
    client: &HddtClient,
    token: &str,
) -> FillOutcome {
    let mut out = FillOutcome::default();
    let rows = match pending_rows(pool).await {
        Ok(r) => r,
        Err(e) => {
            out.fail(e);
            return out;
        }
    };
    for row in rows {
        run_one(pool, dir, client, token, &row, &mut out).await;
    }
    out
}

async fn run_one(
    pool: &SqlitePool,
    dir: &Path,
    client: &HddtClient,
    token: &str,
    row: &PendingXml,
    out: &mut FillOutcome,
) {
    let Some(key) = to_key(row) else {
        // Thiếu MST/ký hiệu → không gọi được cổng, bỏ qua âm thầm (dòng này
        // không đủ dữ kiện xuất XML bất kỳ cách nào).
        out.skipped += 1;
        return;
    };
    match download_one(pool, dir, client, token, &key, &row.portal_id, "auto").await {
        Ok(r) if r.already => out.skipped += 1,
        Ok(_) => out.saved += 1,
        Err(e) => out.fail(e),
    }
}

// ─── Tra cứu trạng thái ───

/// Toàn bộ file đã lưu — frontend tra "hóa đơn nào đã có XML".
pub(crate) async fn list_saved(pool: &SqlitePool) -> Result<Vec<SavedXmlRow>, String> {
    sqlx::query_as::<_, SavedXmlRow>(
        r#"SELECT id, portal_id, direction, kind, nbmst, khmshdon, khhdon, shdon,
                  file_name, byte_size, source, saved_at
             FROM hddt_invoice_xml ORDER BY saved_at DESC, id DESC"#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Danh sách file XML lỗi: {e}"))
}

// ─── Gói XML vào báo cáo kỳ thuế ───

/// Toàn bộ XML đã lưu, gom sẵn vào **thư mục trong ZIP theo chiều** hóa đơn.
///
/// Trả `(đường dẫn trong ZIP, nội dung)`; blob thiếu (dòng lưu trước khi thêm
/// cột `xml_body`) lây từ bản sao file — file cũng mất thì đếm vào số trả về
/// thứ hai để báo cáo ghi vào `CHU-THICH.txt`.
///
/// Không lọc theo kỳ: file là bản gốc tải từ cổng lúc bấm "Tải XML", giữ nguyên
/// toàn bộ để hồ sơ nộp/luu trữ không bị hụt file.
pub(crate) async fn xml_report_entries(
    pool: &SqlitePool,
    dir: &Path,
) -> Result<(Vec<(String, Vec<u8>)>, usize), String> {
    let rows: Vec<(i64, String, String, Option<Vec<u8>>)> = sqlx::query_as(
        "SELECT id, direction, file_name, xml_body FROM hddt_invoice_xml ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Đọc danh sách XML lỗi: {e}"))?;

    let mut out = Vec::new();
    let mut lost = 0usize;
    for (id, direction, name, body) in rows {
        let body = match body.filter(|b| !b.is_empty()) {
            Some(b) => Some(b),
            None => body_from_disk(pool, dir, id, &name).await?,
        };
        let Some(body) = body else {
            lost += 1;
            continue;
        };
        let folder = if direction == "sold" {
            "hoa-don/xml/ban-ra"
        } else {
            "hoa-don/xml/mua-vao"
        };
        out.push((format!("{folder}/{name}"), body));
    }
    Ok((out, lost))
}

// ─── Nộp / gửi ZIP ───

/// Gộp các hóa đơn đã lưu thành **1 file ZIP** để nộp/gửi cho kế toán hoặc
/// lưu trữ theo quy định.
///
/// * `ids` là danh sách `hddt_invoice_xml.id` frontend lấy từ [`list_saved`];
///   rỗng = lấy **tất cả**.
/// * Mỗi entry dùng đúng `file_name` đã đặt (đã chứa chiều/loại/ký hiệu/số →
///   không hóa đơn nào trùng tên trong ZIP).
/// * Nội dung đọc từ `xml_body`; dòng cũ chưa có blob được nạp lại từ bản sao
///   file nên không phải tải lại từ cổng.
pub(crate) async fn export_zip(
    pool: &SqlitePool,
    dir: &Path,
    ids: &[i64],
) -> Result<Vec<u8>, String> {
    let targets: Vec<(i64, String)> = if ids.is_empty() {
        sqlx::query_as("SELECT id, file_name FROM hddt_invoice_xml ORDER BY saved_at DESC, id DESC")
            .fetch_all(pool)
            .await
            .map_err(|e| format!("Đọc danh sách XML lỗi: {e}"))?
    } else {
        let mut out: Vec<(i64, String)> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for id in ids {
            // Id lặp lại → 2 entry cùng tên trong ZIP là lỗi, bỏ qua bản sao.
            if !seen.insert(*id) {
                continue;
            }
            let row: Option<(i64, String)> =
                sqlx::query_as("SELECT id, file_name FROM hddt_invoice_xml WHERE id = ?")
                    .bind(id)
                    .fetch_optional(pool)
                    .await
                    .map_err(|e| format!("Đọc bản ghi XML lỗi: {e}"))?;
            if let Some(r) = row {
                out.push(r);
            }
        }
        out
    };

    if targets.is_empty() {
        return Err("Chưa có hóa đơn nào được lưu XML nên chưa gộp được ZIP.".to_string());
    }

    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let mut packed = 0usize;
    let mut lost = 0usize;
    for (id, name) in &targets {
        let row: Option<(String, Option<Vec<u8>>)> =
            sqlx::query_as("SELECT file_name, xml_body FROM hddt_invoice_xml WHERE id = ?")
                .bind(id)
                .fetch_optional(pool)
                .await
                .map_err(|e| format!("Đọc nội dung XML lỗi: {e}"))?;
        let Some((_, body)) = row else { continue };
        // Blob thiếu (dòng lưu trước khi có cột) → lây từ bản sao file.
        let body = match body.filter(|b| !b.is_empty()) {
            Some(b) => Some(b),
            None => body_from_disk(pool, dir, *id, name).await?,
        };
        let Some(body) = body else {
            lost += 1;
            continue;
        };
        w.start_file(name.as_str(), opts)
            .map_err(|e| format!("Tạo mục {name} trong ZIP lỗi: {e}"))?;
        std::io::Write::write_all(&mut w, &body)
            .map_err(|e| format!("Ghi {name} vào ZIP lỗi: {e}"))?;
        packed += 1;
    }
    if packed == 0 {
        return Err(format!(
            "Không đọc được nội dung XML nào để gộp ZIP ({lost} hóa đơn mất cả blob lẫn file)."
        ));
    }
    w.finish()
        .map_err(|e| format!("Hoàn tất ZIP lỗi: {e}"))
        .map(|c| c.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_pool;
    use std::io::Write;

    /// Dựng ZIP trong bộ nhớ đúng dạng cổng trả về (entry `invoice.xml` + HTML).
    fn sample_zip(with_xml: bool) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        if with_xml {
            w.start_file("invoice.xml", opts).unwrap();
            w.write_all(b"<HDon><TTChung><SHDon>0001</SHDon></TTChung></HDon>")
                .unwrap();
        }
        w.start_file("invoice.html", opts).unwrap();
        w.write_all(b"<html></html>").unwrap();
        w.finish().unwrap().into_inner()
    }

    fn key() -> XmlInvoiceKey {
        XmlInvoiceKey {
            direction: InvoiceDirection::Purchase,
            kind: InvoiceKind::Regular,
            nbmst: "0106409522".into(),
            khmshdon: 1,
            khhdon: "C26THN".into(),
            shdon: "00002397".into(),
        }
    }

    #[test]
    fn extract_takes_invoice_xml_and_skips_html() {
        let xml = extract_invoice_xml(&sample_zip(true)).expect("trích XML");
        assert!(String::from_utf8_lossy(&xml).contains("<HDon>"));
    }

    #[test]
    fn extract_errors_when_zip_has_no_xml() {
        let err = extract_invoice_xml(&sample_zip(false)).expect_err("không có XML");
        assert!(err.contains("không chứa file XML"), "lỗi = {err}");
    }

    #[test]
    fn extract_rejects_non_zip() {
        assert!(extract_invoice_xml(b"{\"message\":\"x\"}").is_err());
    }

    #[tokio::test]
    async fn save_writes_file_and_row_then_is_idempotent() {
        let pool = test_pool().await;
        let dir = std::env::temp_dir().join(format!("hkd-xml-{}", uuid::Uuid::new_v4()));
        let k = key();

        let saved = save_from_zip(&pool, &dir, &k, &sample_zip(true), "pid-1", "manual")
            .await
            .expect("lưu XML");
        assert!(!saved.already);
        assert!(saved.byte_size > 0);
        let path = dir.join(&saved.file_name);
        let body = std::fs::read_to_string(&path).expect("đọc file");
        assert!(body.contains("<HDon>"), "file chứa XML thật");

        let rows = list_saved(&pool).await.expect("liệt kê");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].direction, "purchase");
        assert_eq!(rows[0].kind, "regular");
        assert_eq!(rows[0].source, "manual");

        // Lưu lại cùng khóa → không nhân đôi bản ghi.
        save_from_zip(&pool, &dir, &k, &sample_zip(true), "pid-1", "manual")
            .await
            .expect("lưu lần 2");
        let rows = list_saved(&pool).await.expect("liệt kê");
        assert_eq!(rows.len(), 1, "không được trùng bản ghi");
        assert!(rows[0].id > 0, "phải trả id để frontend chọn ra ZIP");

        // Nội dung nằm trong CSDL chứ không chỉ trong file.
        let stored: Option<Vec<u8>> = sqlx::query_scalar("SELECT xml_body FROM hddt_invoice_xml")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            stored.as_deref(),
            Some(extract_invoice_xml(&sample_zip(true)).unwrap().as_slice()),
            "xml_body phải đúng nội dung file"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn existing_keeps_row_when_file_deleted_because_blob_is_authoritative() {
        // File `.xml` chỉ là bản sao để mở tay — mất file không được coi là
        // "chưa lưu" (nếu không, mỗi lần quét lại tốn 1 request cổng vô ích).
        let pool = test_pool().await;
        let dir = std::env::temp_dir().join(format!("hkd-xml-{}", uuid::Uuid::new_v4()));
        let k = key();
        let saved = save_from_zip(&pool, &dir, &k, &sample_zip(true), "pid-1", "auto")
            .await
            .expect("lưu");
        std::fs::remove_file(dir.join(&saved.file_name)).unwrap();

        let found = existing(&pool, &dir, &k)
            .await
            .unwrap()
            .expect("blob còn thì vẫn tính là đã lưu");
        assert_eq!(found.0, saved.file_name);
        assert_eq!(found.1, saved.byte_size, "cỡ lấy từ CSDL, không từ file");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn row_saved_before_blob_column_is_backfilled_from_its_file() {
        // Dòng cũ có file nhưng `xml_body` NULL (lưu trước migration thêm cột):
        // đọc lúc sau phải tự nạp blob từ file, không gọi cổng.
        let pool = test_pool().await;
        let dir = std::env::temp_dir().join(format!("hkd-xml-{}", uuid::Uuid::new_v4()));
        let k = key();
        let saved = save_from_zip(&pool, &dir, &k, &sample_zip(true), "pid-1", "auto")
            .await
            .expect("lưu");
        sqlx::query("UPDATE hddt_invoice_xml SET xml_body = NULL")
            .execute(&pool)
            .await
            .unwrap();
        assert!(
            existing(&pool, &dir, &k).await.unwrap().is_some(),
            "file còn thì vẫn là đã lưu"
        );
        let stored: Option<Vec<u8>> = sqlx::query_scalar("SELECT xml_body FROM hddt_invoice_xml")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(
            stored.as_deref().is_some_and(|b| !b.is_empty()),
            "blob đã được nạp lại từ file"
        );

        // Giờ file có mất đi nữa cũng không cần tải lại cổng.
        std::fs::remove_file(dir.join(&saved.file_name)).unwrap();
        assert!(
            existing(&pool, &dir, &k).await.unwrap().is_some(),
            "blob đã có trong CSDL"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn file_name_is_deterministic_and_safe() {
        let mut k = key();
        let a = file_name_of(&k);
        assert_eq!(a, "purchase_regular_0106409522_C26THN_00002397.xml");
        assert_eq!(a, file_name_of(&k), "cùng khóa → cùng tên file");
        k.khhdon = "C26/THN".into();
        let b = file_name_of(&k);
        assert!(!b.contains('/'), "ký tự lạ phải bị thay: {b}");
        assert_ne!(a, b, "khác ký hiệu → khác tên file");
    }

    // ─── Luồng thật với mock portal ───

    /// Dựng 1 dòng trong bảng cache (dùng query runtime để khỏi thêm macro sqlx).
    async fn seed_cache(pool: &SqlitePool, portal_id: &str, shdon: &str) {
        sqlx::query(
            "INSERT INTO hddt_purchase_invoice
                (portal_id, portal_kind, tdlap, posting_date, nbmst, nbten, nmmst,
                 khmshdon, khhdon, shdon, hthdon, tchat, tgtcthue, tgtthue,
                 tgtttbso, status, skip_reason, line_count, raw_json, detail_json)
             VALUES (?, 'regular', '2026-10-01T17:00:00Z', '2026-10-02', '0100000000',
                     'CÔNG TY MOCK', '001170019085', 1, 'C26MOCK', ?, 1, 1,
                     100000, 8000, 108000, 'pending', '', 1, '{}', '{}')",
        )
        .bind(portal_id)
        .bind(shdon)
        .execute(pool)
        .await
        .expect("seed cache");
    }

    #[tokio::test]
    async fn download_one_fetches_and_saves_then_skips_second_time() {
        let pool = test_pool().await;
        let portal = crate::hddt::mock_portal::MockPortal::start().await;
        let client = crate::hddt::HddtClient::for_test(&portal.base);
        let dir = std::env::temp_dir().join(format!("hkd-xml-{}", uuid::Uuid::new_v4()));
        let k = key();

        let first = download_one(&pool, &dir, &client, "mock-token", &k, "pid-1", "manual")
            .await
            .expect("lần 1 tải + lưu");
        assert!(!first.already, "lần đầu phải tải mới");
        assert!(dir.join(&first.file_name).is_file(), "file nằm trên đĩa");

        // Lần 2: file đã có → không gọi cổng nữa (tiết kiệm request, tránh 429).
        let calls = portal.count("/invoices/export-xml");
        let second = download_one(&pool, &dir, &client, "mock-token", &k, "pid-1", "manual")
            .await
            .expect("lần 2 trả đã lưu");
        assert!(second.already, "đã lưu rồi");
        assert_eq!(portal.count("/invoices/export-xml"), calls, "không gọi lại");

        assert_eq!(
            list_saved(&pool).await.unwrap().len(),
            1,
            "1 hóa đơn = 1 bản ghi"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn fill_missing_after_scan_saves_every_cached_invoice_once() {
        let pool = test_pool().await;
        let portal = crate::hddt::mock_portal::MockPortal::start().await;
        let client = crate::hddt::HddtClient::for_test(&portal.base);
        let dir = std::env::temp_dir().join(format!("hkd-xml-{}", uuid::Uuid::new_v4()));

        seed_cache(&pool, "pid-1", "0001").await;
        seed_cache(&pool, "pid-2", "0002").await;

        let out = fill_missing_after_scan(&pool, &dir, &client, "mock-token").await;
        assert_eq!(out.saved, 2, "cả 2 hóa đơn cache phải có file: {out:?}");
        assert_eq!(out.failed, 0, "không được lỗi: {out:?}");

        // Chạy lại → tất cả đã có file nên không còn dòng nào cần tải.
        let again = fill_missing_after_scan(&pool, &dir, &client, "mock-token").await;
        assert_eq!(again.saved, 0, "không tải lại: {again:?}");
        assert_eq!(again.failed, 0, "{again:?}");
        assert_eq!(portal.count("/invoices/export-xml"), 2, "đúng 2 request");

        // File thật sự nằm trong thư mục hồ sơ, nội dung là XML của cổng.
        let rows = list_saved(&pool).await.unwrap();
        assert_eq!(rows.len(), 2);
        let body = std::fs::read_to_string(dir.join(&rows[0].file_name)).expect("đọc file");
        assert!(body.contains("<HDon>"), "phải là XML hóa đơn");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn fill_missing_survives_a_row_without_business_tin() {
        // Dòng thiếu MST/ký hiệu (cache cũ) không được làm hỏng cả lượt tải.
        let pool = test_pool().await;
        let portal = crate::hddt::mock_portal::MockPortal::start().await;
        let client = crate::hddt::HddtClient::for_test(&portal.base);
        let dir = std::env::temp_dir().join(format!("hkd-xml-{}", uuid::Uuid::new_v4()));

        sqlx::query(
            "INSERT INTO hddt_purchase_invoice
                (portal_id, portal_kind, tdlap, posting_date, nbmst, nbten, nmmst,
                 khmshdon, khhdon, shdon, hthdon, tchat, tgtcthue, tgtthue,
                 tgtttbso, status, skip_reason, line_count, raw_json, detail_json)
             VALUES ('pid-bad', 'regular', '2026-10-01T17:00:00Z', '2026-10-02', '',
                     '', '', 1, '', '', 1, 1, 0, 0, 0, 'pending', '', 1, '{}', '{}')",
        )
        .execute(&pool)
        .await
        .expect("seed dòng thiếu dữ kiện");
        seed_cache(&pool, "pid-ok", "0007").await;

        let out = fill_missing_after_scan(&pool, &dir, &client, "mock-token").await;
        assert_eq!(out.saved, 1, "dòng đủ dữ kiện vẫn tải được: {out:?}");
        assert_eq!(out.skipped, 1, "dòng thiếu MST/ký hiệu bị bỏ qua: {out:?}");
        assert_eq!(out.failed, 0, "không tính là lỗi: {out:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ─── Gộp ZIP nộp/gửi ───

    /// Đọc ZIP thành danh sách `(tên entry, nội dung)`.
    fn zip_entries(bytes: &[u8]) -> Vec<(String, String)> {
        let mut a = zip::ZipArchive::new(std::io::Cursor::new(bytes.to_vec())).unwrap();
        (0..a.len())
            .map(|i| {
                let mut e = a.by_index(i).unwrap();
                let name = e.name().to_string();
                let mut s = String::new();
                std::io::Read::read_to_string(&mut e, &mut s).unwrap();
                (name, s)
            })
            .collect()
    }

    /// Hóa đơn thứ 2 — cùng mẫu số/ký hiệu nhưng khác số.
    fn key2() -> XmlInvoiceKey {
        XmlInvoiceKey {
            shdon: "00002398".into(),
            ..key()
        }
    }

    #[tokio::test]
    async fn export_zip_packs_only_the_selected_invoices() {
        let pool = test_pool().await;
        let dir = std::env::temp_dir().join(format!("hkd-xml-{}", uuid::Uuid::new_v4()));
        let (k1, k2) = (key(), key2());
        save_from_zip(&pool, &dir, &k1, &sample_zip(true), "pid-1", "auto")
            .await
            .unwrap();
        save_from_zip(&pool, &dir, &k2, &sample_zip(true), "pid-2", "auto")
            .await
            .unwrap();

        let rows = list_saved(&pool).await.unwrap();
        assert_eq!(rows.len(), 2);

        // Chọn 1 hóa đơn → ZIP có đúng 1 file, tên file đúng như đã đặt.
        let one = export_zip(&pool, &dir, &[rows[0].id]).await.expect("gộp 1");
        let entries = zip_entries(&one);
        assert_eq!(entries.len(), 1, "chỉ gộp hóa đơn được chọn");
        assert_eq!(entries[0].0, rows[0].file_name);
        assert!(entries[0].1.contains("<HDon>"), "nội dung là XML thật");

        // Không chọn gì → gộp tất cả (dùng khi nộp theo kỳ).
        let all = export_zip(&pool, &dir, &[]).await.expect("gộp tất cả");
        assert_eq!(zip_entries(&all).len(), 2, "2 hóa đơn = 2 file XML");

        // Id lặp lại không được sinh 2 entry cùng tên (ZIP sẽ lỗi).
        let dup = export_zip(&pool, &dir, &[rows[0].id, rows[0].id])
            .await
            .expect("gộp id trùng");
        assert_eq!(zip_entries(&dup).len(), 1, "bỏ bản sao id");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn export_zip_reads_a_legacy_row_whose_blob_is_still_empty() {
        // Dòng lưu trước khi có cột `xml_body`: file vẫn còn → ZIP vẫn gộp được
        // và blob được nạp lại từ file trong lúc gộp.
        let pool = test_pool().await;
        let dir = std::env::temp_dir().join(format!("hkd-xml-{}", uuid::Uuid::new_v4()));
        save_from_zip(&pool, &dir, &key(), &sample_zip(true), "pid-1", "auto")
            .await
            .unwrap();
        sqlx::query("UPDATE hddt_invoice_xml SET xml_body = NULL")
            .execute(&pool)
            .await
            .unwrap();

        let rows = list_saved(&pool).await.unwrap();
        let zip = export_zip(&pool, &dir, &[rows[0].id]).await.expect("gộp");
        assert_eq!(zip_entries(&zip).len(), 1, "lây từ file được");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn export_zip_explains_when_there_is_nothing_to_pack() {
        // Không có gì đã lưu → báo lỗi rõ ràng thay vì trả ZIP rỗng.
        let pool = test_pool().await;
        let dir = std::env::temp_dir().join(format!("hkd-xml-{}", uuid::Uuid::new_v4()));
        let err = export_zip(&pool, &dir, &[]).await.expect_err("chưa có gì");
        assert!(err.contains("Chưa có hóa đơn"), "lỗi = {err}");
    }

    // ─── Gói XML vào báo cáo kỳ thuế ───

    #[tokio::test]
    async fn xml_report_entries_groups_by_direction_and_backfills_blob() {
        // Báo cáo kỳ thuế cần XML gom sẵn theo THƯ MỤC (mua vào / bán ra) chứ
        // không phải ZIP riêng như nút "Tải XML".
        let pool = test_pool().await;
        let dir = std::env::temp_dir().join(format!("hkd-xml-{}", uuid::Uuid::new_v4()));
        save_from_zip(&pool, &dir, &key(), &sample_zip(true), "pid-1", "auto")
            .await
            .unwrap();
        let sold = XmlInvoiceKey {
            direction: InvoiceDirection::Sold,
            shdon: "00002398".into(),
            ..key()
        };
        save_from_zip(&pool, &dir, &sold, &sample_zip(true), "pid-2", "auto")
            .await
            .unwrap();
        // Dòng lưu trước khi thêm cột `xml_body` → nạp lại từ file.
        sqlx::query("UPDATE hddt_invoice_xml SET xml_body = NULL WHERE direction = 'sold'")
            .execute(&pool)
            .await
            .unwrap();

        let (entries, lost) = xml_report_entries(&pool, &dir).await.expect("gói XML");
        assert_eq!(lost, 0, "file còn thì không mất gì");
        assert_eq!(entries.len(), 2, "đủ cả hai chiều");
        assert!(
            entries
                .iter()
                .any(|(p, _)| p.starts_with("hoa-don/xml/mua-vao/")),
            "XML mua vào vào thư mục riêng: {entries:?}"
        );
        let (path, body) = entries
            .iter()
            .find(|(p, _)| p.starts_with("hoa-don/xml/ban-ra/"))
            .expect("có XML bán ra");
        assert!(path.ends_with(".xml"), "giữ nguyên tên file: {path}");
        assert!(
            String::from_utf8_lossy(body).contains("<HDon>"),
            "nội dung lây từ file được"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn xml_report_entries_counts_rows_that_lost_blob_and_file() {
        let pool = test_pool().await;
        let dir = std::env::temp_dir().join(format!("hkd-xml-{}", uuid::Uuid::new_v4()));
        save_from_zip(&pool, &dir, &key(), &sample_zip(true), "pid-1", "auto")
            .await
            .unwrap();
        sqlx::query("UPDATE hddt_invoice_xml SET xml_body = NULL, file_name = 'khong-con.xml'")
            .execute(&pool)
            .await
            .unwrap();

        let (entries, lost) = xml_report_entries(&pool, &dir).await.expect("gói XML");
        assert!(entries.is_empty(), "không có gì để gộp");
        assert_eq!(lost, 1, "báo đúng số file mất để ghi vào CHU-THICH.txt");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
