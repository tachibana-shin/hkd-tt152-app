-- Chuẩn hóa mã đơn vị ghi sổ (unit) và kho mặc định: HaNoi-01 → HKD / KHO-CHINH.
-- Áp dụng cho DB đã chạy seed cũ; với bản cài mới (đã seed mã mới) thì không có
-- dòng nào bị đổi (idempotent). Kho được tham chiếu qua warehouse_id nên đổi mã
-- không ảnh hưởng stock_lot / kiểm kê.
UPDATE business_unit SET code = 'HKD', name = 'Cơ sở chính' WHERE code = 'HaNoi-01';
UPDATE journal_entry SET unit_code = 'HKD' WHERE unit_code = 'HaNoi-01';
UPDATE warehouse SET code = 'KHO-CHINH' WHERE code = 'HaNoi-01';