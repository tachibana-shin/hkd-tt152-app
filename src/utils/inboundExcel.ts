import * as XLSX from "xlsx";
import { notEmpty, toNumber } from "./importExcel";

/**
 * Đọc file Excel nhập hàng — KHÔNG buộc file phải có hàng tiêu đề đúng mẫu.
 *
 * File có hàng tiêu đề gọn (Tên · ĐVT · Số lượng · Thành tiền) thì tự đoán ánh
 * xạ cột; file không có tiêu đề chuẩn — vd "Phụ lục bảng kê hoạt động kinh
 * doanh" của phần mềm kế toán TT88: tiêu đề nhiều hàng gộp ô (Số dư đầu kỳ /
 * Nhập trong kỳ / Xuất trong kỳ / Tồn cuối kỳ × Số lượng · Thành tiền) kèm mã
 * cột [06]…[15] — thì trả về ánh xạ gợi ý để người dùng CHỌN TAY từng cột
 * (xem InboundExcelDialog).
 */
export interface InboundExcelRow {
  /** Tên mặt hàng — dùng để dò danh mục, chưa có thì tạo mới. */
  name: string;
  /** Đơn vị tính. */
  unit: string;
  /** Số lượng. */
  quantity: number;
  /** Thành tiền. */
  amount: number;
  /** Đơn giá suy ra = thành tiền ÷ số lượng. */
  unit_price: number;
}

export interface ParsedInboundExcel {
  rows: InboundExcelRow[];
  /** Dòng có tên nhưng thiếu số liệu (SL và tiền đều ≤ 0) — bị bỏ qua. */
  skipped: number;
}

/** Ánh xạ cột (index 0-based, -1 = không dùng) + vị trí bắt đầu vùng dữ liệu. */
export interface ColumnMapping {
  /** Cột tên mặt hàng (bắt buộc). */
  name: number;
  /** Cột đơn vị tính. */
  unit: number;
  /** Cột số lượng. */
  quantity: number;
  /** Cột thành tiền. */
  amount: number;
  /** Hàng dữ liệu đầu tiên, tính từ 0 (header của Excel = hàng 1). */
  start: number;
}

/** Một cặp cột số liệu của phụ lục tồn kho, vd "Số dư đầu kỳ" = SL + Thành tiền. */
export interface ColumnGroup {
  label: string;
  quantity: number;
  amount: number;
}

export interface InboundExcelSheet {
  /** Toàn bộ dòng của sheet đầu tiên (đã padding đủ độ rộng cột). */
  rows: unknown[][];
  /** Ánh xạ tự đoán — người dùng sửa tay được. */
  mapping: ColumnMapping;
  /** Các nhóm cột số liệu (chỉ có ở file phụ lục tồn kho nhiều tầng). */
  groups: ColumnGroup[];
}

/** Chuẩn hoá tên mặt hàng để so khớp: gộp khoảng trắng thừa + chữ thường. */
export function normInboundName(s: string): string {
  return s.trim().replace(/\s+/g, " ").toLowerCase();
}

/** Bỏ dấu tiếng Việt + về chữ thường — so khớp nhãn cột bất kể hoa/thường/dấu. */
function stripAccents(s: string): string {
  return s
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .replace(/Đ/g, "D")
    .replace(/đ/g, "d")
    .toLowerCase();
}

/** Ô → chuỗi đã trim (rỗng nếu ô trống). */
function cellText(v: unknown): string {
  return notEmpty(v) ? String(v).trim() : "";
}

/**
 * Ép ô dữ liệu về số: bỏ ký tự không phải số (đ, chữ cái đầu ô "10 cái"…)
 * rồi dùng `toNumber` chuẩn (hỗ trợ "1.200.000", "1,5").
 */
function cellNumber(v: unknown): number {
  return toNumber(typeof v === "string" ? v.replace(/[^\d.,-]/g, "").trim() : v);
}

/**
 * Ô chứa số thật — dùng để nhận ra hàng đầu vùng dữ liệu. "Số lượng"(chữ) và
 * mã cột "[08]" của file TT88 đều KHÔNG tính, nhờ đó tiêu đề không bị lẫn vào.
 */
