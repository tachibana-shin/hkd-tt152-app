// ─── Xuất báo cáo kỳ thuế — 1 file ZIP gộp tờ khai, sổ, hóa đơn và sao lưu ───
//
// Sau khi kê khai theo kỳ, người dùng cần một hồ sơ gộp đủ thứ để nộp hoặc lưu
// trữ: tờ khai, các sổ kế toán theo năm, PDF + XML hóa đơn mua vào và bán ra,
// cùng bản sao lưu cơ sở dữ liệu.
//
// Chia đôi trách nhiệm: frontend dựng phần **Excel** (tờ khai siêu chuẩn, sổ
// kế toán) vì đã có sẵn `exceljs` và giữ đúng số liệu người dùng đang sửa trên
// màn; backend dựng phần **PDF** (render qua gói `invoice-pdf`), lấy XML đã lưu
// trong CSDL và chép CSDL — tất cả phần này frontend không với tới được.

use crate::helpers::{audit, require_role};
use crate::models::AppState;
use base64::Engine;
use std::collections::HashSet;
use tauri::{AppHandle, State};

/// File do **frontend** dựng sẵn, gửi sang để backend gộp chung vào ZIP.
#[derive(serde::Deserialize)]
pub(crate) struct ExtraFile {
    /// Đường dẫn trong ZIP, đã gồm thư mục — vd `to-khai/to-khai-….xlsx`.
    pub(crate) path: String,
    /// Nội dung file dạng base64.
    pub(crate) data: String,
}

/// Một hóa đơn chờ dựng PDF: đường dẫn trong ZIP + `detail_json` đã đọc sẵn.
struct PdfJob {
    path: String,
    detail_json: String,
}

/// Số liệu ghi vào `CHU-THICH.txt` (và chỗ gom lỗi khi dựng).
#[derive(Default)]
struct ReportStats {
    /// Số file frontend gửi: tờ khai và sổ kế toán đếm riêng cho `CHU-THICH.txt`.
    tax_entry: usize,
    books: usize,
    xml_purchase: usize,
    xml_sold: usize,
    pdf_purchase: usize,
    pdf_sold: usize,
    db: bool,
    /// Lỗi từng phần (render PDF thất bại, file XML mất…) — không bỏ cuộc vì
    /// một hóa đơn hỏng mà mất cả báo cáo.
    problems: Vec<String>,
}

/// Trả `path` lần đầu; trùng thì thêm `-2`, `-3`… để hai hóa đơn cùng số/ký
/// hiệu (khác mẫu số) không đè lên nhau trong ZIP.
fn unique_path(used: &mut HashSet<String>, path: String) -> String {
    if used.insert(path.clone()) {
        return path;
    }
    let (dir, name) = match path.rsplit_once('/') {
        Some((d, n)) => (format!("{d}/"), n.to_string()),
        None => (String::new(), path),
    };
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) => (s.to_string(), format!(".{e}")),
        None => (name, String::new()),
    };
    let mut i = 2;
    loop {
        let cand = format!("{dir}{stem}-{i}{ext}");
        if used.insert(cand.clone()) {
            return cand;
        }
        i += 1;
    }
}

