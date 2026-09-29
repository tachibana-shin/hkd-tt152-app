import { ref, shallowRef } from "vue";

/**
 * Trạng thái phân trang server-side dùng chung cho các bảng dữ liệu lớn.
 *
 * Màn nào cũng đi theo cùng một vòng đời: bảng lazy báo "đổi trang / sắp xếp /
 * tìm kiếm" → ghép thành `lazyEvent` → gọi API lấy đúng trang đó. Gom ở đây để
 * mỗi màn chỉ còn phần riêng (truy vấn gì, bộ lọc riêng nào) thay vì lặp lại 5
 * biến và 4 hàm giống hệt nhau.
 *
 * Mốc thời gian: `first` = số dòng đã bỏ qua, `rows` = kích thước trang.
 */
export function useLazyPage(
  load: (lazy: Record<string, unknown>) => Promise<void>,
  options: { rows?: number; pageSizes?: number[] } = {},
) {
  const first = ref(0);
  const rowsPerPage = ref(options.rows ?? 50);
  const pageSizes = options.pageSizes ?? [20, 50, 100, 200];
  const keyword = ref("");
  const sortField = ref<string | null>(null);
  const sortOrder = ref<1 | -1 | null>(null);
  /** Bộ lọc riêng của màn (ngày, trạng thái…) đi kèm vào lazyEvent. */
  const extra = shallowRef<Record<string, unknown>>({});
  /**
   * Hàng lọc theo cột của DataTable (mỗi khoá là tên field, value là điều kiện).
   * Bảng lazy không tự lọc client-side nên màn phải gửi lên server.
   */
  const columnFilters = ref<Record<string, { value: unknown }>>({});

  function build(): Record<string, unknown> {
    return {
      first: first.value,
      rows: rowsPerPage.value,
      sortField: sortField.value,
      sortOrder: sortOrder.value,
      filters: {
        global: { value: keyword.value || null, matchMode: "contains" },
        ...columnFilters.value,
      },
      ...extra.value,
    };
  }

  async function reload() {
    await load(build());
  }

  /** Luôn về trang đầu khi đổi điều kiện lọc — nếu không có thể đang ở trang 5
   *  trong khi kết quả chỉ còn 1 trang. */
  function onPage(event: { first: number; rows: number }) {
    first.value = event.first;
    rowsPerPage.value = event.rows;
    return reload();
  }

  function onSort(event: { sortField?: string | null; sortOrder?: 1 | -1 | null }) {
    sortField.value = event.sortField ?? null;
    sortOrder.value = event.sortOrder ?? null;
    first.value = 0;
    return reload();
  }

  /** Ô tìm kiếm toàn cục do AppDataTable dựng sẵn trong header bảng. */
  function onSearch(value: string) {
    keyword.value = value ?? "";
    first.value = 0;
    return reload();
  }

  /**
   * Màn có ô tìm kiếm RIÊNG (không phải ô trong header bảng) thì đặt từ khoá ở
   * đây — cùng khoá `global` mà server hiểu.
   */
  function setKeyword(value: string) {
    keyword.value = value ?? "";
    first.value = 0;
    return reload();
  }

  /** Màn vừa đổi ô lọc theo cột → nạp lại trang đầu. */
  function setColumnFilters(value: Record<string, { value: unknown }>) {
    columnFilters.value = value;
    first.value = 0;
    return reload();
  }

  /** Màn thay đổi bộ lọc riêng rồi nạp lại. */
  function setExtra(value: Record<string, unknown>) {
    extra.value = value;
    first.value = 0;
    return reload();
  }

  /** Nạp lần đầu (và mỗi lần quay lại màn — KeepAlive). */
  function init() {
    return reload();
  }

  return {
    first,
    rowsPerPage,
    pageSizes,
    onPage,
    onSort,
    onSearch,
    setKeyword,
    setColumnFilters,
    setExtra,
    reload,
    init,
  };
}
