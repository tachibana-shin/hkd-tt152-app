-- Ngày bắt đầu sử dụng hóa đơn điện tử (HĐĐT) của hộ kinh doanh.
-- Mốc này là ngày lấp hóa đơn (tra cứu hóa đơn vào) — phải bắt buộc nhập
-- trong popup cấu hình HKD để biết tìm hóa đơn mua vào từ ngày nào.
-- Rỗng = hồ sơ cũ chưa khai báo (UI bắt buộc nhập trước khi dùng tính năng).
ALTER TABLE business ADD COLUMN hddt_start_date TEXT NOT NULL DEFAULT '';
