<script setup lang="ts">
import { storeToRefs } from "pinia";
import { useBusinessStore } from "@/stores/business";
import { api } from "@/db";
import type { LedgerRow } from "@/types";
import { fmtInt as fmt, fmtVnd, toIsoDate } from "@/utils/format";
import { useKeepAliveRefresh } from "@/composables/useKeepAliveRefresh";
import { useLazyPage } from "@/composables/useLazyPage";
import { useInheritedRange } from "@/composables/useInheritedRange";

const business = useBusinessStore();
const { config } = storeToRefs(business);
const toast = useToast();

const loading = ref(false);
const rows = ref<LedgerRow[]>([]);
const total = ref(0);
/** Tổng số tiền CẢ kỳ lọc (do server tính, không phải tổng của trang đang xem). */
const totalAmount = ref(0);

// Khoảng ngày mở từ màn Kế toán HKD (query ?from=&to=) — mở ra đúng kỳ đang xem.
const { inheritedFrom, inheritedTo, backLabel, goBack } = useInheritedRange();
const fromDate = ref<Date | null>(inheritedFrom);
const toDate = ref<Date | null>(inheritedTo);
const entryType = ref("");
/** true = khoảng ngày đến từ màn khác, báo để người dùng biết vì sao sổ không cả năm. */
const rangeInherited = computed(() => !!inheritedFrom || !!inheritedTo);

const entryOptions = [{ label: "Toàn bộ", value: "" }, "PN", "PX", "PT", "PC"];

const currentYear = () => new Date().getFullYear();
const range = () => ({
  fromDate: toIsoDate(fromDate.value) || `${currentYear()}-01-01`,
  toDate: toIsoDate(toDate.value) || `${currentYear()}-12-31`,
  entryType: entryType.value,
});

// Sổ nhật ký theo năm là danh sách dài nhất trong app: mỗi bút toán một dòng.
// Kéo cả năm về client rồi cắt trang khiến mở màn đơ, nên chuyển sang phân trang
// server-side (lọc ngày + loại phiếu + tìm kiếm đều do server xử lý).
const page = useLazyPage(async (lazy) => {
  loading.value = true;
  try {
    const res = await api.getLedgerPage(lazy, range().fromDate, range().toDate, range().entryType);
    rows.value = res.rows;
    total.value = res.total;
    totalAmount.value = res.total_amount ?? 0;
  } catch (e) {
    toast.add({
      severity: "error",
      summary: "Lỗi tải sổ nhật ký",
      detail: String(e),
    });
  } finally {
    loading.value = false;
  }
});

/** Đổi bộ lọc riêng (ngày / loại phiếu) → nạp lại trang đầu. */
function applyFilters() {
  return page.setExtra({});
}

/** Bỏ khoảng ngày kế thừa → xem lại cả năm. */
function clearInheritedRange() {
  fromDate.value = null;
  toDate.value = null;
  return applyFilters();
}

function tagSeverity(et: string): "success" | "info" | "warning" | "danger" {
  switch (et) {
    case "PN":
      return "success";
    case "PX":
      return "info";
    case "PT":
      return "warning";
    default:
      return "danger"; // PC
  }
}

async function reload() {
  if (!config.value) await business.load();
  await applyFilters();
}

void reload();
// Quay lại màn (KeepAlive giữ state) → nạp lại dữ liệu cho khỏi cũ.
useKeepAliveRefresh(reload);
</script>

