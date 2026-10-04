import { computed, ref, watch } from "vue";
import { useToast } from "primevue/usetoast";
import { api } from "@/db";
import type { TaxBook } from "@/types";
import { exportBookDocx } from "@/utils/bookDoc";
import { buildTaxBookXlsx } from "@/utils/excel";
import { downloadBlob } from "@/utils/download";

/**
 * Trạng thái SỔ KẾ TOÁN theo mẫu TT 152/2025/TT-BTC — tab "Sổ kế toán"
 * (`/books`).
 *
 * Sổ KHÔNG còn đi theo tờ khai: năm, loại kỳ và số kỳ do người dùng chọn
 * ngay trên tab Sổ rồi nhớ lại (localStorage) — màn Kế toán HKD tự lo khoảng
 * ngày báo cáo của riêng nó, không gióng gì sang đây. State đặt ở phạm vi
 * module để sống qua các lần mở lại tab trong phiên (không cần store), và
 * KeepAlive của màn giữ state của màn đó.
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
 * Cả 7 mẫu sổ **không lọc theo hồ sơ** — riêng chức năng "Xuất báo cáo kỳ
 * thuế" phải gộp **tất cả** sổ kế toán theo năm vào một ZIP, kể cả sổ hộ chưa
 * áp dụng (thiếu thì hồ sơ thiếu sổ).
 */
export const TT152_BOOKS = allBookOptions;

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

type BookPeriod = "year" | "quarter" | "month";

/** Năm xem sổ — 2020 … năm sau: quá khứ để tra cứu, năm sau để ghi sổ trước. */
const MIN_BOOK_YEAR = 2020;
const MAX_BOOK_YEAR = new Date().getFullYear() + 1;
const bookYearOptions = Array.from({ length: MAX_BOOK_YEAR - MIN_BOOK_YEAR + 1 }, (_, i) => ({
  label: String(MIN_BOOK_YEAR + i),
  value: MIN_BOOK_YEAR + i,
}));

/** Số kỳ hợp lệ với loại kỳ — mặc định theo kỳ đang chạy (quý/tháng hiện tại). */
function defaultPeriodNo(period: BookPeriod): number {
  const now = new Date();
  return period === "month" ? now.getMonth() + 1 : Math.floor(now.getMonth() / 3) + 1;
}

/**
 * Năm + kỳ của sổ: tự chọn trên tab Sổ, KHÔNG đi theo tờ khai nữa, nhớ lại
 * qua các lần mở app (localStorage). Dữ liệu cũ/hỏng thì về mặc định.
 *
 * Sổ mặc định là cuốn GHI CẢ NĂM: Luật Kế toán (Điều 25.2, 26.1, 26.4, 26.7)
 * bắt sổ mở vào đầu kỳ kế toán năm, ghi liên tục đến khi khóa sổ và in thành
 * quyển riêng cho từng năm để lưu trữ. TT 152/2025 không quy định kỳ cho sổ nhưng
 * mẫu S2a có sẵn dòng "Kỳ kê khai" → vẫn giữ chọn quý/tháng để TRÍCH sổ theo
 * kỳ kê khai, đối chiếu với tờ khai 01/CNKD của kỳ đó. Số kỳ để sẵn theo quý
 * hiện tại để người dùng đổi sang "Theo quý" là ra ngay kỳ đang chạy.
 */
const BOOK_VIEW_KEY = "hkd:tt152:book-view";
function readBookView(): { year: number; period: BookPeriod; no: number } {
  const fallback = {
    year: new Date().getFullYear(),
    period: "year" as BookPeriod,
    no: defaultPeriodNo("quarter"),
  };
  try {
    const saved = JSON.parse(localStorage.getItem(BOOK_VIEW_KEY) ?? "null") as {
      year?: number;
      period?: string;
      no?: number;
    } | null;
    if (!saved) return fallback;
    const period: BookPeriod =
      saved.period === "quarter" || saved.period === "month" ? saved.period : "year";
    const year =
      typeof saved.year === "number" && saved.year >= MIN_BOOK_YEAR && saved.year <= MAX_BOOK_YEAR
        ? saved.year
        : fallback.year;
    const max = period === "quarter" ? 4 : 12;
    const no =
      typeof saved.no === "number" && saved.no >= 1 && saved.no <= max
        ? saved.no
        : defaultPeriodNo(period);
    return { year, period, no };
  } catch {
    return fallback;
  }
}

const savedView = readBookView();
const bookYear = ref(savedView.year);
const bookPeriod = ref<BookPeriod>(savedView.period);
const bookPeriodNo = ref(savedView.no);

// Đổi quý ↔ tháng mà số kỳ vượt loại kỳ mới (vd Tháng 12 → Theo quý) thì đưa
// về số kỳ hợp lệ, không để loadBook bắn một quý/tháng không tồn tại.
watch(bookPeriod, (period) => {
  const max = period === "quarter" ? 4 : 12;
  if (bookPeriodNo.value < 1 || bookPeriodNo.value > max)
    bookPeriodNo.value = defaultPeriodNo(period);
});

// Nhớ lựa chọn trên tab Sổ qua các lần mở lại app.
watch([bookYear, bookPeriod, bookPeriodNo], ([year, period, no]) => {
  localStorage.setItem(BOOK_VIEW_KEY, JSON.stringify({ year, period, no }));
});

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
    try {
      await downloadBlob(`so-ke-toan-${b.book}-${bookYear.value}.xlsx`, await buildTaxBookXlsx(b));
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
    bookYearOptions,
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
