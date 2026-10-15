-- =============================================
-- 20261015002 — Mọi mặt hàng hàng hóa chưa có nhóm ngành → PPHH
--
-- Yêu cầu (#4): gặp mặt hàng nào chưa có giá trị cột nhóm ngành (dù tạo từ
-- đâu — nhập tay, import Excel, đồng bộ HĐĐT) thì mặc định gán "Phân phối,
-- cung cấp hàng hóa". Migration 20260925003 chỉ đụng mặt hàng đồng bộ từ cổng
-- (portal_code khác rỗng) — lần này gán cho MỌI hàng hóa còn trống.
--
-- Không đụng dịch vụ (is_service = 1: tỷ lệ thuế theo ngành thực tế) và chỉ
-- chạy khi danh mục còn nhóm PPHH (không gán mã không tồn tại).
-- =============================================

UPDATE product
   SET industry_code = 'PPHH'
 WHERE industry_code = ''
   AND is_service = 0
   AND EXISTS (SELECT 1 FROM industry_group WHERE code = 'PPHH');
