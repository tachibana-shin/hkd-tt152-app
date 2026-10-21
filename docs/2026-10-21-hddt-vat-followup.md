# Ghi chú yêu cầu — HĐĐT, VAT từng dòng, sắp xếp danh sách (2026-10-21)

> Ghi tạm nguyên văn yêu cầu của user trước khi khảo sát để không quên.

## Nguyên văn

> nhớ xem lại logic auto hóa đơn đầu vào vì nó phải lưu được thuế từng hóa đơn và
> cộng tổng ũng phải bằng hóa đơn vì hóa đơn tổng tiền bao gồm cả thuế mà với cả
> thêm cái chỗ phiếu nhập nữa vì mỗi mặt hàng có thể có thuế vat khác nhau. với cả
> vấn đề mặt hàng cần hạch toán sản xuất khi lập hóa đơn nháp nữa. một vài màn hình
> có list ví dụ list phiếu nhập / xuất, hóa đơn, chờ xuất hd, nó hiển thị ASC tức
> là hóa đơn cuối ở cuối nó không thích hợp lắm phải dùng sort DESC. còn 1 cái nữa
> là nếu user nhập tay 1 phiếu nhập và có ghi mã phiếu nhập thì trình tự động import
> hóa đơn nhập phải tự tương thích với nó bằng cách cập nhật các thông tin / hàng
> hóa vào cho nó (nối cách khác là ghi đè cái hóa đơn mà user nhập chứng từ bị trùng
> <tất nhiên các thông tin ví dụ số chứng từ, ngày xuất và số hiệu phải trùng (không
> cần so người xuất vì số hiệu là đủ)). thêm hiển thị địa chỉ người mua ở cái popup
> sao chép hóa đơn nháp nữa; cái prompt này có vẻ rất dài nên hãy ghi tạm vào file
> markdown trước khi tìm hiểu để khỏi quên

## Checklist kỹ thuật

1. **VAT từng dòng hàng**
   - Auto-import HĐĐT đầu vào phải lưu thuế suất theo từng dòng hàng (`tsuat`),
     không gộp thành một mức chung rồi áp lại.
   - Tổng tiền phiếu nhập phải khớp tổng tiền hóa đơn (hóa đơn gồm cả thuế):
     `Σ(số lượng × đơn giá − chiết khấu) + Σ thuế = tổng thanh toán hóa đơn`.
   - Phiếu nhập tay cũng phải cho mỗi mặt hàng một mức VAT khác nhau (cột/mức
     thuế theo dòng), không chỉ một ô % chung cho cả phiếu.
   - Kiểm tra cả trường hợp hộ khấu trừ (tách 133) và không khấu trừ (cộng vào
     giá vốn), bảo đảm tổng công nợ/giá vốn luôn bằng tổng hóa đơn.

2. **Hàng cần hạch toán sản xuất khi lập hóa đơn nháp**
   - Xem lại luồng lập hóa đơn nháp với hàng chế tạo (có định mức): hiện prompt
     gợi ý tạo lô; cần xác định/yêu cầu hạch toán sản xuất đúng lúc, đúng chỗ.

3. **Sắp xếp danh sách DESC**
   - Các màn hình danh sách: phiếu nhập, phiếu xuất, hóa đơn, chờ xuất HĐĐT…
   - Hiện đang ASC (bản ghi mới nhất nằm cuối) → đổi mặc định sang DESC
     (mới nhất lên đầu), vẫn giữ khả năng đổi sort theo cột.

4. **Auto-import khớp phiếu nhập tay**
   - Nếu user đã nhập tay phiếu nhập có số chứng từ trùng với hóa đơn tự động
     import (trùng số chứng từ + ngày xuất + số hiệu/ký hiệu; không cần so người
     xuất), thì không tạo phiếu mới.
   - Cập nhật/ghi đè thông tin + hàng hóa của hóa đơn vào phiếu nhập đã có
     (theo hướng an toàn: không nhân đôi tồn kho, không mất liên kết HĐĐT).

5. **Địa chỉ người mua trong popup sao chép hóa đơn nháp**
   - Bổ sung địa chỉ người mua vào nội dung popup "sao chép hóa đơn nháp".

6. **Trạng thái**
   - Commit trước: `dc44d2e fix(inbound): calculate input VAT and widen product dropdown`
     (đã push main, chờ CI/release).
   - Ghi chú này tạo trước khi khảo sát; cập nhật kết quả bên dưới.

## Kết quả triển khai (cập nhật 21/10)

### Commit `dc44d2e fix(inbound): calculate input VAT and widen product dropdown`

- ProductSelect: sàn `30rem` cho panel — tên hàng dài không còn bị cắt (regression
  do virtual scroller làm panel co đúng bằng cột hẹp).
