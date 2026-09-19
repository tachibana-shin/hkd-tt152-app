-- =============================================
-- 20260919002: Popup "Lập hóa đơn bán hàng" — dòng hóa đơn thêm 2 trường:
--
--  - discount: số tiền chiết khấu thương mại (đ) — cột "Tiền CK" của hóa đơn
--    bán hàng; giá trị dòng = Thành tiền (SL × Đơn giá) − Tiền CK.
--  - warehouse_code: kho xuất trên từng dòng (thông tin — hóa đơn nháp không
--    trừ tồn kho; rỗng = kho mặc định của sản phẩm / kho đầu tiên).
-- =============================================

ALTER TABLE invoice_item ADD COLUMN discount REAL NOT NULL DEFAULT 0.0;
ALTER TABLE invoice_item ADD COLUMN warehouse_code TEXT NOT NULL DEFAULT '';