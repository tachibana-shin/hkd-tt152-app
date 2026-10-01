<script setup lang="ts">
/**
 * TaxBookTable — hiển thị một mẫu sổ kế toán TT 152/2025/TT-BTC.
 *
 * Bảng dựng bằng thẻ `<table>` thuần chứ không dùng `AppDataTable`: đây là mẫu
 * biểu in ra nộp cơ quan thuế, cần giữ đúng độ rộng cột, không có chọn dòng /
 * phân trang / sắp xếp như các bảng nghiệp vụ khác.
 *
 * Bộ cột do backend gửi kèm (`columns`) vì mỗi mẫu một bộ: S1a có 3 cột,
 * S2d có 12, S3a có 12. Ô không có dữ liệu là `null` — in ô trống đúng như mẫu
 * thay vì ghi "0" vào chỗ không có số.
 */
import { computed } from "vue";
import Button from "primevue/button";
import type { BookCell, BookColumn, TaxBook } from "@/types";
import { fmtDec, fmtInt, fmtPct } from "@/utils/format";

const props = defineProps<{ book: TaxBook; loading?: boolean }>();

const emit = defineEmits<{ switchBook: [book: string] }>();

/** Ô trống của mẫu → chuỗi rỗng (không in gì). */
function cell(row: { cells: Record<string, BookCell> }, key: string): BookCell {
  return row.cells[key] ?? null;
}

/**
 * Định dạng theo kiểu cột do backend khai báo.
 *
 * `rate` nhận cả hai dạng: tỉ lệ phần trăm dạng thập phân (0.01 → `1%`) và số
 * phần trăm (5 → `5%`) — tuỳ chỗ nào ghi vào.
 */
function render(row: { cells: Record<string, BookCell> }, col: BookColumn): string {
  const v = cell(row, col.key);
  if (v === null || v === "") return "";
  if (typeof v === "string") return v;
  switch (col.kind) {
    // Sổ kế toán VN ghi bằng đồng nguyên — backend đã làm tròn, fmtInt chỉ việc
    // định dạng theo dấu phẩy/thousand của tiếng Việt.
    case "money":
      return fmtInt(v);
    case "qty":
      return fmtDec(v);
    case "rate":
      return Math.abs(v) <= 1 ? fmtPct(v) : `${fmtInt(v)}%`;
    default:
      return fmtDec(v);
  }
}

/** Cột số luôn căn phải; cột chữ căn trái. */
function align(col: BookColumn): string {
  return col.kind === "text" ? "left" : "right";
}

/** Ô đang âm (chi phí, số dư âm) tô đỏ để dễ soi khi đối chiếu. */
function isNegative(row: { cells: Record<string, BookCell> }, col: BookColumn): boolean {
  if (col.kind === "text") return false;
  const v = cell(row, col.key);
  return typeof v === "number" && v < 0;
}

const rowClass = computed(() => (kind: string) => {
  switch (kind) {
    case "section":
      return "book-row-section";
    case "subtotal":
      return "book-row-subtotal";
    case "total":
      return "book-row-total";
    default:
      return "";
  }
});

/** Cột đầu tiên mang nhãn dòng khi mẫu không có cột "Diễn giải" riêng. */
const labelColumn = computed(
  () => props.book.columns.find((c) => c.kind === "text" && c.key !== "a" && c.key !== "b")?.key,
);

/**
 * Cắt chuỗi ô ở dấu `/` để chèn `<wbr>` — mở cơ hội ngắt dòng ngay tại đó.
 *
 * Cột hẹp (màn 638px, cột "Ngày, tháng" ~72px) thì `27/09/2026` vốn bị bẻ giữa
 * chữ số ra `27/09/202` + `6`; có `<wbr>` thì trình duyệt xuống dòng ở dấu gạch
 * thành `27/09/` + `2026`, còn chỗ rộng thì vẫn nằm gọn một dòng. Không có `/`
 * thì chuỗi không đổi (trở lại một đoạn duy nhất).
 */
function cellParts(value: string): string[] {
  const segs = value.split("/");
  return segs.map((s, i) => (i < segs.length - 1 ? `${s}/` : s));
}

const hasData = computed(() => props.book.rows.length > 0);

/**
 * Độ rộng cột do backend khai theo `rem`, đúng tỉ lệ mẫu in (S1a: 7-7-24-10).
 *
 * Hai kiểu viết đều hỏng một bên: cộng thẳng các `rem` đó lại thì cửa sổ hẹp
 * bị kéo tràn ngang, còn kẹp `max-width` bằng tổng `rem` thì cửa sổ rộng lại
 * hở một khoảng trắng lớn bên phải. Về phần trăm trên tổng là lối chung: tỉ lệ
 * giữa các cột giữ nguyên đúng như mẫu in, mà bảng luôn chạy hết khung — hẹp thì
 * cột co lại cho chữ tự xuống dòng, rộng thì cột giãn ra lấp đầy, không hở.
 */
