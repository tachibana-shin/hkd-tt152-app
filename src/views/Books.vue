<script setup lang="ts">
import { toIsoDate } from "@/utils/format";
import { monthRange, quarterRange, type PeriodRange } from "@/utils/period";
import { useKeepAliveRefresh } from "@/composables/useKeepAliveRefresh";
import { useTaxBooks } from "@/composables/useTaxBooks";

/**
 * Tab "Sổ kế toán" — xem sổ theo mẫu TT 152/2025/TT-BTC, tách khỏi màn Kế
 * toán HKD cho bớt dày: tab này chỉ có chọn kỳ/mẫu sổ, bảng sổ và xuất file;
 * tờ khai, tổng hợp thuế, quyết toán vẫn ở màn Kế toán.
 *
 * Kỳ sổ dùng chung state với màn Kế toán (composable) — "Chuyển nhanh" bên kia
 * sang đây là đúng kỳ, không cần đồng bộ thêm.
 */
const {
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
} = useTaxBooks();

const router = useRouter();
const route = useRoute();

/**
 * Kỳ của sổ dưới dạng khoảng ngày — đẩy sang Sổ nhật ký / Thu-Chi cho đúng kỳ
 * đang xem (trước đây lấy khoảng ngày trên màn Kế toán HKD).
 */
function bookRange(): PeriodRange {
  if (bookPeriod.value === "quarter") return quarterRange(bookYear.value, bookPeriodNo.value);
  if (bookPeriod.value === "month") return monthRange(bookYear.value, bookPeriodNo.value);
  return { from: new Date(bookYear.value, 0, 1), to: new Date(bookYear.value, 11, 31) };
}

/**
 * Mở Sổ nhật ký (`/ledger`) hoặc Thu / Chi (`/cash`) kèm đúng kỳ sổ đang xem.
 *
 * Hai màn này không phải mẫu TT 152 nên không gộp vào thẻ sổ, nhưng vẫn phải có
 * lối vào có chủ đích từ đây (trước chỉ vào được bằng thanh menu). Truyền
 * `?from=&to=` + đường quay lại, màn đích nhận qua `useInheritedRange` → mở ra
 * là đúng kỳ, bấm "Quay lại Sổ kế toán" là về ngay chỗ này.
 */
function openJournal(path: "/ledger" | "/cash") {
  const { from, to } = bookRange();
  void router.push({
    path,
    query: {
      from: toIsoDate(from),
      to: toIsoDate(to),
      back: route.fullPath,
      backLabel: "Sổ kế toán",
    },
  });
}

/** In sổ đang xem — cùng đường in như nút "In báo cáo" trên màn Kế toán. */
function printBook() {
  window.print();
}

// Mở tab là nạp: danh sách mẫu theo hồ sơ + sổ đang chọn.
void refreshBooks();
// Quay lại tab (KeepAlive giữ state) → nạp lại: kỳ có thể đã đổi ở màn Kế toán.
useKeepAliveRefresh(refreshBooks);
</script>

