-- =============================================
-- 20260916005_users.sql — Phân quyền đa người dùng
-- Bảng người dùng + vai trò (admin / ketoan / kho / xem) + gán tên người dùng
-- vào audit log (ai làm gì).
-- Người dùng admin mặc định (admin/admin123) được seed khi khởi động app
-- (từ code — băm SHA-256 salt ngẫu nhiên, không thể seed tĩnh trong SQL).
-- =============================================
CREATE TABLE IF NOT EXISTS app_user (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    username      TEXT UNIQUE NOT NULL,
    display_name  TEXT NOT NULL DEFAULT '',
    password_hash TEXT NOT NULL DEFAULT '',   -- định dạng "salt_hex:hash_hex" (SHA-256 salt ngẫu nhiên)
    role          TEXT NOT NULL DEFAULT 'xem', -- admin | ketoan | kho | xem
    active        INTEGER NOT NULL DEFAULT 1
);

-- Gán tên người dùng đang thao tác cho mỗi dòng audit (rỗng nếu chưa đăng nhập)
ALTER TABLE audit_log ADD COLUMN username TEXT NOT NULL DEFAULT '';
CREATE INDEX IF NOT EXISTS idx_audit_username ON audit_log(username);