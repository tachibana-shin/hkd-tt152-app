-- =============================================
-- 20261022001 — Nhật ký mật khẩu cổng HĐĐT (chống khóa tài khoản)
--
-- Yêu cầu (follow-up 22/10/2026): đổi mật khẩu / auto đổi mật khẩu HĐĐT phải
-- lưu TOÀN BỘ mật khẩu đã dùng ra BẢNG RIÊNG (không phụ thuộc 1 ô key/value
-- dễ mất giá trị); khi nhận phản hồi "mật khẩu không đúng" thì GHI NHẬN mật
-- khẩu đó vào bảng và TỪ CHỐI thử lại cho tới khi tài khoản đăng nhập thành
-- công (bảng được reset) — tránh cổng tạm khóa tài khoản do nhập sai nhiều lần.
--
-- `app_setting.hddt_password` vẫn là mật khẩu ACTIVE đọc ghi như cũ; bảng này
-- là nhật ký + cờ kết quả thử gần nhất.
-- state: 'unknown' = chưa thử, 'ok' = đăng nhập thành công gần nhất dùng nó,
--        'bad' = cổng phản hồi mật khẩu không đúng (không thử lại tự động).
-- =============================================

CREATE TABLE IF NOT EXISTS hddt_password_journal (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  username TEXT NOT NULL,
  password TEXT NOT NULL,
  state TEXT NOT NULL DEFAULT 'unknown',
  note TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  marked_at TEXT,
  UNIQUE(username, password)
);

CREATE INDEX IF NOT EXISTS idx_hddt_password_journal_username
  ON hddt_password_journal(username);
