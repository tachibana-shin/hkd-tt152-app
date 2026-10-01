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

const hasData = computed(() => props.book.rows.length > 0);
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

    <div class="book-unit">Đơn vị tính: {{ book.header.unit }}</div>

    <table class="book-table">
      <thead>
        <tr>
          <th
            v-for="col in book.columns"
            :key="col.key"
            :style="`width:${col.width};text-align:${align(col)}`"
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
            {{ render(row, col) }}
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
.book {
  font-size: 0.75rem;
  color: var(--p-text-color);
}

.book-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 1rem;
  border: 1px solid var(--p-border-color);
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
  border: 1px solid var(--p-border-color);
  padding: 0.2rem 0.35rem;
  vertical-align: top;
  word-break: break-word;
}

.book-table th {
  background: var(--p-surface-100);
  font-weight: 600;
  font-size: 0.7rem;
}

.book-label {
  font-weight: 600;
}

.book-row-section td {
  background: var(--p-surface-100);
  font-weight: 700;
  text-transform: uppercase;
  font-size: 0.7rem;
}

.book-row-subtotal td {
  background: color-mix(in srgb, var(--p-surface-100) 55%, transparent);
}

.book-row-total td {
  font-weight: 700;
  border-top: 1px solid var(--p-text-color);
}

.book-empty {
  border: 1px solid var(--p-border-color);
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
  .book-table th {
    background: #eee !important;
  }

  .book-row-section td,
  .book-row-subtotal td {
    background: #f6f6f6 !important;
  }
}
</style>
