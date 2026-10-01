import { computed, ref } from "vue";
import { useToast } from "primevue/usetoast";
import { api } from "@/db";
import type { TaxBook } from "@/types";
import { exportBookDocx } from "@/utils/bookDoc";
import { exportXlsx, type XlsxColumn } from "@/utils/excel";

/**
 * Trạng thái SỔ KẾ TOÁN theo mẫu TT 152/2025/TT-BTC — dùng chung cho tab
 * "Sổ kế toán" (`/books`) và màn Kế toán HKD (`/accounting`).
 *
 * Kỳ sổ phải đi cùng kỳ tờ khai như trước đây: bấm "Chuyển nhanh" hoặc lưu
 * Cấu hình thuế ở màn Kế toán thì mở tab Sổ ra đúng kỳ đó. Hai màn không mở
 * cùng lúc (mỗi route một view) nên state đặt ở phạm vi module — dùng chung
 * mà không cần store, và KeepAlive của màn nào giữ state của màn đó.
 */

/** 7 mẫu sổ của TT 152/2025 (Điều 4) — lọc theo hồ sơ khi đã nạp xong. */
const allBookOptions = [
  { label: "S1a-HKD · Sổ doanh thu (nhóm 1)", value: "S1a" },
  { label: "S2a-HKD · Sổ doanh thu theo nhóm ngành (nộp theo % doanh thu)", value: "S2a" },
  { label: "S2b-HKD · Sổ doanh thu (nộp GTGT theo %, TNCN theo thu nhập)", value: "S2b" },
  { label: "S2c-HKD · Sổ chi tiết doanh thu, chi phí", value: "S2c" },
  { label: "S2d-HKD · Sổ chi tiết vật liệu, hàng hóa", value: "S2d" },
  { label: "S2e-HKD · Sổ chi tiết tiền", value: "S2e" },
  { label: "S3a-HKD · Sổ theo dõi nghĩa vụ thuế khác", value: "S3a" },
];

/**
 * Danh sách mẫu sổ áp dụng cho hồ sơ (Điều 4 TT 152) do backend trả.
 *
 * `null` = chưa nạp xong (hoặc lỗi) → vẫn hiện đủ 7 mẫu, không vì thế mà biến
 * mất sổ đang xem. Số nhóm/phương pháp tính thuế đổi khi lưu Cấu hình nên không
 * tự suy ra ở đây — một nguồn duy nhất là backend.
 */
const applicableBooks = ref<string[] | null>(null);

/** Công tắc xem đủ 7 mẫu sổ — dùng khi đối chiếu mẫu in, nhớ lại giữa các lần mở. */
const SHOW_ALL_BOOKS_KEY = "hkd:tt152:show-all-books";
const showAllBooks = ref(localStorage.getItem(SHOW_ALL_BOOKS_KEY) === "1");
/** Mẫu sổ đang xem — giữ khi quay lại màn (KeepAlive) và khi đổi kỳ. */
const book = ref("S2a");

/**
 * Năm của sổ — đi theo NĂM khoảng ngày màn Kế toán HKD đang xem (màn đó đồng
 * bộ mỗi lần nạp tờ khai, nên đổi khoảng ngày sang năm khác thì sổ cùng đổi).
 * Mở thẳng tab Sổ chưa qua màn Kế toán thì mặc định năm hiện tại.
 */
const bookYear = ref(new Date().getFullYear());

/**
 * Bộ chọn kỳ — chỉ dùng cho SỔ KẾ TOÁN theo mẫu TT 152/2025.
 *
 * Tờ khai thuế đã gộp còn một bảng và chạy theo khoảng ngày ở thanh công cụ,
 * nên không còn bộ chọn kỳ ở tờ khai.
 *
 * Sổ mặc định là cuốn GHI CẢ NĂM: Luật Kế toán (Điều 25.2, 26.1, 26.4, 26.7)
 * bắt sổ mở vào đầu kỳ kế toán năm, ghi liên tục đến khi khóa sổ và in thành
 * quyển riêng cho từng năm để lưu trữ. TT 152/2025 không quy định kỳ cho sổ nhưng
 * mẫu S2a có sẵn dòng "Kỳ kê khai" → vẫn giữ chọn quý/tháng để TRÍCH sổ theo
 * kỳ kê khai, đối chiếu với tờ khai 01/CNKD của kỳ đó. Số kỳ để sẵn theo quý
 * hiện tại để người dùng đổi sang "Theo quý" là ra ngay kỳ đang chạy.
 */
const bookPeriod = ref<"year" | "quarter" | "month">("year");
const bookPeriodNo = ref(Math.floor(new Date().getMonth() / 3) + 1);

const bookPeriodTypeOptions: { label: string; value: "year" | "quarter" | "month" }[] = [
  { label: "Theo quý", value: "quarter" },
  { label: "Theo tháng", value: "month" },
  { label: "Theo năm", value: "year" },
];
const bookPeriodNoOptions = computed(() => {
  if (bookPeriod.value === "quarter")
    return [1, 2, 3, 4].map((q) => ({ label: `Quý ${q}`, value: q }));
  if (bookPeriod.value === "month")
    return Array.from({ length: 12 }, (_, i) => ({
      label: `Tháng ${i + 1}`,
      value: i + 1,
    }));
  return [];
});

const bookOptions = computed(() =>
  showAllBooks.value || applicableBooks.value == null
    ? allBookOptions
    : allBookOptions.filter((o) => applicableBooks.value!.includes(o.value)),
);

const bookData = ref<TaxBook | null>(null);
const bookLoading = ref(false);

