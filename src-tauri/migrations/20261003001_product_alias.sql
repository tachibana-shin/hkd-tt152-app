-- =============================================
-- 20261003001: TÊN KHÁC (alias) của sản phẩm — F5.
--
-- Một mặt hàng (khối HDLX, hàng đóng gói…) có thể có nhiều tên người dùng gõ
-- khác nhau trên sổ. Cho phép khai nhiều tên cho MỘT sản phẩm để:
--   • tìm sản phẩm bằng mọi tên người dùng hay gõ (bộ chọn + lưới danh mục);
--   • chọn một "tên khác" trên hóa đơn và GIỮ tên đó (không fallback về tên
--     chính) — xem invoice_item.line_name ở 20261003003.
--
-- alias_key: khóa chuẩn hóa (bỏ hoa/thường, gộp khoảng trắng — fold_key) để
-- tra nhanh và chặn hai sản phẩm cùng một tên. UNIQUE toàn bảng: mỗi tên chỉ
-- gán cho MỘT sản phẩm.
-- =============================================

CREATE TABLE IF NOT EXISTS product_alias (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    product_id INTEGER NOT NULL REFERENCES product(id) ON DELETE CASCADE,
    alias      TEXT NOT NULL,
    alias_key  TEXT NOT NULL UNIQUE
);

CREATE INDEX IF NOT EXISTS idx_product_alias_product ON product_alias(product_id);
