-- Ký hiệu (mẫu số) hóa đơn điện tử của hộ, ví dụ "1C26TT152". Mỗi hộ có một
-- ký hiệu riêng nên trường này hợp lý nằm trong hồ sơ HKD.
--
-- Hộp thoại "Liên kết HĐĐT" lấy ký hiệu này làm giá trị mặc định và tự cập
-- nhật theo lần dùng gần nhất (xem `link_hddt`), nên đổi mẫu số theo năm chỉ
-- cần ghi ở đây hoặc gõ tay khi liên kết — không phải sửa lại từng hóa đơn.
ALTER TABLE business ADD COLUMN hddt_symbol TEXT NOT NULL DEFAULT '';
