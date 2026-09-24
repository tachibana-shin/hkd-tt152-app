-- =============================================
-- 20260925002 — Tách cache quét cổng khỏi hóa đơn đã nhập kho
--
-- `hddt_purchase_invoice` trước đây vừa là cache kết quả quét cổng, vừa lưu
-- hóa đơn đã nhập kho → xoá cache là phải lọc theo status và dữ liệu đã nhập
-- nằm chung chỗ với dữ liệu thô. Nay:
--
--   hddt_purchase_invoice  → CACHE kết quả quét (xoá được tự do)
--   hddt_imported_invoice  → HÓA ĐƠN CHÍNH THỨC đã nhập kho + liên kết phiếu
--                             (1 hóa đơn = 1 phiếu nhập, FK bắt buộc)
--
-- Hóa đơn đã nhập kho không bao giờ nằm ở cache: sau khi tạo phiếu, dòng cache
-- tương ứng bị xoá và dữ liệu chính thức nằm ở bảng mới.
-- =============================================

CREATE TABLE IF NOT EXISTS hddt_imported_invoice (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    -- id của cổng (UUID): UNIQUE = chốt trùng tuyệt đối, kể cả khi xoá cache
    -- rồi quét lại ngày đó.
    portal_id          TEXT NOT NULL UNIQUE,
    -- regular | cash-register (query | sco-query)
    portal_kind        TEXT NOT NULL DEFAULT '',
    posting_date       TEXT NOT NULL DEFAULT '',
    nbmst              TEXT NOT NULL DEFAULT '',
    nbten              TEXT NOT NULL DEFAULT '',
    nmmst              TEXT NOT NULL DEFAULT '',
    khmshdon           INTEGER,
    khhdon             TEXT NOT NULL DEFAULT '',
    shdon              TEXT NOT NULL DEFAULT '',
    tgtcthue           REAL NOT NULL DEFAULT 0,
    tgtthue            REAL NOT NULL DEFAULT 0,
    ttcktmai           REAL NOT NULL DEFAULT 0,
    tgtttbso           REAL NOT NULL DEFAULT 0,
    line_count         INTEGER NOT NULL DEFAULT 0,
    new_product_count  INTEGER NOT NULL DEFAULT 0,
    supplier_id        INTEGER REFERENCES supplier(id),
    -- Liên kết chặt hóa đơn ↔ phiếu nhập: bắt buộc, và 1 phiếu chỉ có 1 HĐ.
    inbound_voucher_id INTEGER NOT NULL REFERENCES inbound_voucher(id),
    -- Ảnh chụp dữ liệu lúc nhập để đối chiếu với cổng.
    raw_json           TEXT NOT NULL DEFAULT '',
    detail_json        TEXT NOT NULL DEFAULT '',
    imported_at        TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE UNIQUE INDEX IF NOT EXISTS ux_hddt_imported_voucher
    ON hddt_imported_invoice(inbound_voucher_id);
CREATE INDEX IF NOT EXISTS ix_hddt_imported_date
    ON hddt_imported_invoice(posting_date);

-- Chuyển các hóa đơn đã nhập kho sẵn có sang bảng mới, rồi dọn khỏi cache.
INSERT INTO hddt_imported_invoice
    (portal_id, portal_kind, posting_date, nbmst, nbten, nmmst, khmshdon, khhdon, shdon,
     tgtcthue, tgtthue, ttcktmai, tgtttbso, line_count, new_product_count, supplier_id,
     inbound_voucher_id, raw_json, detail_json, imported_at)
SELECT portal_id, portal_kind, posting_date, nbmst, nbten, nmmst, khmshdon, khhdon, shdon,
       tgtcthue, tgtthue, ttcktmai, tgtttbso, line_count, new_product_count, supplier_id,
       inbound_voucher_id, raw_json, detail_json, datetime('now')
  FROM hddt_purchase_invoice
 WHERE status = 'imported' AND inbound_voucher_id IS NOT NULL;

DELETE FROM hddt_purchase_invoice WHERE status = 'imported';
