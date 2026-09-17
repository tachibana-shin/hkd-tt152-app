<script setup lang="ts">
import type { ImportItem } from "@/utils/importExcel";
import { typeSeverity } from "@/utils/importExcel";
import { fmtInt as fmt, fmtDec as fmt2 } from "@/utils/format";

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
  <DataTable :value="rows" :rows="20" paginator stripedRows size="small">
    <Column field="voucher_no" header="Số phiếu" style="width: 110px" />
    <Column field="posting_date" header="Ngày ghi sổ" style="width: 110px" />
    <Column field="entry_type" header="Loại" style="width: 90px">
      <template #body="{ data }">
        <Tag :value="data.entry_type" :severity="typeSeverity(data.entry_type)" />
      </template>
    </Column>
    <Column field="description" header="Diễn giải" />
    <Column field="customer_code" header="Mã KH" style="width: 90px" />
    <Column field="supplier_code" header="Mã NCC" style="width: 90px" />
    <Column field="product_code" header="Mã VT" style="width: 90px" />
    <Column field="quantity" header="SL" style="width: 70px" align="right" />
    <Column field="unit_price" header="Đơn giá" style="width: 110px" align="right">
      <template #body="{ data }">{{ fmt2(data.unit_price) }}</template>
    </Column>
    <Column field="amount" header="Số tiền" style="width: 130px" align="right">
      <template #body="{ data }">{{ fmt(data.amount) }}</template>
    </Column>
    <Column field="industry_code" header="Nhóm ngành" style="width: 100px" />
    <Column field="tax_period" header="Kỳ thuế" style="width: 90px" />
    <template #empty>
      <EmptyState text="Không có dòng dữ liệu nào được nhận diện." icon="pi pi-file-excel" />
    </template>
  </DataTable>
</template>
