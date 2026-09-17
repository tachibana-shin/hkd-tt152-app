-- Cấu hình mặc định cho danh mục sản phẩm.
-- Các thông tư / chính sách thuế thay đổi liên tục nên giá trị mặc định được
-- cấu hình trong bảng app_setting (key-value) thay vì hardcode trong code.

CREATE TABLE IF NOT EXISTS app_setting (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL DEFAULT ''
);

INSERT OR IGNORE INTO app_setting (key, value) VALUES
    ('product_code_prefix',            'SP'),
    ('product_code_start',             '1'),
    ('product_code_digits',            '4'),
    ('product_vat_rate_options',       '[1,3,5]'),
    ('product_vat_rate_default',       '1'),
    ('product_import_tax_options',     '[0,5,8,10,15,20,25,30]'),
    ('product_import_tax_default',     '0'),
    ('product_unit',                   'Cái'),
    ('product_min_stock',              '0');

-- Thuế nhập khẩu theo sản phẩm (lưu dạng tỷ lệ, ví dụ 0.05 = 5%)
ALTER TABLE product ADD COLUMN import_tax_rate REAL NOT NULL DEFAULT 0.0;