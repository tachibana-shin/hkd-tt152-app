-- Khôi phục tiền chiết khấu cho các phiếu nhập tạo từ ĐỒNG BỘ HĐĐT trước ngày
-- 30/09/2026 — thời điểm cột `journal_entry.discount` mới ra đời (xem
-- 20260930001_journal_discount.sql).
--
-- Lúc đó `save_inbound_core` ĐÃ trừ tiền CK vào `amount` (giá vốn FIFO, doanh
-- thu, thuế đều đúng) nhưng chưa lưu lại số tiền CK → màn xem phiếu hiện
-- "Tiền CK —" dù hóa đơn có chiết khấu. Số liệu gốc còn nguyên trong
-- `hddt_imported_invoice.detail_json`, cột `stckhau` của từng dòng hàng.
--
-- Khớp dòng theo THỨ TỰ: journal được ghi theo đúng thứ tự dòng hóa đơn (bỏ
-- dòng không có tên hàng, y hệt cách import bỏ). Để không bao giờ gán nhầm tiền
-- CK sang dòng khác, đồng thời đối chiếu `amount` với lại đúng công thức mà
-- `save_inbound_core` đã dùng: `round(SL × ĐG) − stckhau`. Không khớp là bỏ
-- qua — thà thiếu còn hơn ghi sai sổ.
--
-- Chỉ đụng dòng hàng `entry_type = 'PN'` của phiếu đã link hóa đơn HĐĐT. Phiếu
-- nhập / xuất tay không còn nguồn gốc nên giữ nguyên (amount vẫn là giá trị
-- sau chiết khấu, nên số liệu kế toán không đổi).
WITH valid_invoice AS (
    -- Lọc trước để json_each không bao giờ nhận JSON hỏng / NULL.
    SELECT iv.voucher_no AS voucher_no,
           hi.detail_json AS detail_json
    FROM hddt_imported_invoice hi
    JOIN inbound_voucher iv ON iv.id = hi.inbound_voucher_id
    WHERE hi.detail_json IS NOT NULL
      AND json_valid(hi.detail_json)
),
hddt_lines AS (
    -- Thứ tự dòng hàng như cổng trả về, bỏ dòng thiếu tên (import cũng bỏ).
    SELECT vi.voucher_no AS voucher_no,
           ROW_NUMBER() OVER (
               PARTITION BY vi.voucher_no ORDER BY jl.key
           ) AS line_no,
           json_extract(jl.value, '$.sluong')                   AS sluong,
           json_extract(jl.value, '$.dgia')                     AS dgia,
           COALESCE(json_extract(jl.value, '$.stckhau'), 0.0)    AS stckhau
    FROM valid_invoice vi
    JOIN json_each(vi.detail_json, '$.hdhhdvu') jl
    WHERE COALESCE(TRIM(CAST(json_extract(jl.value, '$.ten') AS TEXT)), '') <> ''
),
journal_lines AS (
    -- Đánh số trên TOÀN BỘ dòng hàng của phiếu (kể cả dòng đã có discount)
    -- để số thứ tự không bị lệch khi migrations trước đã ghi cho một dòng.
    SELECT je.id AS id,
           je.voucher_no AS voucher_no,
           je.amount AS amount,
           ROW_NUMBER() OVER (
               PARTITION BY je.voucher_no ORDER BY je.id
           ) AS line_no
    FROM journal_entry je
    WHERE je.entry_type = 'PN'
      AND je.product_code <> ''
      AND je.quantity <> 0
      AND je.discount = 0
      AND EXISTS (
          SELECT 1
          FROM valid_invoice vi2
          WHERE vi2.voucher_no = je.voucher_no
      )
),
matched AS (
    SELECT jl.id AS id,
           hl.stckhau AS stckhau
    FROM journal_lines jl
    JOIN hddt_lines hl
      ON hl.voucher_no = jl.voucher_no
     AND hl.line_no = jl.line_no
    -- NULL (thiếu sluong/dgia) → không khớp → không update.
    WHERE ABS(jl.amount - (ROUND(hl.sluong * hl.dgia, 2) - hl.stckhau)) < 1.0
)
UPDATE journal_entry
SET discount = matched.stckhau
FROM matched
WHERE journal_entry.id = matched.id;
