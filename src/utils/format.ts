/** Định dạng số / tiền tệ dùng chung toàn app (vi-VN). */

const NF_INT = new Intl.NumberFormat("vi-VN", { maximumFractionDigits: 0 });
const NF_DEC = new Intl.NumberFormat("vi-VN", { maximumFractionDigits: 2 });

/** Số nguyên có phân cách nghìn (vd `1.234.567`). */
export const fmtInt = (n: number | null | undefined): string => NF_INT.format(n ?? 0);

/** Số thập phân tối đa 2 chữ số (vd `1.234,56`). */
export const fmtDec = (n: number | null | undefined): string => NF_DEC.format(n ?? 0);

/** Tiền VND kèm ký hiệu (vd `1.234.567 đ`). */
export const fmtVnd = (n: number | null | undefined): string => `${NF_INT.format(n ?? 0)} đ`;

/** Tỷ lệ phần trăm (vd `1%` hoặc `1,5%`). */
export const fmtPct = (n: number | null | undefined): string =>
  `${((n ?? 0) * 100).toFixed(1).replace(/\.0$/, "")}%`;