const columnWidths = computed(() => {
  const cols = props.book.columns;
  const rem = cols.map((c) => parseFloat(c.width) || 0);
  const total = rem.reduce((a, b) => a + b, 0);
  // Không khai độ rộng (hoặc khai lỗi) thì chia đều, vẫn không được tràn.
  if (total <= 0) return rem.map(() => `${(100 / Math.max(cols.length, 1)).toFixed(2)}%`);
  return rem.map((w) => `${((w / total) * 100).toFixed(2)}%`);
});
</script>

<template>
  <div data-testid="tax-book-table" class="book">
    <!-- Đầu sổ: tên hộ ở trái, khối "Mẫu số …" in nghiêng ở góc phải (đúng mẫu in) -->
    <div class="book-head">
      <div class="book-head-owner">
        <span>HỘ, CÁ NHÂN KINH DOANH: {{ book.header.owner || "…" }}</span>
        <span>Địa chỉ: {{ book.header.address || "…" }}</span>
        <span>Mã số thuế: {{ book.header.tax_code || "…" }}</span>
      </div>
      <span class="book-head-form">{{ book.header.form_ref }}</span>
    </div>

    <h4 class="book-title">{{ book.title }}</h4>

    <!-- Hai dòng dưới tiêu đề: tên địa điểm kinh doanh (hoặc tên vật liệu) + kỳ kê khai -->
    <div class="book-sub">
      <span v-if="book.header.location_label">
        {{ book.header.location_label }}: {{ book.header.location || "…" }}
      </span>
      <span>Kỳ kê khai: {{ book.header.period || book.period_label }}</span>
    </div>

    <!-- Mẫu S2d không có dòng này ở đầu sổ (đơn vị tính nằm trong cột D) -->
    <div v-if="book.header.unit" class="book-unit">Đơn vị tính: {{ book.header.unit }}</div>

    <table class="book-table">
      <thead>
        <tr>
          <th
            v-for="(col, i) in book.columns"
            :key="col.key"
            :style="`width:${columnWidths[i]};text-align:${align(col)}`"
          >
            {{ col.label }}
          </th>
        </tr>
      </thead>
      <tbody v-if="hasData">
        <tr v-for="(row, i) in book.rows" :key="i" :class="rowClass(row.kind)">
          <td
            v-for="col in book.columns"
            :key="col.key"
            :style="`text-align:${align(col)}`"
            :class="{
              'text-rose-600': isNegative(row, col),
              'book-label': col.key === labelColumn,
            }"
          >
            <template v-for="(part, k) in cellParts(render(row, col))" :key="k"
              >{{ part }}<wbr
            /></template>
          </td>
        </tr>
      </tbody>
    </table>

    <p v-if="!hasData && !loading" class="book-empty">Kỳ này chưa có phát sinh nào để ghi sổ.</p>

    <!-- Chữ ký cuối sổ -->
    <div class="book-sign">
      <span>{{ book.header.sign_date ? `Ngày ${book.header.sign_date}` : "" }}</span>
      <span>{{ book.header.signer }}</span>
      <span class="book-sign-note">(Ký, ghi rõ họ tên và đóng dấu nếu có)</span>
    </div>

    <!-- Ghi chú: cảnh báo chọn nhầm mẫu có nút chuyển sang mẫu đúng -->
    <div v-if="book.notes.length" class="book-notes">
      <div
        v-for="(n, i) in book.notes"
        :key="i"
        class="book-note"
        :class="n.level === 'warn' ? 'book-note-warn' : ''"
      >
        <i
          :class="n.level === 'warn' ? 'pi pi-exclamation-triangle' : 'pi pi-info-circle'"
          class="text-xs"
          aria-hidden="true"
        />
        <span>{{ n.text }}</span>
        <Button
          v-if="n.action_book && n.action_label"
          :label="n.action_label"
          size="small"
          text
          @click="emit('switchBook', n.action_book!)"
        />
      </div>
    </div>
  </div>
</template>

<style scoped>
/* Khung sổ in ra nộp cơ quan thuế: đường viền mảnh, chữ nhỏ, không bo góc. */
/*
 * Hai màu riêng của khung sổ, tự đổi theo giao diện sáng / tối:
 *
 * - `--book-line` — màu kẻ ô. `--p-border-color` **không tồn tại** trong PrimeVue
 *   nên trước đây `var(--p-border-color)` rơi về `currentColor` = màu chữ →
 *   dark mode kẻ ô trắng bệt trên nền tối. Giữ `#334155` (đúng màu lưới cũ ở
 *   giao diện sáng) rồi override sang xám chìm bên dưới khi `.dark`.
 *   Không viết bằng `light-dark(...)`: `vite build` hạ cấp nó thành
 *   `var(--lightningcss-light, …)var(--lightningcss-dark, …)` — hai token dính
 *   nhau ra màu hỏng nên khung viền lại rơi về `currentColor`.
 * - `--book-band` — nền hàng cột và hàng khối. `--p-surface-100` là **bảng màu cố
 *   định** (`#f4f4f5` kể cả khi đang dark) nên chữ trắng đè lên thành "trắng
 *   xóa", không đọc nổi. `--p-content-hover-background` là token đổi theo giao
 *   diện (sáng `#f1f5f9`, tối `#27272a`) và PrimeVue chèn CSS này lúc chạy nên
 *   không bị hạ cấp khi build.
 */
