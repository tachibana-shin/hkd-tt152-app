-- Khối dán không còn là TSV cố định: bên kia là ô text tự do nhận nhiều dòng,
-- mỗi dòng tách cột theo dấu phân cách người dùng chọn (Tab/phẩy/chấm phẩy/pipe)
-- và theo bố cục cột mà form máy tính tiền nhận. Đổi tên cột cho đúng ý nghĩa.
ALTER TABLE invoice_export RENAME COLUMN payload_tsv TO payload_paste;
