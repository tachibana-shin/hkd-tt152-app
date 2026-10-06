import { Workbook } from "exceljs";
import type { IndustryRevenue } from "@/types";
import { downloadBlob } from "@/utils/download";
import { fmtPct } from "@/utils/format";

/**
 * Tờ khai **kê khai doanh thu** theo mẫu của hộ kinh doanh — dựng đúng bố cục
 * màn "Chọn địa điểm kinh doanh cần kê khai doanh thu" trên cổng thuế:
 *
 *   I. Hoạt động sản xuất, kinh doanh hàng hóa, cung cấp dịch vụ có địa điểm
 *      kinh doanh cố định
 *   | Nhóm ngành nghề | Doanh thu thuế giá trị gia tăng |
 *                   | Doanh thu thuế thu nhập cá nhân  |
 *
 * 6 dòng nhóm ngành là **cố định** — nhóm chưa phát sinh doanh thu vẫn lên bảng
 * với ô trống, đúng như trên cổng (khác màn "Tờ khai thuế" vốn chỉ liệt kê nhóm
 * có phát sinh).
 *
 * Số liệu tự lấp từ sổ (`get_revenue_by_industry`) nhưng ô vẫn sửa được; tờ khai
 * in/xuất theo số đã sửa.
 */

/** Một dòng nhóm ngành nghề trên mẫu. */
export interface TaxEntryGroup {
  /** Thứ tự dòng trên mẫu (1 → 6). */
  order: number;
  /** Tên nhóm ngành nghề đúng như mẫu. */
  name: string;
  /**
   * Mã nhóm ngành trong `industry_group` của hồ sơ được gộp vào dòng này.
   * Mẫu dùng 6 nhóm tổng quát, hồ sơ của hộ tách nhỏ hơn (Sản xuất, Vận tải,
   * Dịch vụ có bao thầu NVL…) nên phải gộp lại — xem chú thích từng dòng.
   */
  codes: string[];
  vatRate: number;
  pitRate: number;
}

/**
 * 6 nhóm ngành nghề của mẫu — tên và tỷ lệ lấy đúng theo biểu mẫu trên cổng.
 *
 * Dòng 6 ("Hoạt động kinh doanh khác") là **nơi chứa nhóm không khớp mẫu**: bất
 * kỳ mã `industry_group` nào không có trong 5 dòng trên đều gộp vào đây để tờ
 * khai không bao giờ bị hụt doanh thu.
 */
export const TAX_ENTRY_GROUPS: TaxEntryGroup[] = [
  {
    order: 1,
    name: "Phân phối, cung cấp hàng hóa",
    codes: ["PPHH"],
    vatRate: 0.01,
    pitRate: 0.005,
  },
  {
    order: 2,
    name: "Dịch vụ, xây dựng không bao thầu nguyên vật liệu",
    codes: ["DVXD-KNL"],
    vatRate: 0.05,
    pitRate: 0.02,
  },
  {
    order: 3,
    name: "Hoạt động cho thuê tài sản trừ bất động sản",
    codes: [],
    vatRate: 0.05,
    pitRate: 0.05,
  },
  {
    order: 4,
    name: "Sản xuất, vận tải, dịch vụ có gắn với hàng hóa, xây dựng có bao thầu nguyên vật liệu",
    codes: ["SX-CN", "DVXD-KL", "VCTT"],
    vatRate: 0.03,
    pitRate: 0.015,
  },
  {
    order: 5,
    name: "Hoạt động cung cấp sản phẩm nội dung thông tin số",
    codes: [],
    vatRate: 0.05,
    pitRate: 0.05,
  },
  {
    order: 6,
    name: "Hoạt động kinh doanh khác",
    codes: [],
    vatRate: 0.02,
    pitRate: 0.01,
  },
];

/** Dòng tờ khai — thêm 2 cột doanh thu người dùng sửa được. */
export interface TaxEntryRow extends TaxEntryGroup {
  /** Doanh thu thuế giá trị gia tăng. */
  revenueVat: number;
  /** Doanh thu thuế thu nhập cá nhân — mặc định khớp cột GTGT. */
  revenuePit: number;
}

/** Nhóm ngành của hồ sơ không khớp mẫu nào (đã rơi vào "Hoạt động kinh doanh khác"). */
export interface TaxEntryUnmapped {
  code: string;
  name: string;
  revenue: number;
}

const r2 = (n: number) => Math.round(n * 100) / 100;

/** Doanh thu thuần của 1 nhóm: tăng − giảm trong kỳ. */
const netOf = (r: IndustryRevenue) => r2(r.revenue_up - r.revenue_down);

