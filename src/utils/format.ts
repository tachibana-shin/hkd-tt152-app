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

/**
 * Date → chuỗi ngày `yyyy-mm-dd` theo **giờ địa phương**.
 *
 * Không dùng `toISOString().slice(0, 10)`: DatePicker trả về Date lúc 00:00 giờ
 * địa phương, còn `toISOString()` đổi sang UTC — ở múi giờ Việt Nam (+7) ngày
 * được chọn sẽ ra **ngày hôm trước**. Sai lệch này lên tới mọi ngày chứng từ
 * (hóa đơn, phiếu kho, thu chi…) nên phải định dạng theo giờ địa phương.
 */
export const toIsoDate = (d: Date | null | undefined): string => {
  if (!d) return "";
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
};
