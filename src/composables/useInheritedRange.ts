import { parseIsoDate } from "@/utils/format";

/**
 * Khoảng ngày + nút quay lại khi màn được mở từ màn khác.
 *
 * Sổ nhật ký chung và Sổ chi tiết tiền nhận khoảng ngày và đường quay lại qua
 * query `?from=&to=&back=/accounting&backLabel=…` nếu có màn khác đẩy sang —
 * nhờ vậy màn đích mở ra đúng kỳ đang xem thay vì lùi về cả năm. Không có query
 * thì dùng mặc định của màn và nút quay lại đưa về `fallback`.
 *
 * Màn Kế toán HKD **không còn đẩy query này**: 3 nút S2a/S2b/S3a chọn mẫu sổ
 * và hiển thị ngay trên chỗ đó (`selectBook`), không còn chuyển sang `/ledger`
 * hay `/cash` nữa. Cơ chế query vẫn giữ cho lần nối màn sau.
 *
 * Query rác (thiếu, sai định dạng, ngược thứ tự) chỉ bị bỏ qua, không làm hỏng màn.
 */
export function useInheritedRange(fallback = "/accounting") {
  const route = useRoute();
  const router = useRouter();

  const one = (v: unknown): Date | null => (typeof v === "string" ? parseIsoDate(v) : null);
  const from = one(route.query.from);
  const to = one(route.query.to);
  // Chỉ nhận đường dẫn nội bộ bắt đầu bằng "/" và không phải "//" (URL ngoài).
  const back = typeof route.query.back === "string" ? route.query.back : "";
  const safeBack = /^\/(?!\/)/.test(back) ? back : "";

  return {
    /** Khoảng ngày màn trước truyền sang (null nếu không có hoặc sai định dạng). */
    inheritedFrom: from,
    inheritedTo: to && (!from || to >= from) ? to : null,
    /** Nút "Quay lại": về đúng màn trước, không có thì về màn dự phòng. */
    goBack: () => router.push(safeBack || fallback),
    /** Tên hiển thị trên nút quay lại (lấy từ `backLabel` do màn trước truyền). */
    backLabel:
      typeof route.query.backLabel === "string" && route.query.backLabel.trim()
        ? route.query.backLabel.trim()
        : "",
  };
}
