-- Bên dịch vụ phát hành (máy tính tiền) KHÔNG có danh mục hàng: mỗi lần xuất
-- hóa đơn phải nhập lại từng dòng. Vậy bảng ánh xạ sang danh mục bên kia
-- (`product_remote`, tạo ở 20260926001) không dùng được và chỉ gây hiểu nhầm —
-- bỏ đi. Mọi thứ cần dán sang bên kia đã có sẵn trong bảng `product` (tên, đơn
-- vị) và `invoice_item` (số lượng, giá, nhóm ngành → thuế suất).
--
-- Migration riêng (không sửa 20260926001) vì sqlx kiểm tra checksum các
-- migration đã chạy — sửa file cũ sẽ làm app không khởi động được.
DROP TABLE IF EXISTS product_remote;