function isNumericCell(v: unknown): boolean {
  if (typeof v === "number") return Number.isFinite(v);
  if (typeof v !== "string") return false;
  const s = v.trim();
  return s !== "" && /\d/.test(s) && !/[\p{L}[\]]/u.test(s);
}

/** Số cột (0-based) → chữ cái cột Excel: 0 → A, 26 → AA. */
export function colLetter(index: number): string {
  let n = Math.max(index, 0);
  let out = "";
  do {
    out = String.fromCharCode(65 + (n % 26)) + out;
    n = Math.floor(n / 26) - 1;
  } while (n >= 0);
  return out;
}

function columnCount(rows: unknown[][]): number {
  return rows.reduce((w, r) => Math.max(w, r?.length ?? 0), 0);
}

function truncate(s: string, n: number): string {
  return s.length > n ? `${s.slice(0, n - 1)}…` : s;
}

/**
 * Vùng tiêu đề: các hàng ĐẦU FILE, kết thúc ngay trước hàng đầu tiên có số liệu.
 * (File TT88 có 6 hàng tiêu đề/nhãn; file gọn có 1.)
 */
function headerEnd(rows: unknown[][]): number {
  const limit = Math.min(rows.length, 30);
  for (let i = 0; i < limit; i++) if ((rows[i] ?? []).some(isNumericCell)) return i;
  return limit;
}

/** Nhãn text từng cột trong vùng tiêu đề (bỏ mã cột "[06]" — không phải nhãn). */
function headerLabels(rows: unknown[][], end: number): string[][] {
  const out: string[][] = [];
  for (let i = 0; i < end; i++) {
    for (const [c, cell] of (rows[i] ?? []).entries()) {
      const t = cellText(cell);
      if (!t || /^\[[^\]]*\]$/.test(t)) continue;
      (out[c] ??= []).push(t);
    }
  }
  return out;
}

const UNIT_KEYS = ["don vi tinh", "don vi do", "dvt"];
const QTY_KEYS = ["so luong"];
const AMOUNT_KEYS = ["thanh tien", "tong tien", "so tien", "gia tri von"];
const NAME_KEYS = [
  "ten mat hang",
  "ten hang",
  "mat hang",
  "vat lieu",
  "hang hoa",
  "san pham",
  "vat tu",
];

/** Cột trái nhất có nhãn chứa một trong các từ khoá (đã bỏ dấu). */
function findColumn(labels: string[][], keys: string[], skip = new Set<number>()): number {
  for (let c = 0; c < labels.length; c++) {
    if (skip.has(c)) continue;
    const hay = stripAccents((labels[c] ?? []).join(" · "));
    if (hay && keys.some((k) => hay.includes(k))) return c;
  }
  return -1;
}

/** Các cặp cột "Số lượng/Thành tiền" có tiêu đề nhóm ở hàng trên (file TT88). */
function findGroups(rows: unknown[][], end: number): ColumnGroup[] {
  let sub = -1;
  let best = 0;
  for (let i = 0; i < end; i++) {
    const hits = (rows[i] ?? []).filter((c) =>
      stripAccents(cellText(c)).startsWith("so luong"),
    ).length;
    if (hits > best) {
      best = hits;
      sub = i;
    }
  }
  if (best < 2 || sub < 0) return [];

  const groups: ColumnGroup[] = [];
  for (const [c, cell] of (rows[sub] ?? []).entries()) {
    if (!stripAccents(cellText(cell)).startsWith("so luong")) continue;
    if (!cellText((rows[sub] ?? [])[c + 1])) continue; // thiếu cột Thành tiền đi kèm
    // Tiêu đề nhóm nằm ở hàng trên (ô gộp): "Số dư đầu kỳ", "Nhập trong kỳ"…
    let label = "";
    for (let i = sub - 1; i >= 0 && !label; i--) label = cellText((rows[i] ?? [])[c]);
    if (label) groups.push({ label, quantity: c, amount: c + 1 });
  }
  return groups;
}

