// ─── Live test against the real HDDT portal ───
// Reads credentials from `../../info.txt` (gitignored, never committed).
// Performs a full password-change round-trip and RESTORES the original
// password at the end so the account is left unchanged.
//
// Run with: cd src-tauri && cargo test live_hddt -- --nocapture

#[cfg(test)]
mod live_tests {
    use crate::hddt::{generate_password, GlyphTemplateSolver, HddtClient};
    use std::fs;

    #[tokio::test]
    #[ignore = "cần ../info.txt + portal live (round-trip đổi mật khẩu)"]
    async fn change_password_roundtrip() {
        // ── 0. Load creds from info.txt (gitignored) ──
        if !fs::metadata("../info.txt").is_ok() {
            eprintln!("⚠️  skipped — ../info.txt không có");
            return;
        }
        let txt = fs::read_to_string("../info.txt").expect("không đọc được info.txt");
        let mut username = None;
        let mut password = None;
        for line in txt.lines() {
            if let Some((k, v)) = line.split_once('=') {
                match k.trim() {
                    "USERNAME" => username = Some(v.trim().to_string()),
                    "PASSWORD" => password = Some(v.trim().to_string()),
                    _ => {}
                }
            }
        }
        let username = username.expect("thiếu USERNAME");
        let old_password = password.expect("thiếu PASSWORD");

        // ── 1. Login với mật khẩu cũ (xác nhận mật khẩu còn sống) ──
        let client = HddtClient::new(&username, &old_password, None).expect("tạo client thất bại");
        let login = client
            .login_with_solver(&GlyphTemplateSolver, 3)
            .await
            .expect("login bằng mật khẩu cũ thất bại — mật khẩu đã đổi?");
        eprintln!(
            "✅ Login cũ OK. profile name={:?}",
            login.user.as_ref().and_then(|u| u.get("name"))
        );

        // ── 2. Đổi mật khẩu cũ → mới ──
        let new_password = generate_password();
        assert_ne!(new_password, old_password, "trùng mật khẩu (rất hiếm)");
        client
            .change_password(&login.token, &old_password, &new_password)
            .await
            .expect("change-password thất bại (endpoint 404 đã fix)");
        eprintln!("✅ Đổi mật khẩu OK → mới: {new_password}");

        // ── 3. Login bằng mật khẩu mới ──
        let client2 =
            HddtClient::new(&username, &new_password, None).expect("tạo client 2 thất bại");
        let login2 = client2
            .login_with_solver(&GlyphTemplateSolver, 3)
            .await
            .expect("login bằng mật mới thất bại");
        eprintln!("✅ Login bằng mật mới OK");

        // ── 4. Đổi lại về mật khẩu cũ ──
        client2
            .change_password(&login2.token, &new_password, &old_password)
            .await
            .expect("đổi lại về cũ thất bại");
        eprintln!("✅ Đổi lại về mật khẩu cũ OK");

        // ── 5. Login bằng mật khẩu cũ → xác nhận restore ──
        let client3 =
            HddtClient::new(&username, &old_password, None).expect("tạo client 3 thất bại");
        let _login3 = client3
            .login_with_solver(&GlyphTemplateSolver, 3)
            .await
            .expect("login bằng mật cũ (restore) thất bại");
        eprintln!("✅ Login bằng mật cũ (restore) OK → tài khoản còn nguyên");
    }