/**
 * Gộp doanh thu từng nhóm ngành của hồ sơ vào **6 dòng cố định** của mẫu.
 *
 * Mã khớp `codes` thì vào đúng dòng; mã lạ gộp vào dòng 6 — nhờ vậy tờ khai
 * không bao giờ thiếu doanh thu chỉ vì hồ sơ thêm nhóm ngành mới.
 */
export function buildTaxEntryRows(revenues: IndustryRevenue[]): TaxEntryRow[] {
  const rows: TaxEntryRow[] = TAX_ENTRY_GROUPS.map((g) => ({
    ...g,
    codes: [...g.codes],
    revenueVat: 0,
    revenuePit: 0,
  }));
  const fallback = rows[rows.length - 1];
  for (const r of revenues) {
    const net = netOf(r);
    if (net === 0) continue;
    const target = rows.find((g) => g.codes.includes(r.industry_code)) ?? fallback;
    target.revenueVat = r2(target.revenueVat + net);
    target.revenuePit = r2(target.revenuePit + net);
  }
  return rows;
}

/** Danh sách nhóm ngành rớt vào dòng 6 — để báo cho người dùng biết vì sao. */
export function unmappedRevenue(revenues: IndustryRevenue[]): TaxEntryUnmapped[] {
  const mapped = new Set(TAX_ENTRY_GROUPS.flatMap((g) => g.codes));
  return revenues
    .filter((r) => !mapped.has(r.industry_code))
    .map((r) => ({ code: r.industry_code, name: r.industry_name, revenue: netOf(r) }))
    .filter((x) => x.revenue !== 0);
}

/**
 * Cột "Nhóm ngành nghề" của mẫu: tên + tỷ lệ ghi trong ngoặc.
 *
 * Tỷ lệ in đúng kiểu cổng ghi: `0.5%`, `1.5%` (dấu chấm thập phân) — dùng chung
 * [`fmtPct`] để không lệch với mọi số % elsewhere trong app.
 */
export function taxEntryGroupLabel(g: TaxEntryGroup): string {
  return `${g.name}\n(Tỷ lệ tính thuế GTGT: ${fmtPct(g.vatRate)}, TNCN: ${fmtPct(g.pitRate)})`;
}

// ─── Xuất file ───

/** Nội dung cần có của tờ khai (dùng chung cho Excel và bản in). */
export interface TaxEntryExport {
  /** Khoảng ngày đang kê khai, đã định dạng sẵn. */
  periodLabel: string;
  /** Mã địa điểm kinh doanh (trên cổng là số, mặc định 00999). */
  locationCode: string;
  /** Tên hộ / người nộp thuế. */
  taxpayer: string;
  /** Mã số thuế của hộ — dòng nhận diện trên đầu tờ khai. */
  taxCode: string;
  /** Địa chỉ đăng ký kinh doanh — dòng nhận diện trên đầu tờ khai (trống thì bỏ). */
  address: string;
  /**
   * Công tắc cạnh mã địa điểm trên cổng — tắt là **địa điểm này không tham
   * gia kê khai** kỳ này. Bản in/Excel phải ghi rõ trạng thái, không được ra
   * bảng số rồi để người nộp tự hiểu.
   */
  enabled: boolean;
  rows: TaxEntryRow[];
  unmapped: TaxEntryUnmapped[];
}

/**
 * Tờ khai + khoảng ngày + tên file — **bộ dữ liệu đầy đủ** mà màn tờ khai đang
 * giữ (số đã sửa tay) để nút "Xuất báo cáo kỳ thuế" đóng gói cùng các sổ.
 */
export interface TaxEntryPayload extends TaxEntryExport {
  from: string;
  to: string;
  fileName: string;
}

const THIN = { style: "thin" as const };
const BOX = { top: THIN, left: THIN, bottom: THIN, right: THIN };

/** Tên file Excel tờ khai theo khoảng ngày. */
export function taxEntryFileName(fromIso: string, toIso: string): string {
  return `to-khai-ke-khai-${fromIso || "ky"}-${toIso || "ky"}.xlsx`;
}

/**
 * Dòng nhận diện hộ kê khai — "Mã số thuế: … — Địa chỉ: …".
 *
 * Dùng chung cho Excel (`buildTaxEntryXlsx`) và bản in (`TaxEntryForm`) để
 * hai bản không lệch nhau; phần hồ sơ chưa khai thì bỏ, không nhét placeholder.
 */
export function taxIdentityLine(o: { taxCode: string; address: string }): string {
  const parts: string[] = [];
  const tax = o.taxCode.trim();
  const addr = o.address.trim();
  if (tax) parts.push(`Mã số thuế: ${tax}`);
  if (addr) parts.push(`Địa chỉ: ${addr}`);
  return parts.join(" — ");
}

