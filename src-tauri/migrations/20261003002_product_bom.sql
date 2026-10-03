-- =============================================
-- 20261003002: ĐỊNH MỨC VẬT TƯ (BOM) — F4.
--
-- NVL → thành phẩm: định mức ghi "1 thành phẩm cần bao nhiêu NVL".
-- Khi lưu phiếu nhập kho loại `production`, app tự tính
--   NVL cần = định mức × SL thành phẩm
-- rồi tự sinh phiếu xuất NVL (Nợ 154 / Có 152) trong CÙNG transaction —
-- người dùng không nhập tay gì.
--
-- UNIQUE(product_id, material_product_id): mỗi NVL chỉ 1 dòng trong định mức
-- của một thành phẩm (trùng dòng → gộp/lỗi ở tầng command).
-- ON DELETE CASCADE cả hai chiều: xóa thành phẩm hoặc xóa NVL thì dòng định
-- mức liên quan tự mất theo.
-- =============================================

CREATE TABLE IF NOT EXISTS product_bom_item (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    product_id          INTEGER NOT NULL REFERENCES product(id) ON DELETE CASCADE,
    material_product_id INTEGER NOT NULL REFERENCES product(id) ON DELETE CASCADE,
    quantity            REAL NOT NULL DEFAULT 0.0,
    UNIQUE (product_id, material_product_id)
);

CREATE INDEX IF NOT EXISTS idx_product_bom_product ON product_bom_item(product_id);
