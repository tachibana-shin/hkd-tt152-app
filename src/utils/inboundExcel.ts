import * as XLSX from "xlsx";
import { notEmpty, toNumber } from "./importExcel";

/** Một dòng đọc từ file Excel nhập hàng — chỉ theo THỨ TỰ cột, không theo nhãn. */
export interface InboundExcelRow {
  /** Tên mặt hàng (cột 1) — dùng để dò danh mục, chưa có thì tạo mới. */
  name: string;
  /** Đơn vị tính (cột 2). */
  unit: string;
  /** Số lượng (cột 3 — cột "Số lượng tồn" trong file người dùng gửi). */
  quantity: number;
  /** Tổng tiền (cột 4). */
  amount: number;
  /** Đơn giá suy ra = tổng tiền ÷ số lượng. */
  unit_price: number;
}

export interface ParsedInboundExcel {
  rows: InboundExcelRow[];
  /** Dòng có tên nhưng thiếu số liệu (SL và tiền đều = 0) — bị bỏ qua. */
  skipped: number;
}

/** Chuẩn hoá tên mặt hàng để so khớp: gộp khoảng trắng thừa + chữ thường. */
export function normInboundName(s: string): string {
  return s.trim().replace(/\s+/g, " ").toLowerCase();
}

/**
 * Ép ô dữ liệu về số: bỏ ký tự không phải số (đ, chữ cái đầu ô "10 cái"…)
 * rồi dùng `toNumber` chuẩn (hỗ trợ "1.200.000", "1,5").
 */
function cellNumber(v: unknown): number {
  return toNumber(typeof v === "string" ? v.replace(/[^\d.,-]/g, "").trim() : v);
}

/** Ô chỉ chứa chữ (không có số) — đặc trưng của hàng tiêu đề, vd "Số lượng tồn". */
const isHeaderText = (v: unknown): boolean =>
  typeof v === "string" && /[A-Za-zÀ-ỹ]/.test(v) && !/\d/.test(v);

/**
 * Đọc file Excel nhập hàng (cột theo THỨ TỰ, nhãn có thể lệch tên):
 *   0 Tên mặt hàng · 1 Đơn vị tính · 2 Số lượng tồn · 3 Tổng tiền
 *
 * Sheet đầu tiên; bỏ dòng trống; tự bỏ hàng tiêu đề (dòng đầu có ô cột 3/4
 * là chữ, hoặc tên bắt đầu bằng "Tên"/đúng "Sản phẩm"); dòng thiếu số liệu
 * bị bỏ và đếm riêng để báo trong preview.
 */
export async function readInboundExcel(file: File): Promise<ParsedInboundExcel> {
  const wb = XLSX.read(await file.arrayBuffer(), { type: "array" });
  const ws = wb.Sheets[wb.SheetNames[0]!];
  const raw = XLSX.utils.sheet_to_json<unknown[]>(ws, { header: 1, raw: true, defval: "" });

  const rows: InboundExcelRow[] = [];
  let skipped = 0;
  let firstRowDone = false;

  for (const row of raw) {
    const cells: unknown[] = Array.isArray(row) ? row : [];
    const name = notEmpty(cells[0]) ? String(cells[0]).trim() : "";
    if (!name) continue;

    // Hàng tiêu đề — chỉ xét dòng có dữ liệu đầu tiên, sau đó coi mọi dòng là dữ liệu.
    if (!firstRowDone) {
      firstRowDone = true;
      const headerish =
        isHeaderText(cells[2]) ||
        isHeaderText(cells[3]) ||
        /^t[êe]n\b/i.test(name) ||
        /^s[ảa]n ph[ẩầ]m$/i.test(name);
      if (headerish) continue;
    }

    let quantity = cellNumber(cells[2]);
    const amount = cellNumber(cells[3]);
    if (quantity <= 0 && amount <= 0) {
      skipped++;
      continue;
    }
    // Có tiền mà thiếu SL (hoặc SL = 0) → coi là 1 để tính đơn giá, giống import sổ sách.
    if (quantity <= 0) quantity = 1;
    rows.push({
      name,
      unit: notEmpty(cells[1]) ? String(cells[1]).trim() : "",
      quantity,
      amount,
      unit_price: amount > 0 ? Math.round((amount / quantity) * 100) / 100 : 0,
    });
  }

  return { rows, skipped };
}
