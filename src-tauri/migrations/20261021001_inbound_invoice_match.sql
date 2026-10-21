-- =============================================
-- 20261021001: Khớp phiếu nhập tay với hóa đơn mua từ cổng HĐĐT
--
-- 1) inbound_voucher: lưu ký hiệu + số + ngày hóa đơn. Khi đồng bộ HĐĐT,
--    nếu user đã nhập tay phiếu nhập trùng CẢ 3 (không so MST người bán —
--    số hiệu + ngày là đủ) thì app cập nhật chính phiếu đó thay vì tạo phiếu
--    mới, giữ nguyên số phiếu nội bộ user đã ghi.
-- 2) stock_lot: neo lô nhập về đầu phiếu. Lúc ghi đè, app chỉ xoá được lô
--    còn nguyên (chưa xuất/điều chỉnh); lô đã dùng → từ chối, tránh nhân đôi
--    tồn kho hoặc làm lệch giá vốn đã bán.
-- =============================================

ALTER TABLE inbound_voucher ADD COLUMN invoice_serial TEXT NOT NULL DEFAULT '';
ALTER TABLE inbound_voucher ADD COLUMN invoice_no TEXT NOT NULL DEFAULT '';
ALTER TABLE inbound_voucher ADD COLUMN invoice_date TEXT NOT NULL DEFAULT '';

CREATE INDEX IF NOT EXISTS idx_inbound_voucher_invoice
    ON inbound_voucher(invoice_serial, invoice_no, invoice_date);

ALTER TABLE stock_lot ADD COLUMN inbound_voucher_id INTEGER
    REFERENCES inbound_voucher(id);

CREATE INDEX IF NOT EXISTS idx_stock_lot_inbound_voucher
    ON stock_lot(inbound_voucher_id);
