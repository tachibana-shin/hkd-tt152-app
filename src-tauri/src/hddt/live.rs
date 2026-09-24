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
