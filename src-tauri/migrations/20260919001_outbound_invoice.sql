-- =============================================
-- 20260919001: Hóa đơn bán hàng lập kèm phiếu xuất kho
--
-- Số phiếu xuất nguồn liên kết hóa đơn ↔ PX (1-1): phiếu xuất bán hàng tự sinh
-- 1 hóa đơn vào tab "Hóa đơn" với số hóa đơn = số phiếu xuất (không lệch sổ).
-- =============================================

ALTER TABLE invoice ADD COLUMN voucher_no TEXT NOT NULL DEFAULT '';