- Phiếu nhập: luôn hiện ô "Thuế GTGT đầu vào (%)" cho mua hàng ngoài; hộ không
  khấu trừ được cộng thuế vào giá nhập kho / giá vốn (không ghi 133).
- Auto-import HĐĐT: lấy thuế SUẤT từng dòng (`tsuat`) thay vì cứng 0%.
- Prompt thiếu tồn kho hàng chế tạo nói rõ: chỉ mở form tạo lô, lô tạo khi bấm
  Lưu, xoá hóa đơn nháp không xoá lô (lô là chứng từ kho độc lập).

### Phần sau (những thay đổi đang chờ commit)

1. **VAT từng dòng — backend**
   - `InboundItemInput` có `vat_rate` (tỷ lệ riêng dòng) + `vat_amount` (số thuế
     tuyệt đối dùng cho import để tổng khớp hóa đơn); dòng không gửi → dùng mức
     chung đầu phiếu.
   - `journal_entry.vat_rate` ghi % thuế từng dòng; đầu phiếu lưu tỷ lệ bình
     quân gia quyền (chỉ để hiển thị).
   - Import phân bổ: lấy `tgtttbso − tgtthue` làm tổng giá trị hàng (sau CK,
     chưa thuế) của hóa đơn; phần lệch (chiết khấu cấp hóa đơn / làm tròn) dàn
     xuống dòng theo trọng số; số thuế phân bổ theo `net × tsuat` từng dòng
     rồi dồn chênh lệch → Σ 133 khớp `tgtthue` tuyệt đối. Metadata sqlx đã
     prepare lại.
2. **VAT từng dòng — frontend**
   - `LineItemsEditor`: cột "VAT %" (`showVat`) — trống = dùng mức chung đầu phiếu.
   - `Inbound.vue`: tổng thuế cộng theo dòng (làm tròn từng dòng khớp backend),
     label bảng đổi thành "Tiền hàng (chưa thuế)" khi thuế > 0 và hộ không
     khấu trừ.
   - `VoucherViewDialog`: cột GTGT hiện % từng dòng (chỉ hiện khi tồn dòng có %).
3. **Auto-import khớp phiếu nhập tay** (migration `20261021001_inbound_invoice_match`)
   - `inbound_voucher` thêm `invoice_serial` / `invoice_no` / `invoice_date`;
     `stock_lot` thêm `inbound_voucher_id` (neo lô về phiếu nhập).
   - `save_inbound` nhận ký hiệu + số + ngày hóa đơn; chặn 2 phiếu nhập cùng một
     hóa đơn (trùng ký hiệu + số + ngày — không so MST người bán).
   - Đồng bộ: `find_matching_inbound` tìm phiếu tay khớp (trực tiếp qua 3 cột
     trên, hoặc dự phòng `reference_no` chuẩn hoá + ngày); lỗi nếu >1 phiếu khớp.
   - Ghi đè trong CÙNG transaction: `clear_inbound_for_overwrite` chỉ xoá khi
     toàn bộ lô còn nguyên (chưa xuất/điều chỉnh); lô đã dùng → từ chối, giữ
     nguyên phiếu tay + rollback. Phiếu chi "trả tiền ngay" cũ được cập nhật số
     tiền theo tổng mới. Giữ nguyên số phiếu nội bộ user.
   - FE: 2 ô "Ký hiệu hóa đơn" + "Số hóa đơn" (chỉ loại nhập Mua hàng ngoài);
     ngày hóa đơn = ngày nhập.
4. **Sắp xếp danh sách** — đã DESC từ trước ở Phiếu nhập/xuất (`stock.rs`) và
   Hóa đơn (`invoice.rs`); lần này đổi nốt **Chờ xuất HĐĐT**
   (`invoice_queue_page`): `date ASC, id ASC` → `date DESC, id DESC`.
   Sổ nhật ký kế toán (accounting.rs) giữ ASC có chủ ý — đó là sổ ghi theo
   trình tự thời gian giao dịch.
5. **Địa chỉ người mua ở popup sao chép** — đã xong từ commit `19b7a78`
   (popup "Chép sang hóa đơn khác" trong InvoiceExportDialog hiển thị "Địa chỉ
   người mua"; tra theo MST trước, không có thì theo tên, thiếu → "—").

### Kiểm thử

- `bun run test:rust` — 263 pass / 0 fail (thêm test: thuế trộn nhiều mức khớp
  tổng hóa đơn, chiết khấu cấp hóa đơn dàn xuống dòng, ghi đè phiếu tay, từ
  chối ghi đè khi lô đã xuất, chặn 2 phiếu cùng hóa đơn).
- `bun run typecheck` / `bun run lint` xanh; e2e thêm test "Nhập kho: thuế GTGT
  từng dòng khác nhau và chặn lưu 2 phiếu cùng hóa đơn".