/**
 * Dựng file `.xlsx` của tờ khai — đầu file lấy **data hồ sơ HKD** (tên hộ +
 * mã số thuế / địa chỉ) thay dòng placeholder "Chọn địa điểm kinh doanh cần
 * kê khai doanh thu" chép từ portal, rồi kỳ kê khai, mục I, mã địa điểm,
 * bảng 3 cột 6 dòng. Không cộng thêm dòng tổng vì mẫu gốc không có.
 */
export async function buildTaxEntryXlsx(o: TaxEntryExport): Promise<Blob> {
  const wb = new Workbook();
  const ws = wb.addWorksheet("Kê khai doanh thu");
  ws.columns = [{ width: 72 }, { width: 34 }, { width: 34 }];

  const mergeText = (row: number, text: string, bold = false, size = 11) => {
    ws.mergeCells(row, 1, row, 3);
    const c = ws.getCell(row, 1);
    c.value = text;
    c.alignment = { wrapText: true, vertical: "middle" };
    c.font = { name: "Times New Roman", bold, size };
    return c;
  };

  // Đầu file: tên hộ + MST/địa chỉ (data hồ sơ) — không phải dòng placeholder.
  mergeText(1, o.taxpayer.trim() || "HỘ KINH DOANH", true, 13);
  mergeText(2, taxIdentityLine(o));
  mergeText(3, `Kỳ kê khai: ${o.periodLabel}`);
  mergeText(
    4,
    "I. Hoạt động sản xuất, kinh doanh hàng hóa, cung cấp dịch vụ có địa điểm kinh doanh cố định",
    true,
  );
  mergeText(
    5,
    o.enabled
      ? `1. ${o.locationCode} - ${o.taxpayer}`
      : `1. ${o.locationCode} - ${o.taxpayer} — KHÔNG KÊ KHAI ĐỊA ĐIỂM NÀY`,
  );
  ws.getRow(1).height = 22;
  ws.getRow(4).height = 20;

  const head = ws.getRow(6);
  head.values = [
    "Nhóm ngành nghề",
    "Doanh thu thuế giá trị gia tăng",
    "Doanh thu thuế thu nhập cá nhân",
  ];
  head.eachCell((cell) => {
    cell.font = { name: "Times New Roman", bold: true, size: 11 };
    cell.alignment = { wrapText: true, vertical: "middle", horizontal: "center" };
    cell.border = BOX;
    cell.fill = { type: "pattern", pattern: "solid", fgColor: { argb: "FFEFEFEF" } };
  });
  head.height = 32;

  // Địa điểm bị tắt công tắc: ô doanh thu để TRỐNG chứ không phải 0 — 0 là số
  // liệu kê khai, trống mới là "không kê khai", không để nhầm lẫn khi nộp.
  o.rows.forEach((r, i) => {
    const row = ws.getRow(7 + i);
    row.values = [
      taxEntryGroupLabel(r),
      o.enabled ? r.revenueVat : null,
      o.enabled ? r.revenuePit : null,
    ];
    row.getCell(1).alignment = { wrapText: true, vertical: "middle" };
    for (const col of [1, 2, 3]) {
      const cell = row.getCell(col);
      cell.font = { name: "Times New Roman", size: 11 };
      cell.border = BOX;
      if (col > 1) cell.numFmt = "#,##0";
    }
    row.height = 34;
  });

  // Ghi chú nối tiếp nhau ngay dưới bảng (nếu có).
  const notes: string[] = [];
  if (!o.enabled) {
    notes.push(
      `Địa điểm kinh doanh ${o.locationCode} bị tắt công tắc → không tham gia kê khai doanh thu kỳ này.`,
    );
  }
  if (o.unmapped.length) {
    notes.push(
      `Nhóm ngành của hồ sơ không có trong mẫu đã gộp vào "Hoạt động kinh doanh khác": ` +
        o.unmapped.map((u) => `${u.name} (${u.code})`).join(", "),
    );
  }
  notes.forEach((text, i) => {
    const noteRow = 7 + o.rows.length + i;
    ws.mergeCells(noteRow, 1, noteRow, 3);
    const c = ws.getCell(noteRow, 1);
    c.value = text;
    c.font = { name: "Times New Roman", italic: true, size: 10 };
    c.alignment = { wrapText: true, vertical: "middle" };
    ws.getRow(noteRow).height = 30;
  });

  return wb.xlsx.writeBuffer().then((buf) => new Blob([buf as ArrayBuffer]));
}

/** Dựng tờ khai `.xlsx` rồi tải về máy. */
export async function exportTaxEntry(
  o: TaxEntryExport,
  fromIso: string,
  toIso: string,
): Promise<void> {
  const blob = await buildTaxEntryXlsx(o);
  await downloadBlob(taxEntryFileName(fromIso, toIso), blob);
}
