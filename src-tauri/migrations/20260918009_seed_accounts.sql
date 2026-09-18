-- Bộ tài khoản kế toán chuẩn cho hộ kinh doanh (DMTK) — dùng cho:
-- - Chọn tài khoản Nợ/Có ở màn Thu/Chi (yêu cầu mã có trong danh mục).
-- - Báo cáo "Bảng cân đối số phát sinh" (số dư đầu kỳ + phát sinh trong kỳ).
-- INSERT OR IGNORE: không ghi đè nếu người dùng đã thêm/sửa tài khoản.
INSERT OR IGNORE INTO account (code, name, opening_debit, opening_credit) VALUES
    ('111', 'Tiền mặt', 0, 0),
    ('112', 'Tiền gửi ngân hàng', 0, 0),
    ('131', 'Phải thu của khách hàng', 0, 0),
    ('141', 'Tạm ứng', 0, 0),
    ('152', 'Hàng hóa, hoá đơn, chứng từ mua vào', 0, 0),
    ('153', 'Công cụ, dụng cụ', 0, 0),
    ('211', 'TSCĐ hữu hình', 0, 0),
    ('214', 'Hao mòn TSCĐ', 0, 0),
    ('331', 'Phải trả cho người bán', 0, 0),
    ('334', 'Phải trả người lao động', 0, 0),
    ('411', 'Vốn đầu tư của chủ sở hữu', 0, 0),
    ('511', 'Doanh thu bán hàng và cung cấp dịch vụ', 0, 0),
    ('515', 'Doanh thu tài chính', 0, 0),
    ('632', 'Giá vốn hàng bán', 0, 0),
    ('635', 'Chi phí tài chính', 0, 0),
    ('642', 'Chi phí quản lý kinh doanh', 0, 0),
    ('911', 'Xác định kết quả kinh doanh', 0, 0);