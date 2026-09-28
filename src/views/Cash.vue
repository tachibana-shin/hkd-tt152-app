<script setup lang="ts">
import { storeToRefs } from "pinia";
import { useAuthStore } from "@/stores/auth";
import { useCatalogStore } from "@/stores/catalog";
import { api } from "@/db";
import type { JournalEntryRow, Account } from "@/types";
import { fmtInt as fmt, fmtVnd, toIsoDate } from "@/utils/format";
import { useKeepAliveRefresh } from "@/composables/useKeepAliveRefresh";

const auth = useAuthStore();
const catalog = useCatalogStore();
const { customers, suppliers } = storeToRefs(catalog);
const toast = useToast();

// Danh mục tài khoản (DMTK) — nguồn chọn TK Nợ / TK Có khi lập phiếu.
const accounts = ref<Account[]>([]);
const accountOptions = computed(() =>
  accounts.value.map((a) => ({
    code: a.code,
    label: `${a.code} — ${a.name}`,
  })),
);

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
const tagSeverity: Record<string, "success" | "warn"> = {
  PT: "success",
  PC: "warn",
};

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
  note: "",
});

const filteredEntries = computed(() =>
  typeFilter.value ? entries.value.filter((e) => e.entry_type === typeFilter.value) : entries.value,
);

const objectCode = computed({
  get: () => (form.entry_type === "PT" ? form.customer_code : form.supplier_code),
  set: (v: string) => {
    const code = v ?? "";
    if (form.entry_type === "PT") form.customer_code = code;
    else form.supplier_code = code;
  },
});

/** Số phiếu do backend sinh (PT001 / PC001) — danh sách trên UI có thể đang lọc. */
async function suggestVoucherNo(type: "PT" | "PC") {
  form.voucher_no = await api.nextVoucherNo(type);
}

function resetForm(type: "PT" | "PC") {
  Object.assign(form, {
    entry_type: type,
    posting_date: new Date(),
    voucher_no: "",
    description: type === "PT" ? "Thu tiền mặt" : "Chi tiền mặt",
    customer_code: "",
    supplier_code: "",
    amount: 0,
    debit_account: type === "PT" ? "111" : "642",
    credit_account: type === "PT" ? "511" : "111",
    note: "",
  });
}

function openCreate() {
  resetForm("PT");
  dialog.value = true;
  void suggestVoucherNo("PT");
}

function onTypeChange() {
  const t = form.entry_type;
  form.customer_code = "";
  form.supplier_code = "";
  form.debit_account = t === "PT" ? "111" : "642";
  form.credit_account = t === "PT" ? "511" : "111";
  void suggestVoucherNo(t);
}

async function loadEntries() {
  loading.value = true;
  try {
    const [pt, pc] = await Promise.all([api.getJournalEntries("PT"), api.getJournalEntries("PC")]);
    entries.value = [...pt, ...pc].sort((a, b) =>
      a.posting_date === b.posting_date
        ? a.voucher_no.localeCompare(b.voucher_no)
        : b.posting_date.localeCompare(a.posting_date),
    );
  } catch (e) {
    toast.add({
      severity: "error",
      summary: "Không tải được phiếu",
      detail: String(e),
    });
  } finally {
    loading.value = false;
  }
}

async function save() {
  if (!form.posting_date) {
    toast.add({
      severity: "warn",
      summary: "Thiếu ngày ghi sổ",
      detail: "Chọn ngày cho phiếu",
    });
    return;
  }
  if (!form.voucher_no.trim()) {
    toast.add({
      severity: "warn",
      summary: "Thiếu số phiếu",
      detail: "Số phiếu là bắt buộc",
    });
    return;
  }
  if (!form.description.trim()) {
    toast.add({
      severity: "warn",
      summary: "Thiếu diễn giải",
      detail: "Diễn giải là bắt buộc",
    });
    return;
  }
  if (!form.amount || form.amount <= 0) {
    toast.add({
      severity: "warn",
      summary: "Số tiền không hợp lệ",
      detail: "Số tiền phải lớn hơn 0",
    });
    return;
  }
  saving.value = true;
  try {
    await api.saveCashEntry({
      entry_type: form.entry_type,
      posting_date: toIsoDate(form.posting_date),
      voucher_no: form.voucher_no.trim(),
      description: form.description.trim(),
      customer_code: form.entry_type === "PT" ? form.customer_code : "",
      supplier_code: form.entry_type === "PC" ? form.supplier_code : "",
      amount: form.amount,
      debit_account: form.debit_account.trim(),
      credit_account: form.credit_account.trim(),
      // Phiếu thu/chi chỉ theo dõi tiền mặt — không khai nhóm ngành/thuế
      // (tờ khai thuế chỉ tính từ phiếu bán hàng PX).
      industry_code: "",
      vat_rate: 0,
      pit_rate: 0,
      unit_code: "HKD",
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
    toast.add({
      severity: "error",
      summary: "Không lưu được phiếu",
      detail: String(e),
    });
  } finally {
    saving.value = false;
  }
}

async function reload() {
  await Promise.all([
    catalog.loadAll(),
    loadEntries(),
    api.getAccounts().then((a) => (accounts.value = a)),
  ]);
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
        <AppDataTable :value="filteredEntries" :loading="loading" stripedRows paginator :rows="10">
          <Column field="posting_date" header="Ngày" />
          <Column field="voucher_no" header="Số phiếu" />
          <Column field="entry_type" header="Loại">
            <template #body="{ data }">
              <Tag :value="data.entry_type" :severity="tagSeverity[data.entry_type]" />
            </template>
          </Column>
          <Column field="description" header="Diễn giải" />
          <Column header="Khách hàng / NCC">
            <template #body="{ data }">{{
              data.customer_name || data.supplier_name || "—"
            }}</template>
          </Column>
          <Column field="amount" header="Số tiền" align="right">
            <template #body="{ data }">{{ fmtVnd(data.amount) }}</template>
          </Column>
          <Column field="note" header="Ghi chú">
            <template #body="{ data }">{{ data.note || "—" }}</template>
          </Column>
          <template #empty>
            <EmptyState
              :text="entries.length ? 'Không có phiếu khớp bộ lọc.' : 'Chưa có phiếu thu/chi.'"
              icon="pi pi-wallet"
            />
          </template>
        </AppDataTable>
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
      <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4 py-2">
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
            :placeholder="
              form.entry_type === 'PT' ? 'Thu tiền bán hàng...' : 'Chi phí hoạt động...'
            "
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
          <Select
            v-model="form.debit_account"
            :options="accountOptions"
            option-label="label"
            option-value="code"
            filter
            class="w-full"
          />
        </FormField>
        <FormField label="TK Có">
          <Select
            v-model="form.credit_account"
            :options="accountOptions"
            option-label="label"
            option-value="code"
            filter
            class="w-full"
          />
        </FormField>
        <FormField label="Ghi chú" class="col-span-3">
          <InputText v-model="form.note" />
        </FormField>
      </div>
      <div class="mt-3 flex items-center gap-1 text-xs text-gray-400">
        <span>Đơn vị: HKD · Nợ {{ form.debit_account }} / Có {{ form.credit_account }}</span>
        <i
          class="pi pi-info-circle cursor-help text-xs text-gray-400"
          v-tooltip="'Chọn từ danh mục tài khoản (màn Tài khoản); mặc định theo loại phiếu.'"
          aria-hidden="true"
        />
      </div>
    </AppDialog>
  </div>
</template>
