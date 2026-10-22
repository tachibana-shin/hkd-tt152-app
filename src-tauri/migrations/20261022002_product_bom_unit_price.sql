-- =============================================
-- 20261022002: ĐƠN GIÁ ĐỊNH MỨC VẬT TƯ (BOM).
--
-- Trước đây màn định mức chỉ hiện "Giá bán" của danh mục. Thực tế một gói
-- sửa chữa / chế tạo thường có nhân công + vật tư với giá thỏa thuận riêng,
-- nên đơn giá được khai và lưu ngay trên từng dòng định mức.
--
-- Backfill theo Giá bán hiện tại để các định mức đã khai giữ nguyên số liệu
-- như trước — người dùng chỉ sửa khi thực sự muốn đổi giá hợp đồng.
-- =============================================

ALTER TABLE product_bom_item ADD COLUMN unit_price REAL NOT NULL DEFAULT 0.0;

UPDATE product_bom_item
SET unit_price = COALESCE(
    (SELECT p.sale_price FROM product p WHERE p.id = product_bom_item.material_product_id),
    0.0
);