/// PDF hóa đơn **mua vào** phát sinh trong kỳ — lấy từ cả cache chờ nhập và
/// bảng hóa đơn đã nhập kho (hóa đơn đã nhập bị xoá khỏi cache, sang bảng mới).
async fn purchase_pdf_jobs(
    pool: &sqlx::SqlitePool,
    from: &str,
    to: &str,
) -> Result<Vec<PdfJob>, String> {
    let cached = sqlx::query!(
        r#"SELECT portal_id, posting_date, khhdon, shdon, detail_json
             FROM hddt_purchase_invoice
            WHERE posting_date >= ? AND posting_date <= ?
              AND detail_json <> '' AND detail_error = ''"#,
        from,
        to,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Đọc hóa đơn mua vào (cache) lỗi: {e}"))?;
    let imported = sqlx::query!(
        r#"SELECT portal_id, posting_date, khhdon, shdon, detail_json
             FROM hddt_imported_invoice
            WHERE posting_date >= ? AND posting_date <= ? AND detail_json <> ''"#,
        from,
        to,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Đọc hóa đơn mua vào (đã nhập) lỗi: {e}"))?;

    let mut seen = HashSet::new();
    let mut jobs = Vec::new();
    // Hai lệnh `query!` sinh ra hai kiểu `Record` khác nhau → đưa về chung một
    // bộ tuple trước khi nối, nếu không `chain` không khớp kiểu.
    let cached: Vec<_> = cached
        .into_iter()
        .map(|r| {
            (
                r.portal_id,
                r.posting_date,
                r.khhdon,
                r.shdon,
                r.detail_json,
            )
        })
        .collect();
    let imported: Vec<_> = imported
        .into_iter()
        .map(|r| {
            (
                r.portal_id,
                r.posting_date,
                r.khhdon,
                r.shdon,
                r.detail_json,
            )
        })
        .collect();
    for (portal_id, posting_date, khhdon, shdon, detail_json) in cached.into_iter().chain(imported)
    {
        if !seen.insert(portal_id) {
            continue; // cùng hóa đơn xuất hiện ở hai bảng → dựng 1 lần
        }
        jobs.push(PdfJob {
            path: format!("hoa-don/pdf/mua-vao/{posting_date}_{khhdon}-{shdon}.pdf"),
            detail_json,
        });
    }
    Ok(jobs)
}

/// PDF hóa đơn **bán ra** phát sinh trong kỳ — dựng từ hóa đơn nội bộ của app.
async fn sold_pdf_jobs(
    pool: &sqlx::SqlitePool,
    from: &str,
    to: &str,
    problems: &mut Vec<String>,
) -> Result<Vec<PdfJob>, String> {
    let rows = sqlx::query!(
        "SELECT id, date, number FROM invoice WHERE date >= ? AND date <= ? ORDER BY date, id",
        from,
        to,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Đọc hóa đơn bán ra lỗi: {e}"))?;

    let mut jobs = Vec::new();
    for r in rows {
        // Lỗi dựng `detail_json` của MỘT hóa đơn không được làm hỏng cả báo cáo:
        // ghi vào danh sách lỗi rồi bỏ qua hóa đơn đó.
        match crate::commands::invoice::invoice_draft_detail_json(pool, r.id).await {
            Ok(detail) => jobs.push(PdfJob {
                path: format!("hoa-don/pdf/ban-ra/{}_{}.pdf", r.date, r.number),
                detail_json: detail,
            }),
            Err(e) => problems.push(format!(
                "hoa-don/pdf/ban-ra/{}_{}.pdf: {e}",
                r.date, r.number
            )),
        }
    }
    Ok(jobs)
}

/// Gộp báo cáo kỳ thuế thành **1 file ZIP** rồi trả về base64.
///
/// * `extra_files` — tờ khai / sổ kế toán do frontend dựng (base64);
/// * PDF hóa đơn **trong kỳ** (`from_date` → `to_date`);
/// * XML hóa đơn **đã lưu trọn vẹn** (không lọc kỳ — file được lưu lúc bấm
///   "Tải XML", ghi rõ trong `CHU-THICH.txt`);
/// * bản sao lưu CSDL (chép sau khi checkpoint WAL để không mất dữ liệu mới).
#[tauri::command]
pub(crate) async fn export_tax_report(
    state: State<'_, AppState>,
    app: AppHandle,
    from_date: String,
    to_date: String,
    extra_files: Vec<ExtraFile>,
) -> Result<String, String> {
    require_role(&state, &["admin", "ketoan"]).await?;
    let pool = state.pool.read().await;
    let xml_dir = crate::commands::hddt::xml_dir(&app)?;
    let mut stats = ReportStats::default();
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    let mut used: HashSet<String> = HashSet::new();

    // ── 1. Tờ khai + sổ kế toán dựng sẵn từ frontend ──
    for f in extra_files {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(f.data.trim())
            .map_err(|e| format!("File \"{}\" không phải base64 hợp lệ: {e}", f.path))?;
        let path = f.path;
        if path.starts_with("so-ke-toan/") {
            stats.books += 1;
        } else {
            stats.tax_entry += 1;
        }
        entries.push((unique_path(&mut used, path), bytes));
    }

    // ── 2. XML đã lưu — gom theo chiều hóa đơn ──
    let (xml_entries, xml_lost) = crate::hddt::xml_report_entries(&pool, &xml_dir).await?;
    for (path, bytes) in xml_entries {
        if path.starts_with("hoa-don/xml/ban-ra/") {
            stats.xml_sold += 1;
        } else {
            stats.xml_purchase += 1;
        }
        entries.push((unique_path(&mut used, path), bytes));
    }
    if xml_lost > 0 {
        stats.problems.push(format!(
            "{xml_lost} file XML không đọc được (cả blob lẫn file đều mất)"
        ));
    }

    // ── 3. PDF hóa đơn trong kỳ ──
    let purchase = purchase_pdf_jobs(&pool, &from_date, &to_date).await?;
    let sold = sold_pdf_jobs(&pool, &from_date, &to_date, &mut stats.problems).await?;
    for job in purchase.into_iter().chain(sold) {
        let path = unique_path(&mut used, job.path);
        // Render là CPU-bound (~200ms/hóa đơn) → tách worker, chạy tuần tự để
        // không chiếm sạch lõi CPU trên máy người dùng.
        let detail = job.detail_json;
        let outcome =
            tauri::async_runtime::spawn_blocking(move || invoice_pdf::render_pdf(&detail)).await;
        match outcome {
            Ok(Ok(bytes)) => {
                let is_sold = path.starts_with("hoa-don/pdf/ban-ra/");
                entries.push((path, bytes));
                if is_sold {
                    stats.pdf_sold += 1;
                } else {
                    stats.pdf_purchase += 1;
                }
            }
            Ok(Err(e)) => stats.problems.push(format!("{path}: {e}")),
            Err(e) => stats.problems.push(format!("{path}: {e}")),
        }
    }

    // ── 4. Sao lưu CSDL — checkpoint WAL trước khi chép, nếu không bản sao có
    //        thể thiếu dữ liệu mới nhất vẫn nằm trong file `-wal`. ──
    // PRAGMA không khai báo cột nên giữ runtime query (câu lệnh bảo trì DB).
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&*pool)
        .await
        .map_err(|e| format!("Chốt WAL trước khi sao lưu lỗi: {e}"))?;
    let db_path = crate::commands::profile::active_db_path(&app)?;
    match std::fs::read(&db_path) {
        Ok(bytes) => {
            entries.push((unique_path(&mut used, "sao-luu/hkd.db".to_string()), bytes));
            stats.db = true;
        }
        Err(e) => stats
            .problems
            .push(format!("Không chép được CSDL {}: {e}", db_path.display())),
    }

    // ── 5. Chú thích đặt đầu ZIP ──
    let year = from_date.get(0..4).unwrap_or("");
    entries.push((
        "CHU-THICH.txt".to_string(),
        manifest(&from_date, &to_date, year, &stats).into_bytes(),
    ));

    if stats.tax_entry == 0 && stats.books == 0 && stats.pdf_purchase == 0 && stats.pdf_sold == 0 {
        return Err(
            "Không lấy được dữ liệu nào để đóng gói báo cáo (thiếu tờ khai/sổ và không có hóa đơn)."
                .to_string(),
        );
    }

    let zip = pack_zip(&entries)?;
    drop(pool);
    audit(
        &state,
        "export_tax_report",
        "report",
        &format!("kỳ {from_date}–{to_date}, {n} mục", n = entries.len()),
    )
    .await;
    Ok(base64::engine::general_purpose::STANDARD.encode(zip))
}

/// Nén các mục đã gộp thành ZIP (deflate — `zip` crate đã bật sẵn).
fn pack_zip(entries: &[(String, Vec<u8>)]) -> Result<Vec<u8>, String> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (path, bytes) in entries {
        w.start_file(path.as_str(), opts)
            .map_err(|e| format!("Tạo mục {path} trong ZIP lỗi: {e}"))?;
        std::io::Write::write_all(&mut w, bytes)
            .map_err(|e| format!("Ghi {path} vào ZIP lỗi: {e}"))?;
    }
    w.finish()
        .map_err(|e| format!("Hoàn tất ZIP lỗi: {e}"))
        .map(|c| c.into_inner())
}

/// `CHU-THICH.txt` — hướng dẫn mở hồ sơ, nêu rõ có gì và vì sao PDF/XML chọn
/// kỳ khác nhau (XML lưu khi bấm "Tải XML" nên không lọc theo kỳ).
fn manifest(from: &str, to: &str, year: &str, s: &ReportStats) -> String {
    let mut out = String::new();
    out.push_str("BÁO CÁO KỲ THUẾ — HỘ KINH DOANH\n");
    out.push_str("================================\n");
    out.push_str(&format!("Kỳ kê khai    : {from} – {to}\n"));
    out.push_str(&format!("Năm sổ kế toán: {year}\n"));
    out.push_str(&format!(
        "Tạo lúc       : {}\n\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    ));
    out.push_str("Cấu trúc thư mục\n");
    out.push_str("-----------------\n");
    out.push_str("CHU-THICH.txt            Tệp này\n");
    out.push_str(&format!(
        "to-khai/                 Tờ khai kê khai doanh thu (Excel) — {} file\n",
        s.tax_entry
    ));
    out.push_str(&format!(
        "so-ke-toan/              Sổ kế toán theo năm (Excel) — {} file\n",
        s.books
    ));
    out.push_str(&format!(
        "hoa-don/pdf/mua-vao/     PDF hóa đơn mua vào trong kỳ — {} hóa đơn\n",
        s.pdf_purchase
    ));
    out.push_str(&format!(
        "hoa-don/pdf/ban-ra/      PDF hóa đơn bán ra trong kỳ — {} hóa đơn\n",
        s.pdf_sold
    ));
    out.push_str(&format!(
        "hoa-don/xml/mua-vao/     XML hóa đơn mua vào đã lưu — {} file\n",
        s.xml_purchase
    ));
    out.push_str(&format!(
        "hoa-don/xml/ban-ra/      XML hóa đơn bán ra đã lưu — {} file\n",
        s.xml_sold
    ));
    out.push_str(&format!(
        "sao-luu/hkd.db           Bản sao lưu CSDL — {}\n",
        if s.db {
            "có (mở lại bằng app: Cài đặt → Sao lưu)"
        } else {
            "KHÔNG có"
        }
    ));
    out.push_str("\nGhi chú\n");
    out.push_str("-------\n");
    out.push_str(
        "- PDF lấy hóa đơn phát sinh TRONG kỳ kê khai.\n\
         - XML lấy TOÀN BỘ file đã lưu (lưu lúc bấm \"Tải XML\"), không lọc theo kỳ:\n\
          \x20 file là bản gốc từ cổng thuế nên giữ nguyên.\n\
         - Sổ kế toán lấy theo NĂM của kỳ (dù kỳ chưa tròn năm vẫn xuất đủ dữ liệu\n\
          \x20 hiện có — Luật Kế toán bắt sổ ghi liên tục cả năm).\n",
    );
    if s.problems.is_empty() {
        out.push_str("- Không có lỗi nào khi dựng báo cáo.\n");
    } else {
        out.push_str(&format!("- {} mục bị bỏ qua do lỗi:\n", s.problems.len()));
        for p in &s.problems {
            out.push_str(&format!("  * {p}\n"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Đọc ZIP vừa nén thành `(tên entry, nội dung)`.
    fn zip_entries(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
        let mut a = zip::ZipArchive::new(std::io::Cursor::new(bytes.to_vec())).unwrap();
        (0..a.len())
            .map(|i| {
                let mut e = a.by_index(i).unwrap();
                let name = e.name().to_string();
                let mut buf = Vec::new();
                std::io::Read::read_to_end(&mut e, &mut buf).unwrap();
                (name, buf)
            })
            .collect()
    }

    #[test]
    fn unique_path_keeps_first_then_suffixes_every_duplicate() {
        let mut used = HashSet::new();
        assert_eq!(unique_path(&mut used, "a/b.pdf".into()), "a/b.pdf");
        assert_eq!(unique_path(&mut used, "a/b.pdf".into()), "a/b-2.pdf");
        assert_eq!(unique_path(&mut used, "a/b.pdf".into()), "a/b-3.pdf");
        // Tên không có đuôi vẫn phân biệt được (trùng là đè mất hóa đơn thật).
        assert_eq!(unique_path(&mut used, "noext".into()), "noext");
        assert_eq!(unique_path(&mut used, "noext".into()), "noext-2");
        // Đường dẫn khác thư mục thì không phải xung đột.
        assert_eq!(unique_path(&mut used, "c/b.pdf".into()), "c/b.pdf");
    }

    #[test]
    fn pack_zip_keeps_every_entry_and_its_bytes() {
        let entries = vec![
            ("CHU-THICH.txt".to_string(), b"chu thich".to_vec()),
            (
                "sao-luu/hkd.db".to_string(),
                vec![0x53, 0x51, 0x4c, 0x69, 0x74, 0x65],
            ),
        ];
        let zip = pack_zip(&entries).expect("nén");
        let out = zip_entries(&zip);
        let names: Vec<&str> = out.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            vec!["CHU-THICH.txt", "sao-luu/hkd.db"],
            "giữ nguyên cả tên lẫn thứ tự"
        );
        assert_eq!(out[0].1, b"chu thich");
        assert_eq!(out[1].1, entries[1].1, "byte không bị hỏng qua nén");
    }

    #[test]
    fn manifest_tells_the_period_what_each_folder_holds_and_that_xml_keeps_everything() {
        let s = ReportStats {
            tax_entry: 1,
            books: 7,
            xml_purchase: 3,
            xml_sold: 2,
            pdf_purchase: 4,
            pdf_sold: 5,
            db: true,
            problems: vec!["hoa-don/pdf/mua-vao/x.pdf: lỗi render".into()],
        };
        let m = manifest("2026-01-01", "2026-03-31", "2026", &s);

        assert!(m.contains("2026-01-01 – 2026-03-31"), "nêu kỳ kê khai: {m}");
        assert!(m.contains("2026"), "nêu năm sổ: {m}");
        assert!(
            m.contains("to-khai/") && m.contains("— 1 file"),
            "tờ khai: {m}"
        );
        assert!(
            m.contains("so-ke-toan/") && m.contains("— 7 file"),
            "đủ 7 sổ: {m}"
        );
        assert!(
            m.contains("PDF hóa đơn mua vào trong kỳ — 4"),
            "PDF mua: {m}"
        );
        assert!(
            m.contains("PDF hóa đơn bán ra trong kỳ — 5"),
            "PDF bán: {m}"
        );
        assert!(m.contains("XML hóa đơn mua vào đã lưu — 3"), "XML mua: {m}");
        assert!(m.contains("XML hóa đơn bán ra đã lưu — 2"), "XML bán: {m}");

        // Hai nhóm chọn kỳ KHÁC nhau phải được nói rõ, không để người dùng hiểu lầm.
        assert!(
            m.contains("PDF lấy hóa đơn phát sinh TRONG kỳ"),
            "PDF lọc kỳ: {m}"
        );
        assert!(m.contains("XML lấy TOÀN BỘ"), "XML không lọc kỳ: {m}");
        assert!(m.contains("kỳ chưa tròn năm"), "sổ lấy theo năm: {m}");

        // Lỗi từng mục phải liệt kê, không nuốt.
        assert!(m.contains("mục bị bỏ qua do lỗi"), "kèm số lỗi: {m}");
        assert!(m.contains("lỗi render"), "liệt kê từng lỗi: {m}");
    }

    #[test]
    fn manifest_says_so_when_nothing_failed() {
        let m = manifest("2026-01-01", "2026-01-31", "2026", &ReportStats::default());
        assert!(m.contains("Không có lỗi nào"), "báo sạch lỗi: {m}");
        assert!(m.contains("KHÔNG có"), "báo rõ khi thiếu bản sao lưu: {m}");
    }
}
