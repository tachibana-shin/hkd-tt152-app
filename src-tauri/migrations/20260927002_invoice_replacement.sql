-- ── Thay thế hóa đơn thay cho "hủy" (Thông tư 91/2026/TT-BTC Điều 10) ──
--
-- Từ 01/07/2026 người bán KHÔNG được tự ý hủy hóa đơn điện tử đã lập; sai sót
-- thì xử lý bằng thông báo / hóa đơn điều chỉnh / hóa đơn thay thế. Vì vậy:
--
--   * `cancelled` (đã hủy) → `replaced` (đã bị thay thế bởi HĐĐT khác)
--   * `cancel_reason`     → `replace_reason` (lý do phải thay thế)
--   * thêm `replaces_invoice_id` trỏ về hóa đơn thay thế mới
--
-- Hóa đơn CHƯA phát hành (bản nháp) không đổi: chưa phát hành thì không phải
-- hóa đơn, cứ sửa hoặc xoá như bình thường.
ALTER TABLE invoice RENAME COLUMN cancel_reason TO replace_reason;

ALTER TABLE invoice ADD COLUMN replaces_invoice_id INTEGER REFERENCES invoice(id);

UPDATE invoice SET status = 'replaced' WHERE status = 'cancelled';

CREATE INDEX IF NOT EXISTS ix_invoice_replaces ON invoice(replaces_invoice_id);
