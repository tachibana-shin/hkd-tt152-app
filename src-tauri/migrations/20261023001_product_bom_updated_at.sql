-- =============================================
-- 20261023001: MỐC CẬP NHẬT ĐỊNH MỨC VẬT TƯ (BOM).
--
-- Màn "Định mức vật tư" phải xếp bảng mới cập nhật/tạo lên trước. Bảng định
-- mức không có dòng tiêu đề riêng: mỗi lần lưu, command xóa rồi ghi lại TOÀN
-- BỘ dòng của một thành phẩm, nên mốc thời gian trên dòng chính là lần lưu
-- gần nhất của định mức đó.
--
-- ADD COLUMN không nhận default động như datetime('now'), nên dùng default
-- rỗng rồi backfill các dòng cũ bằng thời điểm chạy migration; các lần lưu
-- sau ghi CURRENT_TIMESTAMP tường minh.
-- =============================================

ALTER TABLE product_bom_item ADD COLUMN updated_at TEXT NOT NULL DEFAULT '';

UPDATE product_bom_item
SET updated_at = CURRENT_TIMESTAMP
WHERE updated_at = '';