    /// Probe 4 endpoint tra cứu hóa đơn (bán ra/mua vào × thường/máy tính tiền)
    /// và dump schema. Run with:
    ///   cd src-tauri && cargo test live_probe_invoices -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "cần ../info.txt + portal live"]
    async fn live_probe_invoices() {
        use crate::hddt::{InvoiceDirection, InvoiceKind, InvoiceQuery};
        use std::fs;

        if !fs::metadata("../info.txt").is_ok() {
            eprintln!("⚠️ skipped — ../info.txt không có");
            return;
        }
        let txt = fs::read_to_string("../info.txt").expect("không đọc được info.txt");
        let mut username = None;
        let mut password = None;
        for line in txt.lines() {
            if let Some((k, v)) = line.split_once('=') {
                match k.trim() {
                    "USERNAME" => username = Some(v.trim().to_string()),
                    "PASSWORD" => password = Some(v.trim().to_string()),
                    _ => {}
                }
            }
        }
        let client = HddtClient::new(
            &username.expect("thiếu USERNAME"),
            &password.expect("thiếu PASSWORD"),
            None,
        )
        .expect("tạo client thất bại");
        let login = client
            .login_with_solver(&GlyphTemplateSolver, 2)
            .await
            .expect("login thất bại");
        eprintln!(
            "✅ Login OK type={:?}",
            login.user.as_ref().and_then(|u| u.get("type"))
        );
        let token = &login.token;
        // Dump token để dùng cho việc inject vào Chrome / curl.
        let _ = std::fs::write("/tmp/opencode/token.txt", format!("{}\n", login.token));
        eprintln!(
            "🔑 token (60 chars) = {}",
            login.token.chars().take(60).collect::<String>()
        );

        let show = |list: &crate::hddt::InvoiceList, label: &str| {
            eprintln!("── {label}: total={} time={}", list.total, list.time);
            if let Some(f) = list.datas.first() {
                let mut keys: Vec<&str> = f
                    .as_object()
                    .map(|o| o.keys().map(|k| k.as_str()).collect())
                    .unwrap_or_default();
                keys.sort_unstable();
                eprintln!("   fields({}): {:?}", keys.len(), keys);
            }
        };

        // Cổng giới hạn khoảng ngày ≤ 1 tháng (HTTP 400 "Khoảng thời gian tìm kiếm
        // không được lớn hơn 1 tháng") → dùng đúng cửa sổ mặc định của trang:
        // hôm nay - 1 tháng + 1 ngày … hôm nay.
        let to = chrono::Local::now();
        let from = to - chrono::Duration::days(30);
        let fmt = |d: chrono::DateTime<chrono::Local>| d.format("%d/%m/%Y").to_string();
        let (from_s, to_s) = (fmt(from), fmt(to));
        eprintln!("📅 khoảng ngày lập: {from_s} → {to_s}");

        // Duyệt cả 4 tab: {bán ra, mua vào} × {HĐĐT thường, máy tính tiền}.
        // "Kết quả kiểm tra" để Tất cả (-1) để so kết quả thuần với tab mặc định.
        for (dir, kind) in [
            (InvoiceDirection::Sold, InvoiceKind::Regular),
            (InvoiceDirection::Sold, InvoiceKind::CashRegister),
            (InvoiceDirection::Purchase, InvoiceKind::Regular),
            (InvoiceDirection::Purchase, InvoiceKind::CashRegister),
        ] {
            let list = client
                .query_invoices(
                    token,
                    &InvoiceQuery {
                        direction: dir,
                        kind,
                        size: 3,
                        from: Some(from_s.clone()),
                        to: Some(to_s.clone()),
                        ttxly: Some("-1".into()),
                        ..Default::default()
                    },
                )
                .await
                .unwrap_or_else(|e| panic!("{dir:?}/{kind:?} thất bại: {e}"));
            show(&list, &format!("{dir:?} × {kind:?}"));
        }

        // Tab "hóa đơn vào" mặc định của cổng gửi kèm ttxly==5 (Đã cấp mã hóa đơn).
        let purchase_default = client
            .query_invoices(
                token,
                &InvoiceQuery {
                    direction: InvoiceDirection::Purchase,
                    kind: InvoiceKind::Regular,
                    size: 3,
                    from: Some(from_s.clone()),
                    to: Some(to_s.clone()),
                    ttxly: Some("5".into()),
                    ..Default::default()
                },
            )
            .await
            .expect("mua vào (mặc định ttxly==5) thất bại");
        show(
            &purchase_default,
            "Purchase × Regular (mặc định cổng: ttxly==5)",
        );
    }

