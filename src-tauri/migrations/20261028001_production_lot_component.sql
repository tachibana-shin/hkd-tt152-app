-- =============================================
-- 20261028001: THÀNH PHẦN CỦA TỪNG LÔ SẢN XUẤT (snapshot BOM).
--
-- Bảng định mức vật tư (product_bom_item) được sửa tự do, nên nếu chỉ đọc lại
-- định mức hiện hành thì lô cũ sẽ hiển thị sai thành phần / giá trị. Mỗi lần
-- lưu phiếu nhập loại `production`, app chụp lại toàn bộ dòng định mức của
-- thành phẩm vào bảng này: số lượng định mức, số lượng thực tế của lô, đơn giá
-- đã khai, thành tiền theo giá khai và giá vốn FIFO thực tế của NVL đã xuất.
--
-- `inbound_voucher_id` ON DELETE CASCADE: xoá phiếu nhập thì snapshot đi theo.
-- Không đặt FK cho `material_id`: vật tư có thể bị xoá khỏi danh mục sau này
-- nhưng lô đã sản xuất vẫn phải giữ đúng tên/mã tại thời điểm chụp.
-- =============================================

CREATE TABLE IF NOT EXISTS production_lot_component (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    inbound_voucher_id INTEGER NOT NULL REFERENCES inbound_voucher(id) ON DELETE CASCADE,
    product_code       TEXT NOT NULL,
    product_name       TEXT NOT NULL DEFAULT '',
    material_id        INTEGER,
    material_code      TEXT NOT NULL,
    material_name      TEXT NOT NULL DEFAULT '',
    material_unit      TEXT NOT NULL DEFAULT '',
    is_service         INTEGER NOT NULL DEFAULT 0,
    -- Định mức cho 1 đơn vị thành phẩm và sản lượng thành phẩm của dòng lô.
    per_unit_qty       REAL NOT NULL DEFAULT 0,
    finished_qty       REAL NOT NULL DEFAULT 0,
    -- Số lượng vật tư cần cho cả lô = per_unit_qty × finished_qty.
    quantity           REAL NOT NULL DEFAULT 0,
    -- Đơn giá khai trên dòng định mức + thành tiền theo giá khai.
    unit_price         REAL NOT NULL DEFAULT 0,
    amount             REAL NOT NULL DEFAULT 0,
    -- Giá vốn FIFO bình quân thực tế của phần NVL đã xuất (0 với dịch vụ).
    unit_cost          REAL NOT NULL DEFAULT 0,
    cost_amount        REAL NOT NULL DEFAULT 0,
    warehouse_code     TEXT NOT NULL DEFAULT '',
    created_at         TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_production_lot_component_voucher
    ON production_lot_component(inbound_voucher_id);