/** Ưu tiên nhóm cột với vai trò "nhập kho": Nhập → Số dư đầu kỳ → Tồn cuối → Xuất. */
function groupPriority(label: string): number {
  const s = stripAccents(label);
  if (s.includes("nhap")) return 0;
  if (s.includes("dau ky")) return 1;
  if (s.includes("cuoi ky")) return 2;
  if (s.includes("xuat")) return 3;
  return 4;
}

/**
 * Chọn nhóm cột mặc định: nhóm CÓ số liệu đầu tiên theo thứ tự ưu tiên
 * (file TT88 kỳ nhập bằng 0 thì rơi về "Số dư đầu kỳ" — số dư đủ để xem preview).
 */
function defaultGroup(rows: unknown[][], groups: ColumnGroup[], start: number): ColumnGroup | null {
  if (!groups.length) return null;
  const sorted = [...groups].sort((a, b) => groupPriority(a.label) - groupPriority(b.label));
  for (const g of sorted) {
    for (let i = start; i < rows.length; i++) {
      const row = rows[i] ?? [];
      if (cellNumber(row[g.quantity]) > 0 || cellNumber(row[g.amount]) > 0) return g;
    }
  }
  return sorted[0] ?? null;
}

/**
 * Hàng đầu vùng dữ liệu: hàng đầu tiên có tên + có số ở CÁC CỘT KHÁC cột tên.
 *
 * Không bám theo cột số lượng/đã chọn: đổi sang nhóm "Nhập trong kỳ" (nhiều
 * hàng trống) không được làm vùng dữ liệu nhảy xuống giữa file. Số ở cột STT
 * hay số dư đầu kỳ đều nhận ra được hàng tiêu đề (hàng tiêu đề chỉ có chữ hoặc
 * mã cột "[08]" — không phải số).
 */
export function findStart(rows: unknown[][], m: ColumnMapping): number {
  if (m.name < 0) return 0;
  let first = -1;
  for (let i = 0; i < rows.length; i++) {
    const row = rows[i] ?? [];
    if (!cellText(row[m.name])) continue;
    if (first < 0) first = i;
    if (row.some((cell, c) => c !== m.name && isNumericCell(cell))) return i;
  }
  return first >= 0 ? first : 0;
}

/**
 * Đoán cột từ CHÍNH DỮ LIỆU khi file không có (hoặc thiếu) hàng tiêu đề:
 * cột chữ đầu tiên = tên, cột chữ kế = ĐVT, các cột số bên phải tên = SL rồi tiền.
 */
function guessFromData(rows: unknown[][]): Omit<ColumnMapping, "start"> {
  const guess = { name: -1, unit: -1, quantity: -1, amount: -1 };
  for (const row of rows.slice(0, 30)) {
    for (const [c, cell] of (row ?? []).entries()) {
      if (!notEmpty(cell)) continue;
      if (isNumericCell(cell)) {
        // Số chỉ tính khi đã gặp cột tên — cột STT/cột số đứng trước không lấy.
        if (guess.name < 0 || c === guess.name) continue;
        if (guess.quantity < 0) guess.quantity = c;
        else if (guess.amount < 0) guess.amount = c;
      } else if (guess.name < 0) guess.name = c;
      else if (guess.unit < 0) guess.unit = c;
    }
    if (guess.name >= 0 && guess.quantity >= 0) break;
  }
  return guess;
}

