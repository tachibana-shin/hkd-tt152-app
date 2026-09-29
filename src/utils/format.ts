/** Định dạng số / tiền tệ dùng chung toàn app (vi-VN). */

const NF_INT = new Intl.NumberFormat("vi-VN", { maximumFractionDigits: 0 });
const NF_DEC = new Intl.NumberFormat("vi-VN", { maximumFractionDigits: 2 });

/** Số nguyên có phân cách nghìn (vd `1.234.567`). */
export const fmtInt = (n: number | null | undefined): string => NF_INT.format(n ?? 0);

/** Số thập phân tối đa 2 chữ số (vd `1.234,56`). */
export const fmtDec = (n: number | null | undefined): string => NF_DEC.format(n ?? 0);

/** Tiền VND kèm ký hiệu (vd `1.234.567 đ`). */
export const fmtVnd = (n: number | null | undefined): string => `${NF_INT.format(n ?? 0)} đ`;

/**
 * Mốc tiền rút gọn cho nhãn: `1.000.000.000` → `1 tỷ`, `500.000.000` → `500 triệu`.
 *
 * Ngưỡng thuế giờ sửa được ở Cài đặt nên nhãn phải đọc theo giá trị đang áp dụng,
 * nhưng viết "1.000.000.000 đ" trong nhãn nhóm hộ thì khó đọc hơn "1 tỷ".
 */
export const fmtThreshold = (n: number | null | undefined): string => {
  const v = n ?? 0;
  if (v >= 1_000_000_000) {
    const t = v / 1_000_000_000;
    const txt = Number.isInteger(t) ? String(t) : NF_DEC.format(t);
    return `${txt} tỷ`;
  }
  if (v >= 1_000_000) {
    const tr = v / 1_000_000;
    const txt = Number.isInteger(tr) ? String(tr) : NF_DEC.format(tr);
    return `${txt} triệu`;
  }
  return NF_INT.format(v);
};

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

/**
 * Mốc thời gian do SQLite ghi bằng `datetime('now')` (giờ UTC) → chuỗi giờ địa
 * phương để hiển thị. Không có mốc giờ trong chuỗi nên phải gắn `Z` (UTC) khi
 * parse, rồi định dạng theo giờ máy — nếu hiện thẳng chuỗi gốc sẽ lệch 7 giờ ở
 * Việt Nam.
 */
export const fmtLocalDateTime = (ts?: string | null): string => {
  if (!ts) return "";
  const d = new Date(`${ts.trim().replace(" ", "T")}Z`);
  if (Number.isNaN(d.getTime())) return ts;
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getDate())}/${p(d.getMonth() + 1)}/${d.getFullYear()} ${p(d.getHours())}:${p(d.getMinutes())}`;
};

/**
 * Ngày trong DB (yyyy-mm-dd) → Date lúc 00:00 **giờ địa phương** để đưa vào
 * DatePicker. `new Date("2026-09-29")` sẽ là 00:00 UTC nên lệch một ngày ở múi
 * giờ âm.
 */
export const parseIsoDate = (s?: string | null): Date | null =>
  s?.trim() ? new Date(`${s.trim().slice(0, 10)}T00:00:00`) : null;
