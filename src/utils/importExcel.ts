import * as XLSX from "xlsx";

/**
 * Nhập khối dữ liệu từ file Excel mẫu (sheet "NHAP LIEU").
 *
 * Cấu trúc sheet (0-based khi parse với sheet_to_json header:1, raw:true, defval:""):
 *   dòng 0-4: header nhiều tầng (bỏ qua), dòng >= 5: dữ liệu (dòng trống xen kẽ "sọc")
 *   idx 1 SoPhieu | idx 2 NgayGhiSo | idx 3 SoHieu | idx 4 NgayChungTu | idx 5 KyKhaiThue
 *   idx 6 Diễn giải | idx 7 SL | idx 8 Số tiền | idx 9 MaKH | idx 10 MaNCC | idx 11 MaVatTu
 *   idx 18 NhomNganhNghe | idx 19 TyLeGTGT (%) | idx 20 TyLeTNCN (%)
 */
export interface ImportItem {
  posting_date: string;
  voucher_no: string;
  doc_date: string;
  entry_type: string;
  description: string;
  product_code: string;
  supplier_code: string;
  customer_code: string;
  quantity: number;
  unit_price: number;
  amount: number;
  debit_account: string;
  credit_account: string;
  industry_code: string;
  vat_rate: number;
  pit_rate: number;
  tax_period: number;
  unit_code: string;
  adjust_code: string;
  note: string;
}

export const notEmpty = (v: unknown): boolean => v !== "" && v !== undefined && v !== null;

/** Excel date serial -> ISO (yyyy-mm-dd) */
function serialToIso(v: number): string {
  const d = new Date(Math.round((v - 25569) * 86400 * 1000));
  return d.toISOString().slice(0, 10);
}

/** Parse ô ngày: số serial (20000..80000) hoặc chuỗi dd/mm/yyyy */
export function parseExcelDate(v: unknown): string {
  if (!notEmpty(v)) return "";
  if (typeof v === "number") {
    if (v >= 20000 && v <= 80000) return serialToIso(v);
    return "";
  }
  const s = String(v).trim();
  if (/^\d{4}-\d{2}-\d{2}/.test(s)) return s.slice(0, 10); // đã là ISO
  const m = s.match(/^(\d{1,2})[/\-.](\d{1,2})[/\-.](\d{2,4})$/);
  if (m) {
    const dd = m[1].padStart(2, "0");
    const mm = m[2].padStart(2, "0");
    let yyyy = m[3];
    if (yyyy.length === 2) yyyy = (Number(yyyy) > 30 ? "19" : "20") + yyyy;
    if (
      yyyy.length === 4 &&
      Number(mm) >= 1 &&
      Number(mm) <= 12 &&
      Number(dd) >= 1 &&
      Number(dd) <= 31
    ) {
      return `${yyyy}-${mm}-${dd}`;
    }
  }
  return "";
}

/** Parse số kiểu Excel/tiếng Việt: "1.200", "1,25", "0,01", "1.234,56" */
export function toNumber(v: unknown): number {
  if (typeof v === "number") return Number.isFinite(v) ? v : 0;
  if (v === true) return 1;
  if (v === false) return 0;
  if (!notEmpty(v)) return 0;
  let s = String(v).trim().replace(/\s/g, "");
  if (s === "") return 0;
  // chuẩn hoá dấu phân cách ("," luôn cần xử lý; "." chỉ khi không phải dạng thập phân "1.5")
  if (s.includes(",") || (s.includes(".") && !/^[+-]?\d+\.\d+$/.test(s))) {
    const hasComma = s.includes(",");
    const hasDot = s.includes(".");
    if (hasComma && hasDot) {
      // dấu thập phân là dấu xuất hiện sau cùng
      const dec = s.lastIndexOf(",") > s.lastIndexOf(".") ? "," : ".";
      s = s
        .split(dec === "," ? "." : ",")
        .join("")
        .split(dec)
        .join(".");
    } else if (hasComma) {
      const parts = s.split(",");
      if (parts.length > 2) s = parts.join(""); // "1,200,000" -> nghìn
      else if (parts[1] && parts[1].length === 3) s = parts.join(""); // "1,200" -> nghìn
      else s = parts.join("."); // "0,01" -> thập phân
    } else {
      const parts = s.split(".");
      if (parts.length > 2) s = parts.join(""); // "1.200.000" -> nghìn
      else if (parts[1] && parts[1].length === 3) s = parts.join(""); // "1.200" -> nghìn
    }
  }
  const n = Number(s);
  return Number.isFinite(n) ? n : 0;
}

/**
 * Parse tỷ lệ thuế: ô đơn vị % (vd 1 = 1% -> 0.01); giá trị đã là số thập phân (≤ 1) giữ nguyên.
 * Chuỗi kiểu "0,01" hoặc "1%" cũng được xử lý.
 */
