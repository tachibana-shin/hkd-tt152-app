-- Đánh dấu thủ công "khoảng kỳ tờ khai đã nộp" (hộ tự kê khai ở CQT — app không
-- liên kết hệ thống thuế). Hóa đơn nằm trong khoảng này vẫn SỬA ĐƯỢC (đúng cam
-- kết: kỳ đóng tờ khai vẫn cho sửa), nhưng mọi thao tác phải kèm cảnh báo để
-- người dùng biết số liệu đã gửi có thể lệch và tự kê khai bổ sung nếu cần.
-- Lưu theo đúng khoảng ngày user đang xem trên màn Tờ khai (tháng / quý / năm
-- đều được, không cứng theo YYYY-MM).
CREATE TABLE IF NOT EXISTS tax_period_closed (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    from_date TEXT NOT NULL,
    to_date   TEXT NOT NULL,
    note      TEXT NOT NULL DEFAULT '',
    actor     TEXT NOT NULL DEFAULT '',
    closed_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (from_date, to_date)
);