export function useTaxBooks() {
  const toast = useToast();

  /** Một mẫu có được hiện/chọn không (dùng cho dropdown lẫn các nút sổ nhanh). */
  function bookVisible(value: string): boolean {
    return bookOptions.value.some((o) => o.value === value);
  }

  /**
   * Sổ đang chọn phải luôn nằm trong danh sách được phép.
   *
   * Đổi nhóm hộ hoặc tắt công tắc "xem tất cả" thì sổ cũ có thể không còn thuộc
   * hồ sơ → tự chuyển về mẫu áp dụng đầu tiên, không để người dùng kẹt trên một
   * mẫu sổ không nộp được. Trả về true nếu đã phải đổi sổ.
   */
  function ensureBookVisible(): boolean {
    if (bookVisible(book.value)) return false;
    const first = bookOptions.value[0];
    if (first) book.value = first.value;
    return true;
  }

  /**
   * Nạp danh sách mẫu sổ đúng hồ sơ (Điều 4 TT 152) cho năm đang xem.
   * Lỗi thì để nguyên `null` → dropdown vẫn hiện đủ 7 mẫu; lỗi nạp sổ báo ở
   * `loadBook` nên ở đây không toast lần nữa cho khỏi trùng.
   */
  async function loadApplicable() {
    try {
      applicableBooks.value = await api.getApplicableBooks(bookYear.value);
    } catch {
      /* chưa nạp được → giữ `null`, xem chú thích của `applicableBooks` */
    }
  }

  async function loadBook() {
    bookLoading.value = true;
    try {
      const no = bookPeriod.value === "year" ? 0 : bookPeriodNo.value;
      bookData.value = await api.getTaxBooks(bookYear.value, bookPeriod.value, no, book.value);
    } catch (e) {
      bookData.value = null;
      toast.add({ severity: "error", summary: "Lỗi tải sổ kế toán", detail: String(e) });
    } finally {
      bookLoading.value = false;
    }
  }

  /** Bật/tắt xem đủ 7 mẫu sổ → lưu lựa chọn và đổi ngay sang sổ hợp lệ nếu cần. */
  function toggleShowAllBooks(value: boolean) {
    showAllBooks.value = value;
    localStorage.setItem(SHOW_ALL_BOOKS_KEY, value ? "1" : "0");
    if (ensureBookVisible()) void loadBook();
  }

  /**
   * Chuyển sang mẫu sổ `target` rồi nạp và cuộn tới chỗ đang xem.
   *
   * Trước đây bấm "Sổ S2a/S3a-HKD" đẩy sang màn nhật ký chung `/ledger` (S3a thì
   * đẩy sang `/cash` — hoàn toàn sai mẫu), nên số liệu không đúng mẫu TT 152 và
   * mất luôn kỳ đang xem. Nay đổi mẫu ngay trên tab Sổ.
   */
  function selectBook(target: string) {
    // Sổ không thuộc hồ sơ (vd chuyển sang S2b khi hộ đang nộp theo % doanh thu)
    // thì không mở — chọn là in ra sổ không nộp được.
    if (!bookVisible(target)) return;
    book.value = target;
    void loadBook().then(() => {
      document
        .querySelector("[data-testid='tax-book']")
        ?.scrollIntoView({ behavior: "smooth", block: "start" });
    });
  }

  /**
   * Xuất sổ hiện tại ra Excel theo đúng bộ cột của mẫu.
   *
   * Bộ cột mỗi mẫu một bộ (S1a 3 cột, S2d 12 cột…) nên không khai cứng được
   * như trước — lấy nguyên `columns` backend trả về, ô trống xuất rỗng.
   */
  async function exportBookExcel() {
    const b = bookData.value;
    if (!b) return;
    const cols: XlsxColumn[] = b.columns.map((c) => ({ header: c.label, key: c.key }));
    const rows = b.rows.map((r) => {
      const out: Record<string, unknown> = {};
      for (const c of b.columns) out[c.key] = r.cells[c.key] ?? "";
      return out;
    });
    try {
      await exportXlsx(`so-ke-toan-${b.book}-${bookYear.value}`, cols, rows);
    } catch (e) {
      toast.add({ severity: "error", summary: "Lỗi xuất file Excel", detail: String(e) });
    }
  }

  /**
   * Xuất cùng mẫu sổ đó ra file Word `.docx` khổ A4 dọc — khi muốn nộp bản sửa
   * tay hoặc in bằng Word thay vì in thẳng từ trình duyệt. Tên file trùng mẫu
   * Excel cho dễ tìm trong thư mục tải về.
   */
  async function exportBookWord() {
    const b = bookData.value;
    if (!b) return;
    try {
      await exportBookDocx(b, `so-ke-toan-${b.book}-${bookYear.value}`);
    } catch (e) {
      toast.add({ severity: "error", summary: "Lỗi xuất file Word", detail: String(e) });
    }
  }

  /**
   * Nạp một lần khi mở/ quay lại tab Sổ: danh sách mẫu đúng hồ sơ → chọn sổ
   * hợp lệ → nạp sổ đang xem.
   */
  async function refreshBooks() {
    await loadApplicable();
    ensureBookVisible();
    await loadBook();
  }

  return {
    /** Năm của sổ — màn Sổ tính khoảng ngày đẩy sang Sổ nhật ký / Thu-Chi. */
    bookYear,
    book,
    bookPeriod,
    bookPeriodNo,
    bookPeriodTypeOptions,
    bookPeriodNoOptions,
    bookOptions,
    bookVisible,
    bookData,
    bookLoading,
    showAllBooks,
    selectBook,
    loadBook,
    toggleShowAllBooks,
    exportBookExcel,
    exportBookWord,
    refreshBooks,
  };
}
