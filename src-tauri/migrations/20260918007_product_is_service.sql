-- Sản phẩm dịch vụ: không theo dõi tồn kho.
-- Khi xuất hóa đơn / phiếu xuất, dòng sản phẩm dịch vụ bỏ qua kiểm tra tồn kho
-- (nhân công, lắp đặt, sửa chữa,...) và không tạo bút toán giá vốn.
ALTER TABLE product ADD COLUMN is_service INTEGER NOT NULL DEFAULT 0;