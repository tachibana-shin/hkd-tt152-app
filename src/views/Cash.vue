<script setup lang="ts">
import { storeToRefs } from "pinia";
import { useAuthStore } from "@/stores/auth";
import { useCatalogStore } from "@/stores/catalog";
import { api } from "@/db";
import type { JournalEntryRow } from "@/types";
import { fmtInt as fmt } from "@/utils/format";

const auth = useAuthStore();
const catalog = useCatalogStore();
const { customers, suppliers, industryGroups } = storeToRefs(catalog);
const toast = useToast();

const dialog = ref(false);
const saving = ref(false);
const loading = ref(false);
const entries = ref<JournalEntryRow[]>([]);
const typeFilter = ref("");

const entryTypeOptions = [
  { label: "Phiếu thu (PT)", value: "PT" },
  { label: "Phiếu chi (PC)", value: "PC" },
];
const filterTypeOptions = [{ label: "Toàn bộ", value: "" }, ...entryTypeOptions];
const tagSeverity: Record<string, "success" | "warn"> = { PT: "success", PC: "warn" };

const form = reactive({
  entry_type: "PT" as "PT" | "PC",
  posting_date: new Date(),
  voucher_no: "PT001",
  description: "",
  customer_code: "",
  supplier_code: "",
  amount: 0,
  debit_account: "111",
  credit_account: "511",
  industry_code: "",
  vat_rate: 0,
  pit_rate: 0,
  note: "",
});

const filteredEntries = computed(() =>
  typeFilter.value
    ? entries.value.filter((e) => e.entry_type === typeFilter.value)
    : entries.value,
);

const objectCode = computed({
  get: () => (form.entry_type === "PT" ? form.customer_code : form.supplier_code),
  set: (v: string) => {
    const code = v ?? "";
    if (form.entry_type === "PT") form.customer_code = code;
    else form.supplier_code = code;
  },
});

const iso = (d: Date | null) => (d ? d.toISOString().slice(0, 10) : "");
function suggestVoucherNo(type: string) {
  const nums = entries.value
    .filter((e) => e.entry_type === type)
    .map((e) => parseInt(e.voucher_no.replace(/\D/g, ""), 10))
    .filter((n) => !isNaN(n));
  const next = nums.length ? Math.max(...nums) + 1 : 1;
  return `${type}${String(next).padStart(3, "0")}`;
}

function resetForm(type: "PT" | "PC") {
  Object.assign(form, {
    entry_type: type,
    posting_date: new Date(),
    voucher_no: suggestVoucherNo(type),
    description: type === "PT" ? "Thu tiền mặt" : "Chi tiền mặt",
    customer_code: "",
    supplier_code: "",
    amount: 0,
    debit_account: type === "PT" ? "111" : "642",
    credit_account: type === "PT" ? "511" : "111",
    industry_code: "",
    vat_rate: 0,
    pit_rate: 0,
    note: "",
  });
}

function openCreate() {
  resetForm("PT");
  dialog.value = true;
}

function onTypeChange() {
  const t = form.entry_type;
  form.customer_code = "";
  form.supplier_code = "";
  form.debit_account = t === "PT" ? "111" : "642";
  form.credit_account = t === "PT" ? "511" : "111";
  form.voucher_no = suggestVoucherNo(t);
}

function onIndustryPick() {
  const g = industryGroups.value.find((x) => x.code === form.industry_code);
  form.vat_rate = g ? Math.round(g.vat_rate * 100) : 0;
  form.pit_rate = g ? Math.round(g.pit_rate * 100) : 0;
}

async function loadEntries() {
  loading.value = true;
  try {
    const [pt, pc] = await Promise.all([
      api.getJournalEntries("PT"),
      api.getJournalEntries("PC"),
    ]);
    entries.value = [...pt, ...pc].sort(
      (a, b) =>
        a.posting_date === b.posting_date
          ? a.voucher_no.localeCompare(b.voucher_no)
          : b.posting_date.localeCompare(a.posting_date),
    );
  } catch (e) {
    toast.add({ severity: "error", summary: "Không tải được phiếu", detail: String(e) });
  } finally {
    loading.value = false;
  }
}

