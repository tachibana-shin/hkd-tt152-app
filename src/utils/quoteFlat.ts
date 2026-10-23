import type { BomItem } from "@/types";
import type { QuoteExcelLine } from "@/utils/quoteExcel";

/** Dòng hàng đang nhập trên hóa đơn nháp (đủ dữ liệu để báo giá). */
export interface FlatQuoteSourceLine {
  product_code: string;
  /** Tên hiển thị trên dòng (tên khác/alias) — đã fallback về tên chính. */
  name: string;
  unit: string;
  quantity: number;
  price: number;
  /** Chiết khấu của dòng; nếu là hàng chế tạo sẽ chia cho các dòng con. */
  discount?: number;
}

/**
 * Làm phẳng báo giá: hàng chế tạo (có định mức) được thay bằng các dòng vật tư/
 * dịch vụ con, nhân đúng số lượng thành phẩm đang báo giá.
 *
 * - Hàng thường/dịch vụ không có định mức → giữ nguyên dòng.
 * - Định mức chỉ có dịch vụ (không có vật tư) → coi là gói dịch vụ, giữ nguyên.
 * - Định mức lồng nhau (bán thành phẩm) → tách tiếp tới vật tư cuối.
 * - Gặp vòng lặp định mức (A chứa B, B chứa A) → giữ dòng đang xét để không treo.
 * - Chiết khấu của gói được chia theo tỉ lệ thành tiền các dòng con, nhờ vậy
 *   tổng báo giá flat vẫn giữ đúng phần chiết khấu khách đã nhập.
 */
export function flattenQuoteLines(lines: FlatQuoteSourceLine[], boms: BomItem[]): QuoteExcelLine[] {
  const byProduct = new Map<string, BomItem[]>();
  for (const bom of boms) {
    const rows = byProduct.get(bom.product_code) ?? [];
    rows.push(bom);
    byProduct.set(bom.product_code, rows);
  }

  function isManufactured(productCode: string): boolean {
    return (byProduct.get(productCode) ?? []).some((b) => !b.material_is_service);
  }

  function expand(line: FlatQuoteSourceLine, ancestors: Set<string>): QuoteExcelLine[] {
    if (!isManufactured(line.product_code) || ancestors.has(line.product_code)) {
      return [
        {
          code: line.product_code,
          name: line.name,
          unit: line.unit,
          quantity: line.quantity,
          price: line.price,
          discount: 0,
        },
      ];
    }

    const nextAncestors = new Set(ancestors).add(line.product_code);
    const result: QuoteExcelLine[] = [];
    for (const material of byProduct.get(line.product_code) ?? []) {
      const child: FlatQuoteSourceLine = {
        product_code: material.material_code,
        name: material.material_name,
        unit: material.material_unit,
        quantity: (line.quantity || 0) * (material.quantity || 0),
        price: material.unit_price || 0,
        discount: 0,
      };
      result.push(...expand(child, nextAncestors));
    }
    return result;
  }

  /** Chia chiết khấu của dòng gốc cho các dòng con theo thành tiền (đồng). */
  function distributeDiscount(rows: QuoteExcelLine[], discount: number): QuoteExcelLine[] {
    const totalDiscount = Math.round(discount || 0);
    if (!totalDiscount || !rows.length) return rows;
    const gross = rows.reduce((sum, row) => sum + (row.quantity || 0) * (row.price || 0), 0);
    if (gross <= 0) return rows;

    let allocated = 0;
    return rows.map((row, index) => {
      const share =
        index === rows.length - 1
          ? totalDiscount - allocated
          : Math.floor(((row.quantity || 0) * (row.price || 0) * totalDiscount) / gross);
      allocated += share;
      return { ...row, discount: share };
    });
  }

  return lines.flatMap((line) => {
    const rows = expand(line, new Set());
    return distributeDiscount(rows, line.discount ?? 0);
  });
}
