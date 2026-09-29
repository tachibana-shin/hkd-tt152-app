-- Tờ khai thuế HKD 2026: nhóm ngành còn thiếu + cờ "không được trừ khi tính thuế TNCN".
--
-- 1) Bổ sung nhóm danh mục ngành nghề còn thiếu trong bảng tỷ lệ thuế:
--      • "Hoạt động kinh doanh khác"           → GTGT 2%  (Luật GTGT 48/2024 Đ12.2b4)
--                                              → TNCN 1%  (Luật TNCN 109/2025 Đ7.3e)
--      • Cho thuê tài sản, đại lý bảo hiểm, đại lý xổ số, bán hàng đa cấp
--                                            → GTGT 5% (dịch vụ) / TNCN 5% (Đ7.3c)
--      • Sản phẩm, dịch vụ nội dung số giải trí, quảng cáo số
--                                            → GTGT 5% (dịch vụ) / TNCN 5% (Đ7.3đ)
--    Ba nhóm có sẵn (PPHH 1%/0,5% · SX-CN, DVXD-KL, VCTT 3%/1,5% · DVXD-KNL 5%/2%)
--    đã khớp Luật GTGT Đ12.2b1-b3 và Luật TNCN Đ7.3b-đ.
--
-- 2) journal_entry: 2 cột để thực hiện Điều 6 Nghị định 68/2026 — khoản chi được
--    trừ khi xác định thu nhập tính thuế phải là chi thực tế liên quan kinh doanh,
--    có đủ hóa đơn/chứng từ; các khoản chi lương chủ hộ, chi cá nhân - gia đình,
--    chi không có chứng từ... thì KHÔNG được trừ.
--    Mặc định deductible = 1 nên mọi phiếu cũ giữ nguyên cách tính.

ALTER TABLE journal_entry ADD COLUMN deductible INTEGER NOT NULL DEFAULT 1;
ALTER TABLE journal_entry ADD COLUMN non_deductible_reason TEXT NOT NULL DEFAULT '';

INSERT OR IGNORE INTO industry_group (code, name, vat_rate, pit_rate) VALUES
    ('KD-KHAC',  'Hoạt động kinh doanh khác',                                            0.02, 0.01),
    ('DV-TS',    'Cho thuê tài sản, đại lý bảo hiểm, xổ số, bán hàng đa cấp',            0.05, 0.05),
    ('NDS-SO',   'Sản phẩm, dịch vụ nội dung số giải trí, quảng cáo số',              0.05, 0.05);