async function save() {
  if (!form.posting_date) {
    toast.add({ severity: "warn", summary: "Thiếu ngày ghi sổ", detail: "Chọn ngày cho phiếu" });
    return;
  }
  if (!form.voucher_no.trim()) {
    toast.add({ severity: "warn", summary: "Thiếu số phiếu", detail: "Số phiếu là bắt buộc" });
    return;
  }
  if (!form.description.trim()) {
    toast.add({ severity: "warn", summary: "Thiếu diễn giải", detail: "Diễn giải là bắt buộc" });
    return;
  }
  if (!form.amount || form.amount <= 0) {
    toast.add({ severity: "warn", summary: "Số tiền không hợp lệ", detail: "Số tiền phải lớn hơn 0" });
    return;
  }
  saving.value = true;
  try {
    await api.saveCashEntry({
      entry_type: form.entry_type,
      posting_date: iso(form.posting_date),
      voucher_no: form.voucher_no.trim(),
      description: form.description.trim(),
      customer_code: form.entry_type === "PT" ? form.customer_code : "",
      supplier_code: form.entry_type === "PC" ? form.supplier_code : "",
      amount: form.amount,
      debit_account: form.debit_account.trim(),
      credit_account: form.credit_account.trim(),
      industry_code: form.industry_code,
      vat_rate: form.vat_rate / 100,
      pit_rate: form.pit_rate / 100,
      unit_code: "HaNoi-01",
      note: form.note.trim(),
    });
    toast.add({
      severity: "success",
      summary: `Đã lưu ${form.voucher_no}`,
      detail: `${form.entry_type === "PT" ? "Phiếu thu" : "Phiếu chi"} • ${fmt(form.amount)} đ`,
    });
    dialog.value = false;
    resetForm(form.entry_type);
    await loadEntries();
  } catch (e) {
    toast.add({ severity: "error", summary: "Không lưu được phiếu", detail: String(e) });
  } finally {
    saving.value = false;
  }
}

onMounted(async () => {
  await Promise.all([catalog.loadAll(), loadEntries()]);
});
</script>