.book {
  --book-line: #334155;
  --book-band: var(--p-content-hover-background);

  font-size: 0.75rem;
  color: var(--p-text-color);
}

/* dark mode: kẻ ô xám chìm thay vì trắng bệt. `html.dark` cho đặc hiệu cao hơn
   rule scoped phía trên nên không phụ thuộc thứ tự (viết một selector global
   trọn vẹn — viết `:global(html.dark) .book` thì vue-sfc cắt mất `.book`). */
:global(html.dark .book) {
  --book-line: #3f3f46;
}

.book-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 1rem;
  border: 1px solid var(--book-line);
  border-bottom: 0;
  padding: 0.4rem 0.5rem;
}

.book-head-owner {
  display: flex;
  flex-direction: column;
  gap: 0.15rem;
  line-height: 1.5;
}

/* Khối "Mẫu số Sxx-HKD (Kèm theo Thông tư …)" in nghiêng, canh phải như mẫu in. */
.book-head-form {
  font-style: italic;
  text-align: right;
  max-width: 24rem;
  line-height: 1.35;
}

.book-title {
  text-align: center;
  font-size: 0.85rem;
  font-weight: 700;
  text-transform: uppercase;
  margin: 0.6rem 0 0.35rem;
}

/* Tên địa điểm kinh doanh + kỳ kê khai — canh giữa ngay dưới tiêu đề. */
.book-sub {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.15rem;
  text-align: center;
  line-height: 1.5;
}

/* "Đơn vị tính:" in nghiêng canh phải, ngay trên bảng như mẫu gốc. */
.book-unit {
  font-style: italic;
  text-align: right;
  padding: 0.35rem 0.15rem;
}

.book-table {
  width: 100%;
  border-collapse: collapse;
  table-layout: fixed;
}

.book-table th,
.book-table td {
  border: 1px solid var(--book-line);
  /* Ngang 0.2rem thay vì 0.35rem: cột ngày 7rem chỉ thiếu ~3px nên ngày
     `27/09/2026` phải xuống dòng dù cột đã chia đúng tỉ lệ mẫu. */
  padding: 0.2rem 0.2rem;
  vertical-align: top;
  word-break: break-word;
}

.book-table th {
  background: var(--book-band);
  font-weight: 600;
  font-size: 0.7rem;
}

.book-label {
  font-weight: 600;
}

.book-row-section td {
  background: var(--book-band);
  font-weight: 700;
  text-transform: uppercase;
  font-size: 0.7rem;
}

.book-row-subtotal td {
  background: color-mix(in srgb, var(--book-band) 55%, transparent);
}

.book-row-total td {
  font-weight: 700;
  /* Cùng màu kẻ ô thay vì `currentColor`: dark mode trước đây ra đường kẻ trắng
     bệt, còn khi in thì theo `--book-line` (vốn đã là màu đậm trên nền trắng). */
  border-top: 1px solid var(--book-line);
}

.book-empty {
  border: 1px solid var(--book-line);
  border-top: 0;
  padding: 0.75rem;
  text-align: center;
  color: var(--p-text-muted-color);
}

.book-sign {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.1rem;
  padding: 0.75rem 0 0.25rem;
  font-style: italic;
}

.book-sign-note {
  font-size: 0.65rem;
  opacity: 0.8;
}

.book-notes {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
  margin-top: 0.5rem;
}

.book-note {
  display: flex;
  align-items: center;
  gap: 0.35rem;
  font-size: 0.7rem;
  color: var(--p-text-muted-color);
}

.book-note-warn {
  color: var(--p-amber-600);
}

@media print {
  /*
   * In từ giao diện tối vẫn phải ra mẫu đen trên trắng: ép nền lại bằng xám
   * nhạt (đúng mẫu gốc) và ép chữ về đen. Không ép thì dark mode in ra chữ
   * trắng trên nền trắng — cơ quan thuế không đọc được.
   */
  .book {
    color: #111;
  }

  .book-table th {
    background: #eee !important;
    color: #111 !important;
  }

  .book-row-section td,
  .book-row-subtotal td {
    background: #f6f6f6 !important;
    color: #111 !important;
  }
}
</style>