<template>
  <div class="space-y-4">
    <Toolbar>
      <template #start>
        <div class="flex items-center gap-2">
          <i-mdi-book-open-page-variant class="text-xl text-sky-500" />
          <h3 class="font-semibold">Sổ nhật ký chung</h3>
        </div>
      </template>
      <template #end>
        <div class="flex items-center gap-2">
          <Button
            :label="`Quay lại ${backLabel || 'màn trước'}`"
            icon="pi pi-arrow-left"
            severity="secondary"
            outlined
            data-testid="back-to-accounting"
            @click="goBack()"
          />
          <Button label="Tải lại" icon="pi pi-refresh" @click="applyFilters()" />
        </div>
      </template>
    </Toolbar>

    <Card>
      <template #content>
        <div class="flex flex-wrap items-end gap-3">
          <div>
            <label class="text-xs text-gray-500 block mb-1">Từ ngày</label>
            <DatePicker
              v-model="fromDate"
              dateFormat="dd/mm/yy"
              class="w-44"
              @update:model-value="applyFilters()"
            />
          </div>
          <div>
            <label class="text-xs text-gray-500 block mb-1">Đến ngày</label>
            <DatePicker
              v-model="toDate"
              dateFormat="dd/mm/yy"
              class="w-44"
              @update:model-value="applyFilters()"
            />
          </div>
          <div v-if="rangeInherited" class="flex items-center gap-2 pb-1">
            <Tag
              value="Đang xem đúng khoảng ngày của màn trước"
              severity="info"
              data-testid="ledger-range-inherited"
            />
            <Button label="Xem cả năm" size="small" text @click="clearInheritedRange" />
          </div>
          <div>
            <label class="text-xs text-gray-500 block mb-1">Loại phiếu</label>
            <div class="flex items-center gap-2">
              <i class="pi pi-filter text-gray-400" />
              <Select v-model="entryType" :options="entryOptions" class="w-40" />
            </div>
          </div>
          <Button label="Xem báo cáo" icon="pi pi-search" size="small" @click="applyFilters()" />
          <span v-if="config" class="text-sm text-gray-500 ml-auto">
            <i class="pi pi-calendar mr-1" />Kỳ mặc định: {{ currentYear() }} (theo năm hiện tại)
          </span>
        </div>
      </template>
    </Card>

    <Card>
      <template #content>
        <AppDataTable
          :value="rows"
          :loading="loading"
          :totalRecords="total"
          :first="page.first.value"
          :rows="page.rowsPerPage.value"
          :rows-per-page-options="page.pageSizes"
          data-key="id"
          sort-mode="single"
          removable-sort
          filter-toggle
          :global-filter-fields="[
            'voucher_no',
            'description',
            'product_code',
            'debit_account',
            'credit_account',
          ]"
          stripedRows
          @page="page.onPage"
          @sort="page.onSort"
          @search="page.onSearch"
        >
          <Column field="posting_date" header="Ngày" sortable />
          <Column field="voucher_no" header="Số phiếu" sortable />
          <Column field="entry_type" header="Loại" sortable>
            <template #body="{ data }">
              <Tag :value="data.entry_type" :severity="tagSeverity(data.entry_type)" />
            </template>
          </Column>
          <Column field="description" header="Diễn giải" />
          <Column field="product_code" header="Mã VT">
            <template #body="{ data }">{{ data.product_code || "—" }}</template>
          </Column>
          <Column field="debit_account" header="TK Nợ" />
          <Column field="credit_account" header="TK Có" />
          <Column field="quantity" header="SL" align="right">
            <template #body="{ data }">{{ fmt(data.quantity) }}</template>
          </Column>
          <Column field="unit_price" header="Đơn giá" align="right">
            <template #body="{ data }">{{ fmtVnd(data.unit_price) }}</template>
          </Column>
          <Column field="amount" header="Số tiền" align="right">
            <template #body="{ data }">
              <b>{{ fmtVnd(data.amount) }}</b>
            </template>
          </Column>
          <template #empty
            ><EmptyState text="Chưa có phát sinh trong kỳ." icon="pi pi-list"
          /></template>
        </AppDataTable>
        <div v-if="total" class="mt-4 flex justify-end gap-8 text-sm border-t pt-3">
          <span class="text-gray-500">
            Tổng {{ total.toLocaleString("vi-VN") }} dòng trong kỳ — tổng số tiền:
          </span>
          <b class="text-primary-600">{{ fmt(totalAmount) }} đ</b>
        </div>
      </template>
    </Card>
  </div>
</template>
