-- =============================================
-- 20260926001 — Quy trình xuất hóa đơn ra bên dịch vụ HĐĐT khác
--
-- Hộ không có API để phát hành HĐĐT (dùng 1 bên dịch vụ khác) nên phải chép
-- tay. Ba thứ cần cho việc đó:
--
--   1) Vòng đời hóa đơn đầy đủ. `status` nay dùng: draft | exported | official
--      | cancelled | adjusted ('pasted' cũ → 'exported' = đã chép sang bên kia,
--      đang chờ phát hành). Thêm cột ghi lý do / HĐ thay thế / phiếu điều chỉnh
--      để hủy hay sửa đều có dấu vết.
--   2) `invoice_event` — lịch sử chuyển trạng thái, dùng để đối chiếu lại khi
--      bên kia có bổ sung báo cáo/xuất CSV (khoá đối chiếu: ngày + MST + tổng).
--   3) `invoice_export` — bản chốt (snapshot) ngay lúc bấm "Chép để xuất": lưu
--      đúng nội dung đã chép. Hóa đơn bị sửa sau đó sẽ lệch với bản chốt → app
--      cảnh báo để người dùng biết phải sửa bên kia.
-- =============================================

ALTER TABLE invoice ADD COLUMN exported_at TEXT NOT NULL DEFAULT '';
ALTER TABLE invoice ADD COLUMN cancel_reason TEXT NOT NULL DEFAULT '';
ALTER TABLE invoice ADD COLUMN adjust_reason TEXT NOT NULL DEFAULT '';
-- Số HĐĐT liên quan khi bị hủy/sửa (HĐ thay thế bên kia), vd "1C26TT152/00000123".
ALTER TABLE invoice ADD COLUMN ref_invoice TEXT NOT NULL DEFAULT '';
-- Phiếu xuất điều chỉnh đã lập để hủy hóa đơn đã phát hành (bắt buộc khi hủy HĐ chính thức).
ALTER TABLE invoice ADD COLUMN adjust_voucher_no TEXT NOT NULL DEFAULT '';

UPDATE invoice SET status = 'exported' WHERE status = 'pasted';

CREATE TABLE IF NOT EXISTS invoice_event (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    invoice_id  INTEGER NOT NULL REFERENCES invoice(id),
    at          TEXT NOT NULL DEFAULT (datetime('now')),
    from_status TEXT NOT NULL DEFAULT '',
    to_status   TEXT NOT NULL,
    reason      TEXT NOT NULL DEFAULT '',
    actor       TEXT NOT NULL DEFAULT '',
    ref         TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS ix_invoice_event_invoice ON invoice_event(invoice_id, id);

CREATE TABLE IF NOT EXISTS invoice_export (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    invoice_id   INTEGER NOT NULL REFERENCES invoice(id),
    created_at   TEXT NOT NULL DEFAULT (datetime('now')),
    total        REAL NOT NULL DEFAULT 0.0,
    line_count   INTEGER NOT NULL DEFAULT 0,
    payload_json TEXT NOT NULL DEFAULT '{}',
    payload_text TEXT NOT NULL DEFAULT '',
    payload_tsv  TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS ix_invoice_export_invoice ON invoice_export(invoice_id, id);

-- Ánh xạ mặt hàng của app sang danh mục ở bên dịch vụ HĐĐT khác: dịch vụ đó có
-- danh mục riêng, mỗi hàng bán ra phải khớp 1 mã ở đó. Khớp 1 lần, xong dùng
-- cho mọi hóa đơn (tránh xuất mặt hàng bằng tay từng hóa đơn).
CREATE TABLE IF NOT EXISTS product_remote (
    product_id  INTEGER PRIMARY KEY REFERENCES product(id),
    remote_code TEXT NOT NULL DEFAULT '',
    remote_name TEXT NOT NULL DEFAULT '',
    remote_unit TEXT NOT NULL DEFAULT '',
    note        TEXT NOT NULL DEFAULT '',
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS ix_product_remote_code ON product_remote(remote_code);
