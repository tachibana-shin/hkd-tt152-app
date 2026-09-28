/**
 * Khoảng ngày của kỳ tính thuế.
 *
 * Kỳ khai thuế của hộ được lưu ở app_setting (`tax_period`):
 *   • `month`          — khai theo tháng
 *   • `quarter`        — khai theo quý (mặc định)
 *   • `year`           — nhóm 1, thông báo doanh thu một lần năm
 *   • `per_occurrence` — cũng thuộc nhóm 1 nên xem theo năm
 *
 * Các màn báo cáo dùng kỳ này làm mặc định: mở màn ra thì thấy đúng kỳ đang
 * chạy thay vì cả năm, tính từ ngày hôm nay.
 */

/** Một khoảng thời gian theo giờ địa phương (DatePicker làm việc với Date). */
export type PeriodRange = { from: Date; to: Date };

/** Ngày cuối của tháng (month = 0-11) — truyền 0 cho tháng kế = ngày cuối tháng trước. */
const lastDayOfMonth = (year: number, month: number) => new Date(year, month + 1, 0);

/**
 * Khoảng ngày của kỳ chứa `today`, theo kỳ khai của hộ.
 * Kỳ lạ (rỗng, không đọc được) thì rơi về cả năm — an toàn, không lỗi.
 */
export function taxPeriodRange(period: string | null | undefined, today = new Date()): PeriodRange {
  const y = today.getFullYear();
  const m = today.getMonth();
  if (period === "month") {
    return { from: new Date(y, m, 1), to: lastDayOfMonth(y, m) };
  }
  if (period === "quarter") {
    const first = Math.floor(m / 3) * 3;
    return { from: new Date(y, first, 1), to: lastDayOfMonth(y, first + 2) };
  }
  return { from: new Date(y, 0, 1), to: new Date(y, 11, 31) };
}

/** `01/07/2026` — dùng cho nhãn hiển thị cạnh ô chọn ngày. */
export const ddmmyyyy = (d: Date): string =>
  `${String(d.getDate()).padStart(2, "0")}/${String(d.getMonth() + 1).padStart(2, "0")}/${d.getFullYear()}`;

/**
 * Gọn tên kỳ cho khoảng ngày đang chọn: quý/tháng/năm nếu khớp ranh giới kỳ,
 * nếu không thì in thẳng khoảng ngày. Dùng để người dùng biết đang xem kỳ nào
 * mà không phải tự nhẩy từ hai ô ngày.
 */
export function describeRange(from: Date | null, to: Date | null): string {
  if (!from || !to) return "Chưa chọn khoảng ngày";
  if (
    from.getDate() === 1 &&
    to.getMonth() === 11 &&
    to.getDate() === 31 &&
    from.getMonth() === 0
  ) {
    return `Năm ${from.getFullYear()}`;
  }
  if (
    from.getDate() === 1 &&
    from.getMonth() === to.getMonth() &&
    to.getDate() === lastDayOfMonth(to.getFullYear(), to.getMonth()).getDate()
  ) {
    return `Tháng ${from.getMonth() + 1}/${from.getFullYear()}`;
  }
  if (
    from.getDate() === 1 &&
    to.getMonth() === from.getMonth() + 2 &&
    to.getDate() === lastDayOfMonth(to.getFullYear(), to.getMonth()).getDate()
  ) {
    return `Quý ${Math.floor(from.getMonth() / 3) + 1}/${from.getFullYear()}`;
  }
  return `${ddmmyyyy(from)} – ${ddmmyyyy(to)}`;
}
