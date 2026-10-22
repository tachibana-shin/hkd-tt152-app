import { Workbook } from "exceljs";

/**
 * Dựng file Excel cho **một bảng định mức vật tư** — dùng để in/gửi khách làm
 * hợp đồng. Đơn giá lấy theo đơn giá đã khai trên từng dòng định mức; thành
 * tiền = số lượng × đơn giá, tổng cộng ở dòng cuối.
 *
 * Phần dựng file tách khỏi view để không đụng DOM (dễ soi bằng test E2E: tải
 * file → mở lại bằng SheetJS).
 */

export interface BomExcelLine {
  /** Mã hàng hóa/dịch vụ. */
  code: string;
  name: string;
  unit: string;
  quantity: number;
  /** Đơn giá đã khai cho 1 đơn vị (VND). */
  price: number;
}

export interface BomExcelData {
  productCode: string;
  productName: string;
  lines: BomExcelLine[];
  /** Ngày xuất file; bỏ trống = hôm nay. */
  exportedAt?: Date;
}

/** Cỡ chữ 14pt / Times New Roman — cùng bộ với Excel báo cáo thuế của app. */
const FONT = { name: "Times New Roman", size: 14 } as const;
const MONEY_FORMAT = "#,##0";
const QUANTITY_FORMAT = "#,##0.###";
const XLSX_MIME = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";
const BORDER = {
  top: { style: "thin" as const, color: { argb: "FFB7B7B7" } },
  left: { style: "thin" as const, color: { argb: "FFB7B7B7" } },
  bottom: { style: "thin" as const, color: { argb: "FFB7B7B7" } },
  right: { style: "thin" as const, color: { argb: "FFB7B7B7" } },
};

function pad2(n: number): string {
  return String(n).padStart(2, "0");
}

function dateText(d: Date): string {
  return `${pad2(d.getDate())}/${pad2(d.getMonth() + 1)}/${d.getFullYear()}`;
}

export async function buildBomXlsx(data: BomExcelData): Promise<Blob> {
  const wb = new Workbook();
  const ws = wb.addWorksheet("Định mức");

  // Độ rộng đủ đọc trên A4 dọc; tên hàng rộng nhất để không phải kéo cột.
  ws.columns = [
    { width: 6 }, // STT
    { width: 16 }, // Mã hàng
    { width: 42 }, // Tên
    { width: 10 }, // ĐVT
    { width: 12 }, // Số lượng
    { width: 16 }, // Đơn giá
    { width: 18 }, // Thành tiền
  ];

  const title = ws.addRow(["BẢNG ĐỊNH MỨC VẬT TƯ / DỊCH VỤ"]);
  ws.mergeCells(title.number, 1, title.number, 7);
  title.height = 28;
  title.getCell(1).font = { ...FONT, size: 16, bold: true };
  title.getCell(1).alignment = { horizontal: "center", vertical: "middle" };

  const subtitle = ws.addRow([`Thành phẩm: ${data.productCode} — ${data.productName}`]);
  ws.mergeCells(subtitle.number, 1, subtitle.number, 7);
  subtitle.getCell(1).font = { ...FONT, bold: true };
  subtitle.getCell(1).alignment = { horizontal: "left", vertical: "middle" };

  const meta = ws.addRow([`Ngày xuất: ${dateText(data.exportedAt ?? new Date())}`]);
  ws.mergeCells(meta.number, 1, meta.number, 7);
  meta.getCell(1).font = { ...FONT, italic: true };
  meta.getCell(1).alignment = { horizontal: "left", vertical: "middle" };

  ws.addRow([]);

  const header = ws.addRow([
    "STT",
    "Mã hàng",
    "Tên vật tư / dịch vụ",
    "ĐVT",
    "Số lượng",
    "Đơn giá",
    "Thành tiền",
  ]);
  header.height = 24;
  header.eachCell({ includeEmpty: true }, (cell) => {
    cell.font = { ...FONT, bold: true };
    cell.fill = { type: "pattern", pattern: "solid", fgColor: { argb: "FFE9EDF2" } };
    cell.alignment = { horizontal: "center", vertical: "middle" };
    cell.border = BORDER;
  });

  let total = 0;
  for (const [index, line] of data.lines.entries()) {
    const quantity = Number.isFinite(line.quantity) ? line.quantity : 0;
    const price = Number.isFinite(line.price) ? line.price : 0;
    const amount = quantity * price;
    total += amount;

    const row = ws.addRow([index + 1, line.code, line.name, line.unit, quantity, price, amount]);
    row.height = 20;
    row.eachCell({ includeEmpty: true }, (cell) => {
      cell.font = { ...FONT };
      cell.border = BORDER;
    });
    row.getCell(1).alignment = { horizontal: "center", vertical: "middle" };
    row.getCell(4).alignment = { horizontal: "center", vertical: "middle" };
    row.getCell(5).numFmt = QUANTITY_FORMAT;
    row.getCell(5).alignment = { horizontal: "right", vertical: "middle" };
    row.getCell(6).numFmt = MONEY_FORMAT;
    row.getCell(6).alignment = { horizontal: "right", vertical: "middle" };
    row.getCell(7).numFmt = MONEY_FORMAT;
    row.getCell(7).alignment = { horizontal: "right", vertical: "middle" };
  }

  const totalRow = ws.addRow(["", "", "TỔNG CỘNG", "", "", "", total]);
  totalRow.height = 22;
  totalRow.eachCell({ includeEmpty: true }, (cell) => {
    cell.font = { ...FONT, bold: true };
    cell.border = BORDER;
  });
  ws.mergeCells(totalRow.number, 3, totalRow.number, 6);
  totalRow.getCell(3).alignment = { horizontal: "right", vertical: "middle" };
  totalRow.getCell(7).numFmt = MONEY_FORMAT;
  totalRow.getCell(7).alignment = { horizontal: "right", vertical: "middle" };

  // Cố định 5 dòng đầu (tiêu đề + cột) để cuộn danh sách dài vẫn thấy tên cột.
  ws.views = [{ state: "frozen", ySplit: header.number }];
  ws.pageSetup = {
    paperSize: 9, // A4
    orientation: "portrait",
    fitToPage: true,
    fitToWidth: 1,
    fitToHeight: 0,
    horizontalCentered: true,
  };

  const buffer = await wb.xlsx.writeBuffer();
  return new Blob([buffer as ArrayBuffer], { type: XLSX_MIME });
}
