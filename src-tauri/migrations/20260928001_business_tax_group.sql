-- Nhóm hộ kinh doanh theo doanh thu cả năm (NĐ 68/2026, NĐ 141/2026):
--   1 — DT ≤ 1 tỷ (miễn thuế, thông báo DT năm)
--   2 — DT 1–3 tỷ · 3 — DT 3–50 tỷ · 4 — DT > 50 tỷ
--
-- NULL = chưa chốt, app tự xếp theo doanh thu như trước. Hộp thoại cấu hình HKD
-- tự điền giá trị xếp được rồi lưu lại đây, nên người dùng thấy và xác nhận
-- được; hộ bị cơ quan thuế xếp khác mức doanh thu thì sửa tay ở chính ô này
-- mà không cần sửa code.
ALTER TABLE business ADD COLUMN tax_group INTEGER;
