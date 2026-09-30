import { parseIsoDate } from "@/utils/format";

/**
 * Khoảng ngày + nút quay lại khi màn được mở từ màn khác.
 *
 * Sổ nhật ký chung và Sổ chi tiết tiền nhận khoảng ngày và đường quay lại qua
 * query `?from=&to=&back=/accounting&backLabel=…` nếu có màn khác đẩy sang —
 * nhờ vậy màn đích mở ra đúng kỳ đang xem thay vì lùi về cả năm. Không có query
 * thì dùng mặc định của màn và nút quay lại đưa về `fallback`.
 *
 * Màn Kế toán HKD mở 2 màn này bằng 2 nút ở thẻ "Sổ sách theo mẫu TT 152"
 * (`openJournal`), kèm đúng khoảng ngày người dùng đang xem.
 *
 * Mọi giá trị trả về là **computed đọc `route.query` mỗi lần truy cập**, không
 * phải số chụp một lần ở `setup()`: các màn này nằm trong `<KeepAlive>` nên mở
 * lại bằng query mới sẽ KHÔNG chạy lại `setup()` — đọc tĩnh thì lần mở thứ hai
 * vẫn ra kỳ của lần mở đầu. Consumer phải tự `watch` để nhận thay đổi.
 *
 * Query rác (thiếu, sai định dạng, ngược thứ tự) chỉ bị bỏ qua, không làm hỏng màn.
 */
export function useInheritedRange(fallback = "/accounting") {
  const route = useRoute();
  const router = useRouter();

  const one = (v: unknown): Date | null => (typeof v === "string" ? parseIsoDate(v) : null);

  /** Khoảng ngày màn trước truyền sang (null nếu không có hoặc sai định dạng). */
  const inheritedFrom = computed(() => one(route.query.from));
  const inheritedTo = computed(() => {
    const from = inheritedFrom.value;
    const to = one(route.query.to);
    return to && (!from || to >= from) ? to : null;
  });
  /**
   * Đường dẫn màn trước gửi sang. Chỉ nhận đường dẫn nội bộ bắt đầu bằng "/"
   * và không phải "//" (URL ngoài) — không thì bỏ, giữ `fallback`.
   */
  const backPath = computed(() => {
    const back = typeof route.query.back === "string" ? route.query.back : "";
    return /^\/(?!\/)/.test(back) ? back : "";
  });
  /** Tên hiển thị trên nút quay lại (lấy từ `backLabel` do màn trước truyền). */
  const backLabel = computed(() =>
    typeof route.query.backLabel === "string" && route.query.backLabel.trim()
      ? route.query.backLabel.trim()
      : "",
  );

  return {
    /** Ngày đầu khoảng ngày màn trước truyền sang. */
    inheritedFrom,
    /** Ngày cuối (null nếu thiếu hoặc ngược thứ tự với ngày đầu). */
    inheritedTo,
    /** Nút "Quay lại": về đúng màn trước, không có thì về màn dự phòng. */
    goBack: () => router.push(backPath.value || fallback),
    /** Nhãn nút "Quay lại" do màn trước đặt. */
    backLabel,
  };
}