    /// Probe giới hạn `size` của cổng: size=100 bị từ chối (HTTP 500), size=50
    /// chạy được. In ra message thô để kiểm tra encoding của cổng.
    /// Run with: cd src-tauri && cargo test live_probe_page_size -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "cần ../info.txt + portal live"]
    async fn live_probe_page_size() {
        use crate::hddt::{InvoiceDirection, InvoiceKind, InvoiceQuery};
        use std::fs;

        if !fs::metadata("../info.txt").is_ok() {
            eprintln!("⚠️ skipped — ../info.txt không có");
            return;
        }
        let txt = fs::read_to_string("../info.txt").expect("không đọc được info.txt");
        let pick = |k: &str| {
            txt.lines()
                .filter_map(|l| l.split_once('='))
                .find(|(a, _)| a.trim() == k)
                .map(|(_, v)| v.trim().to_string())
        };
        let client = HddtClient::new(
            &pick("USERNAME").expect("thiếu USERNAME"),
            &pick("PASSWORD").expect("thiếu PASSWORD"),
            None,
        )
        .expect("tạo client thất bại");
        let login = client
            .login_with_solver(&GlyphTemplateSolver, 2)
            .await
            .expect("login thất bại");

        let to = chrono::Local::now();
        let from = to - chrono::Duration::days(30);
        let fmt = |d: chrono::DateTime<chrono::Local>| d.format("%d/%m/%Y").to_string();
        let (from_s, to_s) = (fmt(from), fmt(to));

        for size in [100_u32, 50, crate::hddt::MAX_PAGE_SIZE + 1] {
            let q = InvoiceQuery {
                direction: InvoiceDirection::Purchase,
                kind: InvoiceKind::CashRegister,
                size,
                from: Some(from_s.clone()),
                to: Some(to_s.clone()),
                ttxly: Some("-1".into()),
                ..Default::default()
            };
            match client.query_invoices(&login.token, &q).await {
                Ok(list) => eprintln!(
                    "size={size} → OK total={} (client đã clamp còn {})",
                    list.total,
                    crate::hddt::MAX_PAGE_SIZE
                ),
                Err(e) => eprintln!("size={size} → ERR {e}"),
            }
        }
    }

    /// Fetch một captcha tươi và solve — dùng để điền vào form đăng nhập của
    /// Chrome (browser-side login để bắt request thật). In ra `KEY<TAB>ANSWER`.
    /// Run with: cd src-tauri && cargo test captcha_for_browser -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "cần portal live"]
    async fn captcha_for_browser() {
        use crate::hddt::captcha::CaptchaSolver;
        let client = HddtClient::new("x", "x", None).expect("tạo client thất bại");
        for i in 1..=5 {
            let cap = client.fetch_captcha().await.expect("captcha thất bại");
            match GlyphTemplateSolver.solve(&cap) {
                Ok(ans) if !ans.trim().is_empty() => {
                    eprintln!("CAPTCHA\t{}\t{}\t{}", i, cap.key, ans.trim());
                    return;
                }
                _ => eprintln!("attempt {i} không giải được"),
            }
        }
        panic!("không giải được captcha sau 5 lần");
    }

    /// Giải captcha SVG lưu trong file — dùng cho login browser-side.
    /// Run with: cd src-tauri && cargo test solve_svg_file -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "cần file captcha"]
    async fn solve_svg_file() {
        use crate::hddt::classify_svg;
        let path = "/tmp/opencode/captcha-dialog.svg";
        let svg = std::fs::read_to_string(path).expect("không đọc được svg");
        match classify_svg(&svg) {
            Ok(ans) => eprintln!("ANSWER\t{ans}"),
            Err(e) => eprintln!("ERROR\t{e}"),
        }
    }
}

// ─── Bổ sung templates cho solver captcha ───
//
// Quy trình 2 bước (key captcha không ràng buộc session nên tách được):
//   1) `live_capture_captcha_samples` — tải N captcha, lưu SVG + key + đoán của
//      solver kèm IoU từng glyph vào CAPTCHA_DIR.
//   2) Đọc ảnh (file SVG mở bằng trình duyệt), điền đáp án vào `labels.txt`
//      (`<số thứ tự>\t<KỲ TỰ>`), rồi chạy `live_verify_captcha_labels`:
//      mỗi mẫu được xác minh bằng cách đăng nhập thật (key chỉ dùng 1 lần) và
//      chỉ mẫu nào đúng mới được thêm vào bộ template. Đặt CAPTCHA_PROMOTE=1
//      để ghi đè `templates.bin` sau khi đã xem báo cáo.

use crate::hddt::HddtClient;

const CAPTCHA_DIR_DEFAULT: &str = "../captcha-samples";

fn sample_dir() -> String {
    std::env::var("CAPTCHA_DIR").unwrap_or_else(|_| CAPTCHA_DIR_DEFAULT.to_string())
}

fn load_portal_creds() -> Option<(String, String)> {
    let txt = std::fs::read_to_string("../info.txt").ok()?;
    let pick = |k: &str| {
        txt.lines()
            .filter_map(|l| l.split_once('='))
            .find(|(a, _)| a.trim() == k)
            .map(|(_, v)| v.trim().to_string())
    };
    Some((pick("USERNAME")?, pick("PASSWORD")?))
}

