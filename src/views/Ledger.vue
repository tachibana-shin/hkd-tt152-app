<script setup lang="ts">
import { storeToRefs } from "pinia";
import { useBusinessStore } from "@/stores/business";
import { api } from "@/db";
import type { LedgerRow } from "@/types";
import { fmtInt as fmt, fmtVnd } from "@/utils/format";

const business = useBusinessStore();
const { config } = storeToRefs(business);
const toast = useToast();

const loading = ref(false);
const rows = ref<LedgerRow[]>([]);

const fromDate = ref<Date | null>(null);
const toDate = ref<Date | null>(null);
const entryType = ref("");

const entryOptions = [{ label: "Toàn bộ", value: "" }, "PN", "PX", "PT", "PC"];

const iso = (d: Date | null) => (d ? d.toISOString().slice(0, 10) : "");
const currentYear = () => new Date().getFullYear();
async function loadLedger() {
  loading.value = true;
  try {
    const f = iso(fromDate.value) || `${currentYear()}-01-01`;
    const t = iso(toDate.value) || `${currentYear()}-12-31`;
    rows.value = await api.getLedger(f, t);
  } catch (e) {
    toast.add({
      severity: "error",
      summary: "Lỗi tải sổ nhật ký",
      detail: String(e),
    });
  } finally {
    loading.value = false;
  }
}

// backend get_ledger chỉ lọc theo ngày → lọc loại phiếu ở client
const filteredRows = computed(() =>
  entryType.value ? rows.value.filter((r) => r.entry_type === entryType.value) : rows.value,
);

const totalAmount = computed(() => filteredRows.value.reduce((s, r) => s + r.amount, 0));

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

onMounted(async () => {
  if (!config.value) await business.load();
  await loadLedger();
});
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
        <Button label="Tải lại" icon="pi pi-refresh" @click="loadLedger" />
      </template>
    </Toolbar>

    <Card>
      <template #content>
        <div class="flex flex-wrap items-end gap-3">
          <div>
            <label class="text-xs text-gray-500 block mb-1">Từ ngày</label>
            <DatePicker v-model="fromDate" dateFormat="dd/mm/yy" class="w-44" />
          </div>
          <div>
            <label class="text-xs text-gray-500 block mb-1">Đến ngày</label>
            <DatePicker v-model="toDate" dateFormat="dd/mm/yy" class="w-44" />
          </div>
          <div>
            <label class="text-xs text-gray-500 block mb-1">Loại phiếu</label>
            <div class="flex items-center gap-2">
              <i class="pi pi-filter text-gray-400" />
              <Select v-model="entryType" :options="entryOptions" class="w-40" />
            </div>
          </div>
          <Button label="Xem báo cáo" icon="pi pi-search" size="small" @click="loadLedger" />
          <span v-if="config" class="text-sm text-gray-500 ml-auto">
            <i class="pi pi-calendar mr-1" />Kỳ mặc định: {{ currentYear() }} (theo năm hiện tại)
          </span>
        </div>
      </template>
    </Card>

    <Card>
      <template #content>
        <AppDataTable :value="filteredRows" :loading="loading" stripedRows paginator :rows="15">
          <Column field="posting_date" header="Ngày" />
          <Column field="voucher_no" header="Số phiếu" />
          <Column field="entry_type" header="Loại">
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
        <div v-if="filteredRows.length" class="mt-4 flex justify-end gap-8 text-sm border-t pt-3">
          <span class="text-gray-500">Tổng số tiền:</span>
          <b class="text-primary-600">{{ fmt(totalAmount) }} đ</b>
        </div>
      </template>
    </Card>
  </div>
</template>
