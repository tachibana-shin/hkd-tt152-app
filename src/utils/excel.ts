import { Workbook } from "exceljs";
import type { TaxBook } from "@/types";
import { downloadBinary } from "@/utils/download";

export interface XlsxColumn {
  header: string;
  key: string;
}

/**
 * Cỡ chữ gốc khi xuất Excel: 14pt cho mọi ô, cùng cỡ với file Word `.docx`
 * (bộ xuất file dùng chung một cỡ chữ — in A4 đọc rõ thay vì cỡ 11pt mặc
 * định của Excel). Phông Times New Roman như biểu mẫu thuế và như file Word.
 *
 * Dùng `exceljs` chứ không phải SheetJS (đang giữ cho phần *đọc* file import):
 * bản Community của SheetJS không ghi được style nên không đặt nổi cỡ chữ.
 */
const FONT = { name: "Times New Roman", size: 14 };

/** Tên MIME của `.xlsx` để trình duyệt tải về đúng kiểu. */
const XLSX_MIME = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";

/**
 * Dựng nội dung file `.xlsx` — phần thuần dữ liệu, không đụng DOM (dễ test).
 */
export async function buildXlsxBlob(
  columns: XlsxColumn[],
  rows: Record<string, unknown>[],
): Promise<Blob> {
  const wb = new Workbook();
  const ws = wb.addWorksheet("Data");
  ws.addRow(columns.map((c) => c.header));
  for (const r of rows) ws.addRow(columns.map((c) => r[c.key] ?? ""));

  // Cỡ chữ 14 áp cho toàn bộ vùng dữ liệu — header lẫn thân bảng.
  ws.eachRow((row) =>
    row.eachCell((cell) => {
      cell.font = { ...FONT };
    }),
  );

  const buffer = await wb.xlsx.writeBuffer();
  return new Blob([buffer as ArrayBuffer], { type: XLSX_MIME });
}

export async function exportXlsx(
  filename: string,
  columns: XlsxColumn[],
  rows: Record<string, unknown>[],
): Promise<boolean> {
  const blob = await buildXlsxBlob(columns, rows);
  // Một đường lui duy nhất cho desktop (hộp thoại Lưu file) lẫn web (tải về).
  return downloadBinary(`${filename}.xlsx`, new Uint8Array(await blob.arrayBuffer()), XLSX_MIME);
}

/**
 * Dựng file Excel của **MỘT sổ kế toán** theo đúng bộ cột của mẫu (không tải
 * về — trả Blob để gộp vào gói "Xuất báo cáo kỳ thuế").
 *
 * Bộ cột mỗi mẫu một bộ (S1a 3 cột, S2d 12 cột…) nên lấy nguyên `columns`
 * backend trả về; ô trống xuất rỗng.
 */
export async function buildTaxBookXlsx(b: TaxBook): Promise<Blob> {
  const cols: XlsxColumn[] = b.columns.map((c) => ({ header: c.label, key: c.key }));
  const rows = b.rows.map((r) => {
    const out: Record<string, unknown> = {};
    for (const c of b.columns) out[c.key] = r.cells[c.key] ?? "";
    return out;
  });
  return buildXlsxBlob(cols, rows);
}