export function toRate(v: unknown): number {
  if (!notEmpty(v)) return 0;
  if (typeof v === "number") {
    if (!Number.isFinite(v)) return 0;
    // giá trị >= 1 được coi là đơn vị % (vd 1 = 1% -> 0.01); < 1 là số thập phân, giữ nguyên
    return v >= 1 ? v / 100 : Math.max(v, 0);
  }
  const s = String(v).trim();
  if (s.endsWith("%")) return Math.max(toNumber(s.slice(0, -1)) / 100, 0);
  const n = toNumber(s);
  return n >= 1 ? n / 100 : Math.max(n, 0);
}

/** KyKhaiThue: số 202601 hoặc chuỗi "202601" / "01/2026" -> 202601 */
export function toTaxPeriod(v: unknown): number {
  if (typeof v === "number") return Math.trunc(v);
  if (!notEmpty(v)) return 0;
  const s = String(v).trim();
  const m = s.match(/^(\d{1,2})[/\-.](\d{2,4})$/);
  if (m) {
    const mm = m[1].padStart(2, "0");
    const yyyy = (m[2].length === 2 ? "20" : "") + m[2];
    return Number(yyyy + mm);
  }
  return Math.trunc(toNumber(s));
}

/** Entry type từ 2 ký tự đầu Số phiếu (PN/PX/PT/PC, ngoài ra OTHER) */
export function entryTypeOf(voucherNo: string): string {
  const code = voucherNo.trim().slice(0, 2).toUpperCase();
  return ["PN", "PX", "PT", "PC"].includes(code) ? code : "OTHER";
}

/** Map một dòng thô -> ImportItem; trả null nếu không phải dòng dữ liệu (nhãn/tổng cộng). */
export function parseImportRow(row: unknown[]): ImportItem | null {
  const soPhieu = notEmpty(row[1]) ? String(row[1]).trim() : "";
  const ngayGhiSo = parseExcelDate(row[2]);
  const voucherOk = soPhieu !== "" && /^[A-Za-z]{2,4}[- ]?\d+$/.test(soPhieu);
  // Dòng chỉ có nhãn ("SoPhieu", "x", tổng cộng...) mà không có ngày hợp lệ -> bỏ qua
  if (!voucherOk && !ngayGhiSo) return null;

  const qtyRaw = toNumber(row[7]);
  const quantity = qtyRaw > 0 ? qtyRaw : 1; // 0/rỗng -> 1
  const amount = toNumber(row[8]);
  const unitPrice =
    quantity > 0 && amount > 0 ? Math.round((amount / quantity) * 100) / 100 : amount;

  return {
    posting_date: ngayGhiSo,
    voucher_no: soPhieu,
    doc_date: parseExcelDate(row[4]) || ngayGhiSo, // NgayChungTu; rỗng -> NgayGhiSo
    entry_type: entryTypeOf(soPhieu),
    description: notEmpty(row[6]) ? String(row[6]).trim() : "",
    product_code: notEmpty(row[11]) ? String(row[11]).trim() : "",
    supplier_code: notEmpty(row[10]) ? String(row[10]).trim() : "",
    customer_code: notEmpty(row[9]) ? String(row[9]).trim() : "",
    quantity,
    unit_price: unitPrice,
    amount,
    debit_account: "",
    credit_account: "",
    industry_code: notEmpty(row[18]) ? String(row[18]).trim() : "",
    vat_rate: toRate(row[19]),
    pit_rate: toRate(row[20]),
    tax_period: toTaxPeriod(row[5]),
    unit_code: "",
    adjust_code: "",
    note: notEmpty(row[3]) ? String(row[3]).trim() : "", // SoHieu -> note
  };
}

export function typeSeverity(t: string): string {
  switch (t) {
    case "PN":
      return "info";
    case "PX":
      return "warn";
    case "PT":
      return "success";
    case "PC":
      return "danger";
    default:
      return "secondary";
  }
}

export interface ParsedNhapLieu {
  sheetName: string;
  rows: unknown[][];
  items: ImportItem[];
  candidates: number;
  skipped: number;
}

/**
 * Đọc sheet "NHAP LIEU" từ file Excel. Trả null nếu file không có sheet dữ liệu.
 * Chỉ lấy dòng có Số phiếu (idx 1) hoặc Ngày ghi sổ (idx 2) — bỏ dòng trống xen kẽ.
 */
export async function readNhapLieuSheet(file: File): Promise<ParsedNhapLieu | null> {
  const wb = XLSX.read(await file.arrayBuffer(), { type: "array" });
  const sheetName = wb.SheetNames.find((n) => n.toUpperCase().includes("NHAP LIEU"));
  if (!sheetName) return null;

  const ws = wb.Sheets[sheetName];
  const rows = XLSX.utils.sheet_to_json<unknown[]>(ws, { header: 1, raw: true, defval: "" });

  const items: ImportItem[] = [];
  let candidates = 0;
  let skipped = 0;
  for (let r = 5; r < rows.length; r++) {
    const row = rows[r] ?? [];
    if (!notEmpty(row[1]) && !notEmpty(row[2])) continue;
    candidates++;
    const item = parseImportRow(row);
    if (item) items.push(item);
    else skipped++;
  }
  return { sheetName, rows, items, candidates, skipped };
}
