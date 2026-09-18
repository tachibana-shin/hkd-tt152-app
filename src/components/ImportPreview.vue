<script setup lang="ts">
import type { ImportItem } from "@/utils/importExcel";
import { typeSeverity } from "@/utils/importExcel";
import { fmtDec as fmt2, fmtVnd } from "@/utils/format";

defineProps<{
  rows: ImportItem[];
  candCount: number;
  skipCount: number;
}>();
</script>

<template>
  <div class="mb-3 flex flex-wrap items-center gap-2">
    <Tag :value="`Đã đọc: ${candCount} dòng`" severity="info" />
    <Tag :value="`Nhận diện được: ${rows.length} dòng`" severity="success" />
    <Tag :value="`Bỏ qua: ${skipCount} dòng`" severity="secondary" />
  </div>
  <AppDataTable :value="rows" :rows="20" paginator stripedRows size="small">
    <Column field="voucher_no" header="Số phiếu" />
    <Column field="posting_date" header="Ngày ghi sổ" />
    <Column field="entry_type" header="Loại">
      <template #body="{ data }">
        <Tag
          :value="data.entry_type"
          :severity="typeSeverity(data.entry_type)"
        />
      </template>
    </Column>
    <Column field="description" header="Diễn giải" />
    <Column field="customer_code" header="Mã KH" />
    <Column field="supplier_code" header="Mã NCC" />
    <Column field="product_code" header="Mã VT" />
    <Column field="quantity" header="SL" align="right" />
    <Column field="unit_price" header="Đơn giá" align="right">
      <template #body="{ data }">{{ fmt2(data.unit_price) }}</template>
    </Column>
    <Column field="amount" header="Số tiền" align="right">
      <template #body="{ data }">{{ fmtVnd(data.amount) }}</template>
    </Column>
    <Column field="industry_code" header="Nhóm ngành" />
    <Column field="tax_period" header="Kỳ thuế" />
    <template #empty>
      <EmptyState
        text="Không có dòng dữ liệu nào được nhận diện."
        icon="pi pi-file-excel"
      />
    </template>
  </AppDataTable>
</template>