<template>
  <div class="space-y-4">
    <Toolbar>
      <template #start>
        <div class="flex items-center gap-2">
          <i-mdi-cash-multiple class="text-xl text-green-600" />
          <h3 class="font-semibold">Phiếu thu / chi</h3>
        </div>
      </template>
      <template #end>
        <Button v-if="auth.canAccounting" label="Tạo phiếu" icon="pi pi-plus" @click="openCreate" />
      </template>
    </Toolbar>

    <Card>
      <template #content>
        <div class="mb-3 flex items-center justify-between gap-3">
          <span class="text-sm text-gray-500">Tổng số phiếu: {{ entries.length }}</span>
          <div class="flex items-center gap-2">
            <i class="pi pi-filter text-gray-400" />
            <Select
              v-model="typeFilter"
              :options="filterTypeOptions"
              optionLabel="label"
              optionValue="value"
              class="w-56"
            />
          </div>
        </div>
        <DataTable :value="filteredEntries" :loading="loading" stripedRows paginator :rows="10">
          <Column field="posting_date" header="Ngày" style="width: 110px" />
          <Column field="voucher_no" header="Số phiếu" style="width: 110px" />
          <Column field="entry_type" header="Loại" style="width: 90px">
            <template #body="{ data }">
              <Tag :value="data.entry_type" :severity="tagSeverity[data.entry_type]" />
            </template>
          </Column>
          <Column field="description" header="Diễn giải" />
          <Column header="Khách hàng / NCC" style="width: 190px">
            <template #body="{ data }">{{ data.customer_name || data.supplier_name || "—" }}</template>
          </Column>
          <Column field="amount" header="Số tiền" style="width: 150px" align="right">
            <template #body="{ data }">{{ fmt(data.amount) }}</template>
          </Column>
          <Column field="industry_code" header="Nhóm ngành" style="width: 120px">
            <template #body="{ data }">
              <Tag v-if="data.industry_code" :value="data.industry_code" severity="info" />
              <span v-else>—</span>
            </template>
          </Column>
          <Column field="note" header="Ghi chú" style="width: 160px">
            <template #body="{ data }">{{ data.note || "—" }}</template>
          </Column>
          <template #empty>
            <EmptyState
              :text="entries.length ? 'Không có phiếu khớp bộ lọc.' : 'Chưa có phiếu thu/chi.'"
              icon="pi pi-wallet"
            />
          </template>
        </DataTable>
      </template>
    </Card>

    <AppDialog
      v-model:visible="dialog"
      header="Tạo phiếu thu / chi"
      width="max-w-3xl"
      action-label="Lưu phiếu"
      :saving="saving"
      :show-action="auth.canAccounting"
      @action="save"
    >
      <div class="grid grid-cols-3 gap-4 py-2">
        <FormField label="Loại phiếu" required>
          <Select
            v-model="form.entry_type"
            :options="entryTypeOptions"
            optionLabel="label"
            optionValue="value"
            class="w-full"
            @change="onTypeChange"
          />
        </FormField>
        <FormField label="Ngày ghi sổ" required>
          <DatePicker v-model="form.posting_date" dateFormat="dd/mm/yy" class="w-full" />
        </FormField>
        <FormField label="Số phiếu" required>
          <InputText v-model="form.voucher_no" />
        </FormField>
        <FormField label="Diễn giải" required class="col-span-2">
          <InputText
            v-model="form.description"
            :placeholder="form.entry_type === 'PT' ? 'Thu tiền bán hàng...' : 'Chi phí hoạt động...'"
          />
        </FormField>
        <FormField :label="form.entry_type === 'PT' ? 'Khách hàng' : 'Nhà cung cấp'">
          <Select
            v-model="objectCode"
            :options="form.entry_type === 'PT' ? customers : suppliers"
            optionLabel="name"
            optionValue="code"
            :editable="true"
            filter
            showClear
            class="w-full"
            placeholder="Chọn hoặc nhập (không bắt buộc)"
          />
        </FormField>
        <FormField label="Số tiền" required>
          <InputNumber
            v-model="form.amount"
            :min="0"
            mode="currency"
            currency="VND"
            locale="vi-VN"
            class="w-full"
          />
        </FormField>
        <FormField label="TK Nợ">
          <InputText v-model="form.debit_account" class="w-28" />
        </FormField>
        <FormField label="TK Có">
          <InputText v-model="form.credit_account" class="w-28" />
        </FormField>
        <FormField label="Nhóm ngành">
          <Select
            v-model="form.industry_code"
            :options="industryGroups"
            optionLabel="name"
            optionValue="code"
            filter
            showClear
            class="w-full"
            @change="onIndustryPick"
          />
        </FormField>
        <FormField label="Thuế GTGT (%)">
          <InputNumber v-model="form.vat_rate" :min="0" :max="100" suffix="%" class="w-full" />
        </FormField>
        <FormField label="Thuế TNCN (%)">
          <InputNumber v-model="form.pit_rate" :min="0" :max="100" suffix="%" class="w-full" />
        </FormField>
        <FormField label="Ghi chú" class="col-span-3">
          <InputText v-model="form.note" />
        </FormField>
      </div>
      <div class="mt-3 flex items-center gap-1 text-xs text-gray-400">
        <span>Đơn vị: HaNoi-01 · Nợ {{ form.debit_account }} / Có {{ form.credit_account }}</span>
        <i class="pi pi-info-circle cursor-help text-xs text-gray-400" v-tooltip="'Tài khoản mặc định theo loại phiếu; sửa được.'" aria-hidden="true" />
      </div>
    </AppDialog>
  </div>
</template>