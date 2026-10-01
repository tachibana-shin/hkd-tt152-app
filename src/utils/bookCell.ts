/**
 * Ô của sổ kế toán TT 152/2025/TT-BTC — **một nguồn sự thật** cho mọi nơi ra
 * giấy: giao diện (`TaxBookTable`) và file Word xuất ra (`bookDoc`).
 *
 * Tách khỏi component vì trước đây logic nằm trong `<script setup>`; nếu mỗi
 * nơi một bản thì đổi cách định dạng ở màn hình (ví dụ ô `rate`) sẽ làm file
 * Word in ra kiểu khác mà không ai thấy.
 */
import type { BookCell, BookColumn } from "@/types";
import { fmtDec, fmtInt, fmtPct } from "@/utils/format";

/** Ô không có dữ liệu → `null` (in ô trống đúng mẫu, không ghi "0" vào chỗ trống). */
export function bookCellValue(row: { cells: Record<string, BookCell> }, key: string): BookCell {
  return row.cells[key] ?? null;
}

/**
 * Hiển thị một ô theo `kind` do backend khai báo.
 *
 * `rate` nhận cả hai dạng: tỉ lệ phần trăm dạng thập phân (0.01 → `1%`) và số
 * phần trăm (5 → `5%`) — tuỳ chỗ nào ghi vào.
 */
export function renderBookCell(row: { cells: Record<string, BookCell> }, col: BookColumn): string {
  const v = bookCellValue(row, col.key);
  if (v === null || v === "") return "";
  if (typeof v === "string") return v;
  switch (col.kind) {
    // Sổ kế toán VN ghi bằng đồng nguyên — backend đã làm tròn, fmtInt chỉ việc
    // định dạng theo dấu phẩy/thousand của tiếng Việt.
    case "money":
      return fmtInt(v);
    case "qty":
      return fmtDec(v);
    case "rate":
      return Math.abs(v) <= 1 ? fmtPct(v) : `${fmtInt(v)}%`;
    default:
      return fmtDec(v);
  }
}

/**
 * Cột đầu tiên mang nhãn dòng khi mẫu không có cột "Diễn giải" riêng.
 *
 * S1a (3 cột) và S3a (A, B, 1…10) chỉ có `a`/`b` cho ngày và số hiệu → không
 * có cột nhãn; S2a/S2c có cột `C · Diễn giải` nên cột đó được in đậm để các
 * dòng "Tổng cộng (n)" nổi hơn số liệu.
 */
export function bookLabelColumn(columns: BookColumn[]): string | undefined {
  return columns.find((c) => c.kind === "text" && c.key !== "a" && c.key !== "b")?.key;
}
