import * as XLSX from "xlsx";

export interface XlsxColumn { header: string; key: string; }

export function exportXlsx(filename: string, columns: XlsxColumn[], rows: Record<string, unknown>[]) {
  const header = columns.map((c) => c.header);
  const body = rows.map((r) => columns.map((c) => r[c.key] ?? ""));
  const ws = XLSX.utils.aoa_to_sheet([header, ...body]);
  const wb = XLSX.utils.book_new();
  XLSX.utils.book_append_sheet(wb, ws, "Data");
  XLSX.writeFile(wb, `${filename}.xlsx`);
}