-- =============================================
-- 20261004001: Lưu trữ file XML hóa đơn điện tử
--
-- Quy định: người bán, người mua phải lưu trữ hóa đơn điện tử ở dạng file XML.
-- Cổng HĐĐT có endpoint `GET /api/{query|sco-query}/invoices/export-xml` trả
-- về ZIP chứa `invoice.xml` (bắt từ bundle 10/2026, kiểm chứng live 04/10/2026).
-- App tải ZIP, trích riêng `invoice.xml` vào thư mục hồ sơ HKD rồi ghi lại
-- đường dẫn ở bảng này để biết hóa đơn nào đã lưu.
--
-- Khóa danh tính = đúng 4 tham số gọi cổng (nbmst, khmshdon, khhdon, shdon)
-- cộng chiều (bán/vào) và loại (HĐĐT/máy tính tiền) — UNIQUE để quét lại hay
-- bấm nút hai lần không ghi trùng file.
-- =============================================

CREATE TABLE hddt_invoice_xml (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    -- Id hóa đơn trên cổng ('' khi không có).
    portal_id  TEXT NOT NULL DEFAULT '',
    -- 'sold' = hóa đơn bán ra, 'purchase' = hóa đơn mua vào.
    direction  TEXT NOT NULL DEFAULT 'purchase',
    -- 'regular' = HĐ điện tử (query), 'cash-register' = máy tính tiền (sco-query).
    kind       TEXT NOT NULL DEFAULT 'regular',
    -- 4 tham số bắt buộc của endpoint export-xml.
    nbmst      TEXT NOT NULL DEFAULT '',
    khmshdon   INTEGER NOT NULL DEFAULT 0,
    khhdon     TEXT NOT NULL DEFAULT '',
    shdon      TEXT NOT NULL DEFAULT '',
    -- Tên file XML trong thư mục `hddt_xml/` của hồ sơ HKD.
    file_name  TEXT NOT NULL DEFAULT '',
    byte_size  INTEGER NOT NULL DEFAULT 0,
    -- 'auto' = tải trong lúc quét/nhập, 'manual' = người dùng bấm nút.
    source     TEXT NOT NULL DEFAULT 'auto',
    saved_at   TEXT NOT NULL DEFAULT ''
);

-- Không cho ghi trùng cùng một hóa đơn (quét lại / bấm nút 2 lần = idempotent).
CREATE UNIQUE INDEX ux_hddt_invoice_xml_key
    ON hddt_invoice_xml (direction, kind, nbmst, khmshdon, khhdon, shdon);

CREATE INDEX ix_hddt_invoice_xml_portal ON hddt_invoice_xml (portal_id);
