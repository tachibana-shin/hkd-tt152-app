import { computed, ref, watch } from "vue";

/**
 * Phân trang phía client cho bảng đã có toàn bộ dữ liệu trong bộ nhớ.
 *
 * Dùng cho bảng không gọi API theo trang (dòng mặt hàng phiếu nhập, preview
 * file Excel…): tự `slice(first, first + rows)` để PrimeVue chỉ render1 trang
 * — bảng vài nghìn dòng không treo trình duyệt. Trả `first`/`rows` điều khiển
 * được (không để PrimeVue tự giữ trang) nên khi mảng rút xuống dưới `first`
 * (xóa bớt dòng) sẽ tự lùi về trang còn dữ liệu thay vì đứng nh trang trắng.
 *
 * Gọi rồi giải cấu trúc ở top-level của `<script setup>` để template tự unwrap ref:
 *   `const { first, paged, showPager, onPage } = useClientPage(() => items.value, 50)`
 */
export function useClientPage<T>(getList: () => T[], defaultRows = 20) {
  const first = ref(0);
  const rows = ref(defaultRows);
  const list = computed(() => getList());
  const totalRows = computed(() => list.value.length);
  /** Chỉ hiện thanh phân trang khi dữ liệu nhiều hơn 1 trang. */
  const showPager = computed(() => totalRows.value > rows.value);
  const paged = computed(() =>
    showPager.value ? list.value.slice(first.value, first.value + rows.value) : list.value,
  );

  /** Vị trí bắt đầu của trang cuối cùng còn chứa dữ liệu. */
  function lastStart(n: number): number {
    return n <= 0 ? 0 : Math.floor((n - 1) / rows.value) * rows.value;
  }

  // Dữ liệu rút xuống dưới `first` (xóa dòng / đổi file) → lùi về trang có dữ liệu.
  watch(totalRows, (n) => {
    if (first.value >= n) first.value = lastStart(n);
  });

  function onPage(e: { first: number; rows?: number }) {
    first.value = e.first;
    if (e.rows && e.rows > 0 && e.rows !== rows.value) rows.value = e.rows;
  }

  /** Về trang đầu (mở lại dialog / chọn file khác). */
  function reset() {
    first.value = 0;
  }

  /** Nhảy trang cuối — dùng khi thêm dòng mới ở cuối mảng. */
  function gotoLast() {
    first.value = lastStart(totalRows.value);
  }

  return { first, rows, totalRows, paged, showPager, onPage, reset, gotoLast };
}