/// Tải N captcha + lưu lại để đọc tay.
#[tokio::test]
#[ignore = "cần ../info.txt + portal live"]
async fn live_capture_captcha_samples() {
    use crate::hddt::solver::classify_glyphs_detailed;
    let Some((u, p)) = load_portal_creds() else {
        eprintln!("⚠️ thiếu ../info.txt");
        return;
    };
    let n: usize = std::env::var("CAPTCHA_SAMPLES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(12);
    let dir = sample_dir();
    std::fs::create_dir_all(&dir).expect("tạo thư mục mẫu");
    let client = HddtClient::new(&u, &p, None).expect("client");
    let mut manifest = String::from("index\tfile\tiou\nguess\n");
    let mut low = 0;
    for i in 0..n {
        let cap = client.fetch_captcha().await.expect("tải captcha");
        let file = format!("{dir}/sample_{i:02}.svg");
        std::fs::write(&file, &cap.content).expect("ghi svg");
        let detailed = classify_glyphs_detailed(&cap.content);
        let guess: String = detailed.iter().map(|(c, _)| *c).collect();
        let iou_min = detailed.iter().map(|(_, s)| *s).fold(1.0f32, f32::min);
        if iou_min < 0.75 {
            low += 1;
        }
        manifest.push_str(&format!(
            "{i:02}\t{}\t{:.3}\t{}\t{}\n",
            file, iou_min, guess, cap.key
        ));
        eprintln!("{i:02} iou_min={iou_min:.3} guess={guess} → {file}");
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    }
    std::fs::write(format!("{dir}/samples.tsv"), manifest).expect("ghi manifest");
    eprintln!(
        "✅ {} mẫu tại {dir} (glyph yếu: {low}). Điền labels.txt rồi chạy live_verify_captcha_labels",
        n
    );
}

/// Xác minh nhãn người đọc ảnh: chỉ mẫu nào cổng đăng nhập thành công mới
/// được dùng để bổ sung `templates.bin` (sinh lại bằng
/// `src-tauri/tools/hddt-captcha/export_templates.py` từ thư mục `labeled/`).
///
/// Ghi `verified.tsv` trong CAPTCHA_DIR: index, key, đáp án đã xác minh, file SVG
/// để copy sang `labeled/`.
#[tokio::test]
#[ignore = "cần ../info.txt + portal live"]
async fn live_verify_captcha_labels() {
    let Some((u, p)) = load_portal_creds() else {
        eprintln!("⚠️ thiếu ../info.txt");
        return;
    };
    let dir = sample_dir();
    let Ok(manifest) = std::fs::read_to_string(format!("{dir}/samples.tsv")) else {
        eprintln!("⚠️ chưa có {dir}/samples.tsv — chạy live_capture_captcha_samples trước");
        return;
    };
    let mut labels: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    if let Ok(txt) = std::fs::read_to_string(format!("{dir}/labels.txt")) {
        for line in txt.lines().filter(|l| !l.trim().is_empty()) {
            let mut it = line.split('\t');
            if let (Some(idx), Some(ans)) = (it.next(), it.next()) {
                labels.insert(idx.trim().to_string(), ans.trim().to_string());
            }
        }
    }
    if labels.is_empty() {
        eprintln!("⚠️ chưa có {dir}/labels.txt (mỗi dòng: <index>\t<KỲ TỰ>)");
        return;
    }
    let client = HddtClient::new(&u, &p, None).expect("client");
    let mut verified = String::from("index\tkey\tanswer\tfile\n");
    let mut ok_n = 0;
    for line in manifest.lines().skip(1).filter(|l| !l.trim().is_empty()) {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 5 {
            continue;
        }
        let (idx, file, key) = (cols[0], cols[1], cols[4]);
        let Some(answer) = labels.get(idx) else {
            eprintln!("⚠️ {idx}: chưa có nhãn");
            continue;
        };
        let Ok(svg) = std::fs::read_to_string(file) else {
            eprintln!("⚠️ {idx}: thiếu file {file}");
            continue;
        };
        // Số glyph phải khớp số ký tự để ghép nhãn đúng thứ tự trái→phải.
        let n_glyph = crate::hddt::solver::glyph_paths(&svg).len();
        if n_glyph != answer.chars().count() {
            eprintln!(
                "⚠️ {idx}: SVG có {n_glyph} glyph, đáp án {} ký tự",
                answer.chars().count()
            );
            continue;
        }
        // Xác minh bằng chính cổng: mỗi captcha key chỉ dùng được một lần.
        match client.authenticate(key, answer).await {
            Ok(_) => {
                verified.push_str(&format!("{idx}\t{key}\t{answer}\t{file}\n"));
                ok_n += 1;
                eprintln!("✅ {idx} {answer}");
            }
            Err(e) => eprintln!("❌ {idx} \"{answer}\" bị cổng từ chối — {e}"),
        }
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    }
    std::fs::write(format!("{dir}/verified.tsv"), verified).expect("ghi verified.tsv");
    eprintln!(
        "── {ok_n}/{} mẫu được cổng xác nhận → {dir}/verified.tsv",
        manifest
            .lines()
            .skip(1)
            .filter(|l| !l.trim().is_empty())
            .count()
    );
}

/// Thu hoạch template không cần người đọc: mỗi vòng lấy 1 captcha → solver đoán
/// → đăng nhập thật. Cổng **chấp nhận** nghĩa là chuỗi đoán đúng 100%, nên các
/// glyph của mẫu đó được lưu thẳng vào `labeled/` (đúng định dạng
/// `<index>_<key>_<ANSWER>.svg`) để `export_templates.py` dùng.
///
/// Mẫu bị từ chối = biến thể glyph mà solver chưa có template → lưu vào
/// `hard/` để đọc tay bổ sung (captcha có TTL ngắn nên ảnh đó chỉ dùng để GHI
/// NHÃN, không verify lại được).
///
/// Biến môi trường: `HARVEST_ROUNDS` (mặc định 30), `HARVEST_MAX_FAIL` (mặc
/// định 10 liên tiếp), `HARVEST_DIR` (mặc định `../src-tauri/tools/hddt-captcha`).
#[tokio::test]
#[ignore = "cần ../info.txt + portal live"]
async fn live_harvest_captcha_templates() {
    use crate::hddt::solver::{classify_svg, glyph_paths};
    let Some((u, p)) = load_portal_creds() else {
        eprintln!("⚠️ thiếu ../info.txt");
        return;
    };
    let rounds: usize = std::env::var("HARVEST_ROUNDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30);
    let max_fail: usize = std::env::var("HARVEST_MAX_FAIL")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);
    let base = std::env::var("HARVEST_DIR")
        .unwrap_or_else(|_| "../src-tauri/tools/hddt-captcha".to_string());
    let labeled = format!("{base}/labeled");
    let hard = format!("{base}/hard");
    std::fs::create_dir_all(&labeled).expect("tạo labeled/");
    std::fs::create_dir_all(&hard).expect("tạo hard/");

    let client = HddtClient::new(&u, &p, None).expect("client");
    let (mut ok, mut fail, mut streak) = (0usize, 0usize, 0usize);
    for i in 0..rounds {
        let cap = match client.fetch_captcha().await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[{i}] lỗi tải captcha: {e}");
                streak += 1;
                if streak >= max_fail {
                    break;
                }
                continue;
            }
        };
        let Ok(answer) = classify_svg(&cap.content) else {
            eprintln!("[{i}] SVG không có glyph");
            continue;
        };
        let n_glyph = glyph_paths(&cap.content).len();
        if n_glyph != answer.chars().count() {
            eprintln!(
                "[{i}] bỏ qua: {n_glyph} glyph vs {} ký tự",
                answer.chars().count()
            );
            continue;
        }
        // Cổng trả 200 + token ⇒ chuỗi đoán đúng ⇒ nhãn tin cậy tuyệt đối.
        match client.authenticate(&cap.key, &answer).await {
            Ok(_) => {
                ok += 1;
                streak = 0;
                let file = format!("{labeled}/{i:04}_{}_{}.svg", cap.key, answer);
                std::fs::write(&file, &cap.content).expect("ghi mẫu");
                eprintln!("[{i}] ✅ {answer} → {}", file.rsplit('/').next().unwrap());
            }
            Err(_) => {
                fail += 1;
                streak += 1;
                let file = format!("{hard}/{i:04}_{}_{}.svg", cap.key, answer);
                std::fs::write(&file, &cap.content).expect("ghi mẫu khó");
                eprintln!(
                    "[{i}] ❌ {answer} (cần đọc tay) → {}",
                    file.rsplit('/').next().unwrap()
                );
                if streak >= max_fail {
                    eprintln!("⏹ dừng sau {max_fail} lần liên tiếp sai");
                    break;
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(700)).await;
    }
    eprintln!("── thu hoạch xong: {ok} mẫu đã xác minh, {fail} mẫu cần đọc tay");
    eprintln!("   chạy: python3 src-tauri/tools/hddt-captcha/export_templates.py");
}
