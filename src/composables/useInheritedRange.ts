import { parseIsoDate } from "@/utils/format";

/**
 * Khoảng ngày + nút quay lại khi màn được mở từ màn khác.
 *
 * Màn Kế toán HKD mở sổ (Sổ nhật ký chung, Sổ chi tiết tiền) bằng cách đẩy route
 * kèm `?from=&to=&back=/accounting`. Nhờ vậy màn đích mở ra **đúng khoảng ngày
 * đang xem** thay vì lùi về cả năm, và có nút quay lại chỗ vừa đến — trước đây
 * bấm nút sổ là rơi vào màn khác mà không có đường về, phải tự tìm trên thanh
 * công cụ.
 *
 * Query rác (thiếu, sai định dạng, ngược thứ tự) chỉ bị bỏ qua, không làm hỏng màn.
 */
export function useInheritedRange(fallback = "/accounting") {
  const route = useRoute();
  const router = useRouter();

  const one = (v: unknown): Date | null => (typeof v === "string" ? parseIsoDate(v) : null);
  const from = one(route.query.from);
  const to = one(route.query.to);

  return {
    /** Khoảng ngày màn trước truyền sang (null nếu không có hoặc sai định dạng). */
    inheritedFrom: from,
    inheritedTo: to && (!from || to >= from) ? to : null,
    /** Đường dẫn quay lại màn trước, rỗng nếu vào thẳng từ thanh công cụ. */
    backTo: typeof route.query.back === "string" ? route.query.back : "",
    /** Nút "Quay lại": về đúng màn trước, không có thì về màn dự phòng. */
    goBack: () =>
      router.push((typeof route.query.back === "string" && route.query.back) || fallback),
    /** Tên hiển thị trên nút quay lại (lấy từ `backLabel` do màn trước truyền). */
    backLabel:
      typeof route.query.backLabel === "string" && route.query.backLabel.trim()
        ? route.query.backLabel.trim()
        : "",
  };
}
