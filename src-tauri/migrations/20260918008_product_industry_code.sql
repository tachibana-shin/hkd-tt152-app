-- Liên kết sản phẩm → nhóm ngành: cơ sở tính thuế bán ra (tỷ lệ GTGT/TNCN theo nhóm).
-- - product.industry_code: nhóm ngành mặc định của sản phẩm → tự điền vào dòng
--   phiếu xuất / hóa đơn (vẫn cho phép đổi trên từng dòng).
-- - invoice_item.industry_code / vat_rate / pit_rate: lưu cơ sở thuế đã dùng
--   cho từng dòng hóa đơn bán hàng (HKD không tách thuế trên hóa đơn, nhưng theo
--   dõi tỷ lệ nhóm để tính thuế phải nộp).
ALTER TABLE product ADD COLUMN industry_code TEXT NOT NULL DEFAULT '';

ALTER TABLE invoice_item ADD COLUMN industry_code TEXT NOT NULL DEFAULT '';
ALTER TABLE invoice_item ADD COLUMN vat_rate REAL NOT NULL DEFAULT 0;
ALTER TABLE invoice_item ADD COLUMN pit_rate REAL NOT NULL DEFAULT 0;