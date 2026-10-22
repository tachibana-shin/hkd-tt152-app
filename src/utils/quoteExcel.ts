import { Workbook } from "exceljs";

/**
 * Dựng file Excel **báo giá** từ các dòng đang nhập trong popup hóa đơn nháp.
 *
 * Dùng đúng số liệu đang thấy trên form (kể cả sửa đổi chưa lưu): đơn giá,
 * chiết khấu, thành tiền từng dòng và tổng tiền khách phải trả. Nhờ vậy một
 * gói sửa chữa có cả công (dịch vụ) lẫn vật tư vẫn ra bảng báo giá gửi khách
 * ngay, không phải chờ lập hóa đơn xong.
 */

export interface QuoteExcelLine {
  /** Mã hàng hóa/dịch vụ. */
  code: string;
  name: string;
  unit: string;
  quantity: number;
  /** Đơn giá khách trả cho 1 đơn vị (VND). */
  price: number;
  /** Chiết khấu thương mại của dòng (VND). */
  discount?: number;
}

export interface QuoteExcelData {
  /** Số chứng từ đang lập trên form (dùng làm số báo giá tham chiếu). */
  quoteNumber?: string;
  quoteDate?: Date;
  seller?: {
    name?: string;
    taxCode?: string;
    address?: string;
    phone?: string;
  };
  customer?: string;
  customerTaxCode?: string;
  lines: QuoteExcelLine[];
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

function num(n: number | undefined): number {
  return typeof n === "number" && Number.isFinite(n) ? n : 0;
}

export async function buildQuoteXlsx(data: QuoteExcelData): Promise<Blob> {
  const wb = new Workbook();
  const ws = wb.addWorksheet("Báo giá");
  const lastCol = 8;

  // Độ rộng đủ đọc trên A4 dọc; tên hàng rộng nhất để không phải kéo cột.
  ws.columns = [
    { width: 6 }, // STT
    { width: 16 }, // Mã hàng
    { width: 42 }, // Tên
    { width: 10 }, // ĐVT
    { width: 12 }, // Số lượng
    { width: 16 }, // Đơn giá
    { width: 14 }, // Chiết khấu
    { width: 18 }, // Thành tiền
  ];

  /** Dòng chữ gộp qua toàn bộ bảng — dùng cho đầu báo giá và các dòng tổng. */
  function addMerged(
    value: string,
    opts: { bold?: boolean; italic?: boolean; size?: number; align?: "left" | "center" } = {},
  ) {
    const row = ws.addRow([value]);
    ws.mergeCells(row.number, 1, row.number, lastCol);
    row.getCell(1).font = {
      ...FONT,
      bold: opts.bold ?? false,
      italic: opts.italic ?? false,
      size: opts.size ?? FONT.size,
    };
    row.getCell(1).alignment = {
      horizontal: opts.align ?? "left",
      vertical: "middle",
      wrapText: true,
    };
    return row;
  }

  const seller = data.seller;
  if (seller?.name?.trim()) {
    const row = addMerged(seller.name.trim(), { bold: true, size: 16, align: "center" });
    row.height = 24;
  }
  if (seller?.address?.trim()) addMerged(`Địa chỉ: ${seller.address.trim()}`);
  const sellerMeta = [
    seller?.taxCode?.trim() ? `MST: ${seller.taxCode.trim()}` : "",
    seller?.phone?.trim() ? `Điện thoại: ${seller.phone.trim()}` : "",
  ]
    .filter(Boolean)
    .join("  •  ");
  if (sellerMeta) addMerged(sellerMeta);
  if (seller?.name?.trim() || seller?.address?.trim() || sellerMeta) ws.addRow([]);

  const title = addMerged("BÁO GIÁ", { bold: true, size: 18, align: "center" });
  title.height = 30;

  addMerged(`Kính gửi: ${data.customer?.trim() || "—"}`, { bold: true });
  if (data.customerTaxCode?.trim()) addMerged(`MST khách hàng: ${data.customerTaxCode.trim()}`);
  const quoteDate = data.quoteDate ?? new Date();
  addMerged(
    [
      data.quoteNumber?.trim() ? `Số báo giá: ${data.quoteNumber.trim()}` : "",
      `Ngày báo giá: ${dateText(quoteDate)}`,
    ]
      .filter(Boolean)
      .join("  •  "),
    { italic: true },
  );
  ws.addRow([]);

  const header = ws.addRow([
    "STT",
    "Mã hàng",
    "Tên hàng hóa / dịch vụ",
    "ĐVT",
    "Số lượng",
    "Đơn giá",
    "Chiết khấu",
    "Thành tiền",
  ]);
  header.height = 24;
  header.eachCell({ includeEmpty: true }, (cell) => {
    cell.font = { ...FONT, bold: true };
    cell.fill = { type: "pattern", pattern: "solid", fgColor: { argb: "FFE9EDF2" } };
    cell.alignment = { horizontal: "center", vertical: "middle", wrapText: true };
    cell.border = BORDER;
  });

  let subtotal = 0;
  let totalDiscount = 0;
  for (const [index, line] of data.lines.entries()) {
    const quantity = num(line.quantity);
    const price = num(line.price);
    const discount = num(line.discount);
    const amount = quantity * price - discount;
    subtotal += quantity * price;
    totalDiscount += discount;

    const row = ws.addRow([
      index + 1,
      line.code,
      line.name,
      line.unit,
      quantity,
      price,
      discount,
      amount,
    ]);
    row.height = 20;
    row.eachCell({ includeEmpty: true }, (cell) => {
      cell.font = { ...FONT };
      cell.border = BORDER;
    });
    row.getCell(1).alignment = { horizontal: "center", vertical: "middle" };
    row.getCell(4).alignment = { horizontal: "center", vertical: "middle" };
    row.getCell(5).numFmt = QUANTITY_FORMAT;
    row.getCell(5).alignment = { horizontal: "right", vertical: "middle" };
    for (const col of [6, 7, 8]) {
      row.getCell(col).numFmt = MONEY_FORMAT;
      row.getCell(col).alignment = { horizontal: "right", vertical: "middle" };
    }
  }

  /** Dòng tổng: nhãn ở cột 3, số tiền ở cột 8; gộp 3–7 để nhãn không bị cắt. */
  function addTotal(label: string, amount: number, strong = false) {
    const row = ws.addRow(["", "", label, "", "", "", "", amount]);
    ws.mergeCells(row.number, 3, row.number, 7);
    row.height = strong ? 24 : 21;
    row.eachCell({ includeEmpty: true }, (cell) => {
      cell.font = { ...FONT, bold: true };
      cell.border = BORDER;
    });
    if (strong) {
      row.eachCell({ includeEmpty: true }, (cell) => {
        cell.fill = { type: "pattern", pattern: "solid", fgColor: { argb: "FFE9EDF2" } };
      });
      row.getCell(3).font = { ...FONT, bold: true, size: 15 };
    }
    row.getCell(3).alignment = { horizontal: "right", vertical: "middle" };
    row.getCell(8).numFmt = MONEY_FORMAT;
    row.getCell(8).alignment = { horizontal: "right", vertical: "middle" };
    return row;
  }

  addTotal("Tổng cộng", subtotal);
  addTotal("Chiết khấu", totalDiscount);
  addTotal("Tổng tiền thanh toán", subtotal - totalDiscount, true);

  // Cố định phần đầu + tên cột để bảng dài vẫn thấy cột khi cuộn.
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
