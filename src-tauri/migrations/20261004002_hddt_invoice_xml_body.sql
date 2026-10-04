-- =============================================
-- 20261004002: đưa nội dung file XML vào CSDL
--
-- Ban đầu app chỉ ghi file XML ra `profiles/<key>/hddt_xml/` và ghi trong bảng
-- `hddt_invoice_xml` mỗi đường dẫn + cỡ file. Nhưng hoá đơn phải được *dùng* lại
-- (nộp/gửi ZIP cho kế toán) nên nội dung phải nằm trong SQLite: đọc được mọi
-- lúc không phụ thuộc file còn trên đĩa, join/query được, backup đi theo DB.
--
-- `xml_body` là nguồn sự thật. File `.xml` vẫn được ghi song song như bản sao
-- để mở/tải ra tay; nếu file bị xoá thì chỉ ghi lại từ blob, không tải lại cổng.
-- Dòng cũ (trước migration này) có `xml_body IS NULL` sẽ được nạp lại từ file
-- ngay lúc đọc lần đầu (`existing()` trong hddt/xml.rs) — không cần quét cổng.
-- =============================================

ALTER TABLE hddt_invoice_xml ADD COLUMN xml_body BLOB;