<template>
  <div class="space-y-4">
    <Toolbar>
      <template #start>
        <div class="flex items-center gap-2">
          <i-mdi-book-open-page-variant class="text-xl text-sky-500" />
          <h3 class="font-semibold">Sổ kế toán hộ kinh doanh</h3>
        </div>
      </template>
      <template #end>
        <Button label="Tải lại sổ" icon="pi pi-refresh" @click="refreshBooks" />
      </template>
    </Toolbar>

    <!-- Sổ nhanh đúng hồ sơ (Điều 4 TT 152) + lối sang 2 sổ không phải mẫu -->
    <SectionCard title="Mở sổ nhanh">
      <template #icon><i-mdi-book-open-page-variant class="text-sky-500" /></template>
      <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
        <!-- Nút sổ nhanh chỉ hiện mẫu đúng hồ sơ (Điều 4 TT 152). -->
        <Button
          v-if="bookVisible('S1a')"
          label="Sổ S1a-HKD"
          icon="pi pi-book"
          outlined
          class="justify-start"
          data-testid="open-book-s1a"
          @click="selectBook('S1a')"
        />
        <Button
          v-if="bookVisible('S2a')"
          label="Sổ S2a-HKD"
          icon="pi pi-book"
          outlined
          class="justify-start"
          data-testid="open-book-s2a"
          @click="selectBook('S2a')"
        />
        <Button
          v-if="bookVisible('S2b')"
          label="Sổ S2b-HKD"
          icon="pi pi-book"
          outlined
          class="justify-start"
          data-testid="open-book-s2b"
          @click="selectBook('S2b')"
        />
        <Button
          v-if="bookVisible('S3a')"
          label="Sổ S3a-HKD"
          icon="pi pi-book"
          outlined
          class="justify-start"
          data-testid="open-book-s3a"
          @click="selectBook('S3a')"
        />
        <!-- 2 màn không phải mẫu TT 152: vẫn cho mở thẳng kèm kỳ đang xem. -->
        <Button
          label="Mở Sổ nhật ký"
          icon="pi pi-external-link"
          outlined
          class="justify-start"
          data-testid="open-ledger"
          @click="openJournal('/ledger')"
        />
        <Button
          label="Mở Thu / Chi"
          icon="pi pi-external-link"
          outlined
          class="justify-start"
          data-testid="open-cash"
          @click="openJournal('/cash')"
        />
      </div>
    </SectionCard>

    <!-- Sổ kế toán (mẫu TT 152/2025/TT-BTC) -->
    <SectionCard>
      <template #icon><i-mdi-book-open-variant class="text-cyan-600" /></template>
      <template #title>
        <span>Sổ kế toán (mẫu TT 152/2025/TT-BTC)</span>
      </template>
      <template #actions>
        <Button
          label="In sổ"
          icon="pi pi-print"
          size="small"
          outlined
          title="In sổ đang xem (khổ giấy theo bảng sổ)"
          @click="printBook"
        />
        <Button
          label="Xuất Excel"
          icon="pi pi-file-excel"
          size="small"
          outlined
          :disabled="!bookData?.rows.length"
          @click="exportBookExcel"
        />
        <Button
          label="Xuất Word"
          icon="pi pi-file-word"
          size="small"
          outlined
          title="Xuất sổ ra file Word (.docx) khổ A4 dọc"
          :disabled="!bookData?.rows.length"
          @click="exportBookWord"
        />
      </template>
      <div class="mb-3 flex flex-wrap items-end gap-3">
        <div>
          <label class="text-xs text-gray-500 block mb-1">Loại kỳ sổ</label>
          <Select
            v-model="bookPeriod"
            :options="bookPeriodTypeOptions"
            optionLabel="label"
            optionValue="value"
            class="w-36"
            aria-label="Loại kỳ sổ"
            @change="loadBook"
          />
        </div>
        <div v-if="bookPeriod !== 'year'">
          <label class="text-xs text-gray-500 block mb-1">Kỳ</label>
          <Select
            v-model="bookPeriodNo"
            :options="bookPeriodNoOptions"
            optionLabel="label"
            optionValue="value"
            class="w-32"
            aria-label="Kỳ sổ"
            @change="loadBook"
          />
        </div>
        <div class="min-w-96">
          <label class="text-xs text-gray-500 block mb-1">Mẫu sổ</label>
          <div data-testid="tax-book" class="w-full">
            <Select
              v-model="book"
              :options="bookOptions"
              option-label="label"
              option-value="value"
              class="w-full"
              aria-label="Mẫu sổ"
              @change="loadBook"
            />
          </div>
        </div>
        <Button
          label="Xem sổ"
          icon="pi pi-search"
          size="small"
          :loading="bookLoading"
          class="mt-4"
          @click="loadBook"
        />
        <!-- Mặc định chỉ hiện sổ đúng nhóm hộ; bật lên để đối chiếu đủ 7 mẫu in. -->
        <div class="flex items-center gap-2 mt-4">
          <ToggleSwitch
            :model-value="showAllBooks"
            input-id="show-all-books"
            data-testid="show-all-books"
            aria-label="Xem tất cả 7 mẫu sổ"
            @update:model-value="toggleShowAllBooks"
          />
          <label for="show-all-books" class="text-xs text-gray-500 cursor-pointer">
            Xem tất cả 7 mẫu sổ
          </label>
        </div>
      </div>
      <TaxBookTable
        v-if="bookData"
        :book="bookData"
        :loading="bookLoading"
        @switch-book="selectBook"
      />
    </SectionCard>
  </div>
</template>
