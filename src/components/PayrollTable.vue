<script setup lang="ts">
import type { Employee, PayrollRow } from "@/types";
import { fmtInt as fmt } from "@/utils/format";
import { type PayrollPreview, previewPayroll } from "@/utils/payroll";

const props = withDefaults(
  defineProps<{
    rows: PayrollRow[];
    employees: Employee[];
    loading?: boolean;
    canEdit?: boolean;
    hasPeriods?: boolean;
  }>(),
  { loading: false, canEdit: true, hasPeriods: false },
);

const empMap = computed(() => new Map(props.employees.map((e) => [e.code, e])));
const preview = (row: PayrollRow): PayrollPreview => previewPayroll(row, empMap.value.get(row.employee_code));

const totals = computed(() => {
  let gross = 0;
  let bhEmployer = 0;
  let net = 0;
  for (const r of props.rows) {
    const p = preview(r);
    gross += p.gross;
    bhEmployer += p.bhEmployer;
    net += p.net;
  }
  return { gross, bhEmployer, net };
});
</script>

<template>
  <DataTable
    :value="rows"
    :loading="loading"
    stripedRows
    scrollable
    scrollHeight="460px"
    size="small"
    class="mt-3"
  >
    <Column field="employee_code" header="Mã NV" style="width: 90px" />
    <Column field="employee_name" header="Họ tên" style="min-width: 170px" />
    <Column header="Số công" style="width: 95px">
      <template #body="{ index }">
        <InputNumber
          v-model="rows[index].work_days"
          :min="0"
          :max="26"
          :step="0.5"
          :maxFractionDigits="1"
          :disabled="!canEdit"
          class="w-full"
        />
      </template>
    </Column>
    <Column header="Lương thời gian" style="width: 130px" align="right">
      <template #body="{ data }">{{ fmt(preview(data).timeSalary) }}</template>
    </Column>
    <Column header="Phụ cấp" style="width: 110px" align="right">
      <template #body="{ data }">{{ fmt(preview(data).allowance) }}</template>
    </Column>
    <Column header="Thưởng" style="width: 150px">
      <template #body="{ index }">
        <InputNumber
          v-model="rows[index].bonus"
          :min="0"
          mode="currency"
          currency="VND"
          locale="vi-VN"
          :disabled="!canEdit"
          class="w-full"
        />
      </template>
    </Column>
    <Column header="Lương đóng BH" style="width: 130px" align="right">
      <template #body="{ data }">{{ fmt(preview(data).bhBase) }}</template>
    </Column>
    <Column header="BH SD (21,5%)" style="width: 120px" align="right">
      <template #body="{ data }">{{ fmt(preview(data).bhEmployer) }}</template>
    </Column>
    <Column header="BH NLĐ (10,5%)" style="width: 120px" align="right">
      <template #body="{ data }">{{ fmt(preview(data).bhEmployee) }}</template>
    </Column>
    <Column header="TNCN" style="width: 150px">
      <template #body="{ index }">
        <InputNumber
          v-model="rows[index].pit_amount"
          :min="0"
          mode="currency"
          currency="VND"
          locale="vi-VN"
          :disabled="!canEdit"
          class="w-full"
        />
      </template>
    </Column>
    <Column header="Tạm ứng" style="width: 150px">
      <template #body="{ index }">
        <InputNumber
          v-model="rows[index].advance"
          :min="0"
          mode="currency"
          currency="VND"
          locale="vi-VN"
          :disabled="!canEdit"
          class="w-full"
        />
      </template>
    </Column>
    <Column header="Thực lĩnh" style="width: 140px" align="right">
      <template #body="{ data }">
        <span class="font-bold">{{ fmt(preview(data).net) }}</span>
      </template>
    </Column>
    <Column header="Ghi chú" style="min-width: 180px">
      <template #body="{ index }">
        <InputText v-model="rows[index].note" class="w-full" placeholder="Ghi chú..." :disabled="!canEdit" />
      </template>
    </Column>
    <template #empty>
      <EmptyState
        :text="hasPeriods ? 'Chưa có bảng lương cho kỳ này.' : 'Chưa có bảng lương.'"
        icon="pi pi-money-bill"
      />
    </template>
  </DataTable>

  <div
    v-if="rows.length"
    class="mt-4 flex flex-wrap items-center gap-x-10 gap-y-2 border-t pt-3 text-sm"
  >
    <div class="flex items-center gap-2">
      <span class="text-gray-500">Tổng Gross:</span>
      <span class="font-semibold">{{ fmt(totals.gross) }} đ</span>
    </div>
    <div class="flex items-center gap-2">
      <span class="text-gray-500">Tổng BH SDLĐĐ (21,5%):</span>
      <span class="font-semibold text-amber-600">{{ fmt(totals.bhEmployer) }} đ</span>
    </div>
    <div class="flex items-center gap-2">
      <span class="text-gray-500">Tổng Thực lĩnh:</span>
      <span class="font-bold text-primary-600">{{ fmt(totals.net) }} đ</span>
    </div>
  </div>
</template>
