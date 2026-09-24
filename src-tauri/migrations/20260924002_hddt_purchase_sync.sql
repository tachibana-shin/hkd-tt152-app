-- =============================================
-- 20260924002: Đồng bộ hóa đơn mua vào từ cổng HĐĐT
--
-- 1) product: khoá danh tính theo (tên, đơn vị tính) + mã hàng của cổng HĐĐT.
--    identity_key do RÚT ghi bằng chuẩn hoá Unicode (không dùng lower() của
--    SQLite vì chỉ hỗ trợ ASCII, sẽ lệch với tiếng Việt có dấu).
--    Hàng trùng (tên + đơn vị) sẵn có: chỉ dòng id nhỏ nhất được gán khoá,
--    các dòng còn lại để khoá rỗng (đồng bộ sẽ tạo mặt hàng mới thay vì
--    nhập nhầm vào hàng trùng).
-- 2) inbound_voucher: bảng đầu phiếu nhập kho. Trước đây phiếu nhập chỉ tồn
--    tại dạng các dòng journal_entry (voucher_no) + stock_lot, không có id →
--    không thể liên kết chặt với hóa đơn chính thức. Backfill từ phiếu cũ.
-- 3) hddt_purchase_invoice: hóa đơn mua chính thức từ cổng, liên kết 1-1 với
--    phiếu nhập qua inbound_voucher_id (FK) + số phiếu. portal_id UNIQUE =
--    chốt chặn trùng tuyệt đối khi đồng bộ nhiều lần.
-- 4) hddt_sync_day: cache theo ngày — ngày đã quét thì lần sau không gọi lại
--    cổng (hóa đơn của ngày đã qua không đổi).
-- =============================================

-- ─── 1. Danh tính mặt hàng ───
ALTER TABLE product ADD COLUMN identity_key TEXT NOT NULL DEFAULT '';
ALTER TABLE product ADD COLUMN portal_code TEXT NOT NULL DEFAULT '';
CREATE UNIQUE INDEX IF NOT EXISTS ux_product_identity
    ON product(identity_key) WHERE identity_key <> '';
CREATE INDEX IF NOT EXISTS ix_product_portal_code
    ON product(portal_code) WHERE portal_code <> '';

-- ─── 2. Đầu phiếu nhập kho ───
CREATE TABLE IF NOT EXISTS inbound_voucher (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    voucher_no     TEXT NOT NULL UNIQUE,
    posting_date   TEXT NOT NULL DEFAULT '',
    supplier_code  TEXT NOT NULL DEFAULT '',
    description    TEXT NOT NULL DEFAULT '',
    reference_no   TEXT NOT NULL DEFAULT '',
    inbound_type   TEXT NOT NULL DEFAULT 'purchase',
    warehouse_code TEXT NOT NULL DEFAULT '',
    unit_code      TEXT NOT NULL DEFAULT 'HKD',
    vat_rate       REAL NOT NULL DEFAULT 0.0,
    total          REAL NOT NULL DEFAULT 0.0,
    vat_amount     REAL NOT NULL DEFAULT 0.0,
    -- manual = nhập tay trên UI · hddt = tạo từ đồng bộ cổng · legacy = backfill
    source         TEXT NOT NULL DEFAULT 'manual',
    note           TEXT NOT NULL DEFAULT '',
    created_at     TEXT NOT NULL DEFAULT ''
);

-- Backfill: mỗi voucher_no loại 'PN' thành 1 đầu phiếu.
INSERT OR IGNORE INTO inbound_voucher
    (voucher_no, posting_date, supplier_code, description, inbound_type,
     unit_code, source, note, created_at)
SELECT je.voucher_no,
       MIN(je.posting_date),
       MAX(je.supplier_code),
       MAX(je.description),
       'purchase',
       MAX(je.unit_code),
       'legacy',
       MAX(je.note),
       datetime('now')
FROM journal_entry je
WHERE je.entry_type = 'PN'
GROUP BY je.voucher_no;

ALTER TABLE journal_entry ADD COLUMN inbound_voucher_id INTEGER REFERENCES inbound_voucher(id);
UPDATE journal_entry
SET inbound_voucher_id = (SELECT v.id FROM inbound_voucher v WHERE v.voucher_no = journal_entry.voucher_no)
WHERE entry_type = 'PN';

CREATE INDEX IF NOT EXISTS ix_journal_inbound_voucher
    ON journal_entry(inbound_voucher_id);

-- ─── 3. Hóa đơn mua chính thức từ cổng HĐĐT ───
CREATE TABLE IF NOT EXISTS hddt_purchase_invoice (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    -- id của cổng (UUID) — duy nhất, chốt chặn nhập trùng.
    portal_id         TEXT NOT NULL UNIQUE,
    -- regular | cash-register (query | sco-query)
    portal_kind       TEXT NOT NULL DEFAULT 'regular',
    tdlap             TEXT NOT NULL DEFAULT '',
    posting_date      TEXT NOT NULL DEFAULT '',
    nbmst             TEXT NOT NULL DEFAULT '',
    nbten             TEXT NOT NULL DEFAULT '',
    nmmst             TEXT NOT NULL DEFAULT '',
    khmshdon          INTEGER,
    khhdon            TEXT NOT NULL DEFAULT '',
    shdon             TEXT NOT NULL DEFAULT '',
    hthdon            INTEGER,
    tchat             INTEGER,
    tgtcthue          REAL NOT NULL DEFAULT 0,
    tgtthue           REAL NOT NULL DEFAULT 0,
    ttcktmai          REAL NOT NULL DEFAULT 0,
    tgtttbso          REAL NOT NULL DEFAULT 0,
    -- pending (chờ nhập kho) · imported (đã tạo phiếu) · manual (cần người xử lý)
    status            TEXT NOT NULL DEFAULT 'pending',
    skip_reason       TEXT NOT NULL DEFAULT '',
    line_count        INTEGER NOT NULL DEFAULT 0,
    new_product_count INTEGER NOT NULL DEFAULT 0,
    voucher_no        TEXT NOT NULL DEFAULT '',
    inbound_voucher_id INTEGER REFERENCES inbound_voucher(id),
    supplier_id       INTEGER REFERENCES supplier(id),
    -- payload gốc để đối chiếu/khi cổng đổi mức chi tiết
    raw_json          TEXT NOT NULL DEFAULT '',
    detail_json       TEXT NOT NULL DEFAULT '',
    detail_error      TEXT NOT NULL DEFAULT '',
    created_at        TEXT NOT NULL DEFAULT '',
    imported_at       TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS ix_hddt_pi_date ON hddt_purchase_invoice(posting_date);
CREATE INDEX IF NOT EXISTS ix_hddt_pi_status ON hddt_purchase_invoice(status);

-- ─── 4. Cache ngày đã quét ───
CREATE TABLE IF NOT EXISTS hddt_sync_day (
    day           TEXT NOT NULL,
    kind          TEXT NOT NULL,
    scanned_at    TEXT NOT NULL,
    invoice_count INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (day, kind)
);
