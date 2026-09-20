// ─── Live test against the real HDDT portal ───
// Reads credentials from `../../info.txt` (gitignored, never committed).
// Performs a full password-change round-trip and RESTORES the original
// password at the end so the account is left unchanged.
//
// Run with: cd src-tauri && cargo test live_hddt -- --nocapture

#[cfg(test)]
mod live {
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
}
