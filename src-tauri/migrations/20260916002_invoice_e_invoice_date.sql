-- =============================================
-- 20260916002: Hóa đơn — bổ sung ngày HĐĐT
-- =============================================

ALTER TABLE invoice ADD COLUMN e_invoice_date TEXT NOT NULL DEFAULT '';