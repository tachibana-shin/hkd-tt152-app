-- =============================================
-- 20260925003 — Mặt hàng đồng bộ từ hóa đơn mặc định nhóm ngành PPHH
--
-- Mặt hàng hàng hóa do Đồng bộ HĐĐT tạo ra trước đây để trống `industry_code`
-- → khi xuất bán, màn Xuất kho báo "chưa có nhóm ngành" và bắt gán tay.
-- Hàng hóa mua vào bằng hóa đơn luôn thuộc nhóm "PPHH" (Phân phối, cung cấp
-- hàng hóa — nhóm có sẵn từ migration khởi tạo) nên gán luôn ở đây.
--
-- Chỉ đụng mặt hàng do đồng bộ tạo (`portal_code` khác rỗng — mã hàng của cổng),
-- bỏ qua dịch vụ (`is_service` = 1: tỷ lệ thuế theo ngành thực tế) và bỏ qua
-- mặt hàng người dùng đã tự gán nhóm khác.
-- =============================================

UPDATE product
   SET industry_code = 'PPHH'
 WHERE industry_code = ''
   AND is_service = 0
   AND portal_code <> ''
   AND EXISTS (SELECT 1 FROM industry_group WHERE code = 'PPHH');
