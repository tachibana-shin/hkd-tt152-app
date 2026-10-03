-- =============================================
-- 20261003003: invoice_item.line_name — TÊN HIỂN THỊ CỦA DÒNG HÓA ĐƠN (F5).
--
-- Khi người dùng chọn một "tên khác" (alias) của sản phẩm trên hóa đơn, app
-- phải GIỮ tên đó in ra — không fallback về tên chính. Cột này lưu đúng tên
-- người dùng đã chọn; rỗng = dùng tên sản phẩm (COALESCE(NULLIF(...,''), p.name))
-- ở mọi chỗ in/xem/chi tiết.
-- =============================================

ALTER TABLE invoice_item ADD COLUMN line_name TEXT NOT NULL DEFAULT '';
