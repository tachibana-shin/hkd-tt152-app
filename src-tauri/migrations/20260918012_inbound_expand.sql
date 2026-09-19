-- Nhập kho mở rộng theo TT 88/2021/TT-BTC (Phiếu nhập kho — Mẫu 03-VT):
-- Phiếu nhập kho áp dụng cho mọi trường hợp hàng vào kho (mua ngoài, tự sản
-- xuất, thuê ngoài gia công, thừa phát hiện trong kiểm kê) — không chỉ mua.
-- Bổ sung tài khoản cần cho hạch toán: thuế GTGT đầu vào (133), chi phí sản
-- xuất kinh doanh dở dang (154), thành phẩm (155).
INSERT OR IGNORE INTO account (code, name, opening_debit, opening_credit) VALUES
    ('133', 'Thuế GTGT được khấu trừ', 0, 0),
    ('154', 'Chi phí sản xuất, kinh doanh dở dang', 0, 0),
    ('155', 'Thành phẩm', 0, 0);

-- Công tắc "Khấu trừ thuế GTGT đầu vào" (đồng bộ với phương pháp tính thuế):
--   '1' = được khấu trừ (hộ nộp thuế theo phương pháp lợi nhuận — xác định được chi phí)
--   '0' = không được khấu trừ (mặc định — hộ nộp thuế theo doanh thu, giá nhập gồm thuế)
INSERT OR IGNORE INTO app_setting (key, value) VALUES ('vat_deduct', '0');