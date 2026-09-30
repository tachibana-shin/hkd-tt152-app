-- Lưu số tiền chiết khấu thương mại trên từng dòng phiếu nhập / phiếu xuất.
--
-- Trước đây `amount` đã là "SL × đơn giá − chiết khấu" nên số tiền CK chỉ được
-- dùng để tính rồi bị mất dấu: xem lại phiếu không biết phần chiết khấu là bao
-- nhiêu, trong khi thông tư yêu cầu hóa đơn có mục chiết khấu thì phải ghi nhận.
-- Cột này KHÔNG đổi cách hạch toán: `amount` vẫn là giá trị sau chiết khấu, nên
-- giá vốn FIFO, doanh thu và thuế đều không đổi.
ALTER TABLE journal_entry ADD COLUMN discount REAL NOT NULL DEFAULT 0.0;