/** Đoán ánh xạ cột từ nhãn tiêu đề + các cặp nhóm cột số liệu. */
function guessMapping(rows: unknown[][]): { mapping: ColumnMapping; groups: ColumnGroup[] } {
  const end = headerEnd(rows);
  const labels = headerLabels(rows, end);
  const data = guessFromData(rows);
  const unit0 = findColumn(labels, UNIT_KEYS);
  const quantity0 = findColumn(labels, QTY_KEYS);
  const amount0 = findColumn(labels, AMOUNT_KEYS, new Set(quantity0 >= 0 ? [quantity0] : []));
  const name0 = findColumn(
    labels,
    NAME_KEYS,
    new Set([unit0, quantity0, amount0].filter((c) => c >= 0)),
  );

  // Thiếu nhãn cột số → lấy từ chính dữ liệu; riêng khi đã có 1 trong 2 cột số
  // thì giữ nguyên (cột còn lại chủ động bỏ trống, người dùng chọn tay nếu cần).
  const noNumber = quantity0 < 0 && amount0 < 0;
  const name = name0 >= 0 ? name0 : data.name;
  const unit = unit0 >= 0 ? unit0 : data.unit;
  const quantity = quantity0 >= 0 ? quantity0 : noNumber ? data.quantity : -1;
  const amount = amount0 >= 0 ? amount0 : noNumber ? data.amount : -1;

  const mapping: ColumnMapping = { name, unit, quantity, amount, start: 0 };
  mapping.start = findStart(rows, mapping);

  // File phụ lục tồn kho: chọn nhóm cột CÓ số liệu (ưu tiên "Nhập trong kỳ"),
  // ghi đè cặp phỏng đoán ở trên. `start` không đổi vì không phụ thuộc nhóm.
  const groups = findGroups(rows, end);
  const group = defaultGroup(rows, groups, mapping.start);
  if (group) {
    mapping.quantity = group.quantity;
    mapping.amount = group.amount;
  }
  return { mapping, groups };
}

/** Đọc sheet đầu tiên + tự đoán ánh xạ cột (người dùng sửa tay được). */
export async function readInboundSheet(file: File): Promise<InboundExcelSheet> {
  const wb = XLSX.read(await file.arrayBuffer(), { type: "array" });
  const ws = wb.Sheets[wb.SheetNames[0]!];
  const rows = XLSX.utils.sheet_to_json<unknown[]>(ws, { header: 1, raw: true, defval: "" });
  return { rows, ...guessMapping(rows) };
}

/** Các cột đang có dữ liệu — danh sách lựa chọn cho ô chọn cột. */
export function usedColumns(rows: unknown[][]): number[] {
  const cols = new Set<number>();
  for (const row of rows) for (const [c, v] of (row ?? []).entries()) if (notEmpty(v)) cols.add(c);
  if (!cols.size) return [0, 1, 2, 3];
  return [...cols].sort((a, b) => a - b);
}

/**
 * Nhãn gợi ý cho từng cột (khi chọn tay): gộp nhãn các hàng tiêu đề phía trên,
 * không có thì lấy ô dữ liệu đầu tiên — người xem biết cột đó chứa gì.
 */
export function columnHints(rows: unknown[][], start: number): string[] {
  const labels = headerLabels(rows, Math.max(0, Math.min(start, 30)));
  const out: string[] = [];
  for (let c = 0; c < columnCount(rows); c++) {
    let hint = (labels[c] ?? []).join(" · ");
    if (!hint) {
      for (let i = start; i < rows.length; i++) {
        const t = cellText((rows[i] ?? [])[c]);
        if (t) {
          hint = t;
          break;
        }
      }
    }
    out[c] = truncate(hint, 60);
  }
  return out;
}

/** Đọc dòng theo ánh xạ đã chọn: dòng thiếu số liệu bị bỏ và đếm riêng. */
export function mapInboundRows(
  sheet: { rows: unknown[][] },
  mapping: ColumnMapping,
): ParsedInboundExcel {
  const out: InboundExcelRow[] = [];
  let skipped = 0;
  if (mapping.name < 0) return { rows: out, skipped };

  for (let i = Math.max(mapping.start, 0); i < sheet.rows.length; i++) {
    const cells = sheet.rows[i] ?? [];
    const name = cellText(cells[mapping.name]);
    if (!name) continue;

    const quantity = mapping.quantity >= 0 ? cellNumber(cells[mapping.quantity]) : 0;
    const amount = mapping.amount >= 0 ? cellNumber(cells[mapping.amount]) : 0;
    if (quantity <= 0 && amount <= 0) {
      skipped++;
      continue;
    }
    // Có tiền mà thiếu SL (hoặc SL ≤ 0) → coi là 1 để tính đơn giá, giống import sổ sách.
    const qty = quantity > 0 ? quantity : 1;
    out.push({
      name,
      unit: mapping.unit >= 0 ? cellText(cells[mapping.unit]) : "",
      quantity: qty,
      amount,
      unit_price: amount > 0 ? Math.round((amount / qty) * 100) / 100 : 0,
    });
  }
  return { rows: out, skipped };
}
