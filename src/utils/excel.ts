import { Workbook } from "exceljs";

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
): Promise<void> {
  const blob = await buildXlsxBlob(columns, rows);
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = `${filename}.xlsx`;
  document.body.appendChild(a);
  a.click();
  a.remove();
  // Gỡ link trễ — revoke sớm là trình duyệt huỷ blob khi đang tải dở.
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
}
