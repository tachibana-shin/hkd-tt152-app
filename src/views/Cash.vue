<script setup lang="ts">
import { storeToRefs } from "pinia";
import { useAuthStore } from "@/stores/auth";
import { useCatalogStore } from "@/stores/catalog";
import { api } from "@/db";
import type { JournalEntryRow, Account } from "@/types";
import { fmtInt as fmt, fmtVnd, toIsoDate } from "@/utils/format";
import { useKeepAliveRefresh } from "@/composables/useKeepAliveRefresh";
import { useLazyPage } from "@/composables/useLazyPage";
import { useInheritedRange } from "@/composables/useInheritedRange";

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
// Khoảng ngày mở từ màn Kế toán HKD (query ?from=&to=) — S3a là sổ chi tiết tiền
// nên phải lọc theo kỳ đang xem, không lấy cả năm.
const { inheritedFrom, inheritedTo, backLabel, goBack } = useInheritedRange();
const fromDate = ref<Date | null>(inheritedFrom);
const toDate = ref<Date | null>(inheritedTo);
const rangeInherited = computed(() => !!inheritedFrom || !!inheritedTo);
/** true = chỉ lọc khi có ngày; để trống thì xem toàn bộ. */
const range = () => ({
  from: toIsoDate(fromDate.value),
  to: toIsoDate(toDate.value),
});

const entryTypeOptions = [
  { label: "Phiếu thu (PT)", value: "PT" },
  { label: "Phiếu chi (PC)", value: "PC" },
];
const filterTypeOptions = [{ label: "Toàn bộ", value: "" }, ...entryTypeOptions];
const tagSeverity: Record<string, "success" | "warn"> = {
  PT: "success",
  PC: "warn",
};

/** Lý do không được trừ theo Điều 6 khoản 2 NĐ 68/2026. */
const nonDeductibleReasons = [
  {
    label: "Lương, thưởng của chủ hộ / thành viên hộ (trừ phần BHXH)",
    value: "Lương chủ hộ/thành viên hộ",
  },
  { label: "Chi phí phục vụ nhu cầu cá nhân, gia đình", value: "Chi cá nhân, gia đình" },
  { label: "Chi không có đủ hóa đơn, chứng từ", value: "Thiếu chứng từ" },
  { label: "Tiền phạt vi phạm hành chính, bồi thường", value: "Phạt, bồi thường" },
  {
    label: "Tài sản phục vụ cá nhân (đất ở, ô tô, tài sản mang tên cá nhân)",
    value: "Tài sản cá nhân",
  },
  { label: "Lý do khác", value: "Khác" },
];

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
  /** Chứng từ thanh toán không dùng tiền mặt — bắt buộc cho chi ≥ 5 triệu (Điều 6 khoản 1). */
  bank_code: "",
  /** Khoản chi không được trừ khi tính thu nhập tính thuế (Điều 6 khoản 2). */
  deductible: true,
  non_deductible_reason: "",
  note: "",
});

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
    bank_code: "",
    deductible: true,
    non_deductible_reason: "",
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

// Phiếu thu/chi tích luỹ theo thời gian và màn không lọc theo ngày nên trước đây
// tải toàn bộ PT + PC về client rồi lọc/cắt trang. Giờ phân trang server-side:
// entryType rỗng = cả hai loại, có giá trị = lọc đúng loại đó.
const total = ref(0);
async function loadEntries(lazy: Record<string, unknown>) {
  loading.value = true;
  try {
    const res = await api.getJournalEntriesPage(lazy, typeFilter.value, range().from, range().to);
    entries.value = res.rows;
    total.value = res.total;
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

const page = useLazyPage(loadEntries);

/** Đổi bộ lọc loại phiếu → lọc ở server nên phải nạp lại trang đầu. */
function applyTypeFilter() {
  void page.setExtra({});
}

/** Đổi khoảng ngày → nạp lại trang đầu. */
function applyRange() {
  void page.setExtra({});
}

/** Bỏ khoảng ngày kế thừa → xem toàn bộ phiếu. */
function clearInheritedRange() {
  fromDate.value = null;
  toDate.value = null;
  applyRange();
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
      bank_code: form.entry_type === "PC" ? form.bank_code.trim() : "",
      deductible: form.entry_type === "PC" ? form.deductible : true,
      non_deductible_reason: form.entry_type === "PC" ? form.non_deductible_reason : "",
    });
    toast.add({
      severity: "success",
      summary: `Đã lưu ${form.voucher_no}`,
      detail: `${form.entry_type === "PT" ? "Phiếu thu" : "Phiếu chi"} • ${fmt(form.amount)} đ`,
    });
    dialog.value = false;
    resetForm(form.entry_type);
    await page.reload();
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
    page.reload(),
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
        <div class="flex items-center gap-2">
          <Button
            :label="`Quay lại ${backLabel || 'màn trước'}`"
            icon="pi pi-arrow-left"
            severity="secondary"
            outlined
            data-testid="back-to-accounting"
            @click="goBack()"
          />
          <Button
            v-if="auth.canAccounting"
            label="Tạo phiếu"
            icon="pi pi-plus"
            @click="openCreate"
          />
        </div>
      </template>
    </Toolbar>

    <Card>
      <template #content>
        <div class="mb-3 flex items-center justify-between gap-3">
          <span class="text-sm text-gray-500">
            Tổng <b>{{ total.toLocaleString("vi-VN") }}</b> phiếu
          </span>
          <div class="flex flex-wrap items-center gap-2">
            <div>
              <DatePicker
                v-model="fromDate"
                dateFormat="dd/mm/yy"
                placeholder="Từ ngày"
                class="w-36"
                data-testid="cash-from"
                @update:model-value="applyRange()"
              />
            </div>
            <div>
              <DatePicker
                v-model="toDate"
                dateFormat="dd/mm/yy"
                placeholder="Đến ngày"
                class="w-36"
                data-testid="cash-to"
                @update:model-value="applyRange()"
              />
            </div>
            <Button
              v-if="rangeInherited"
              label="Xem toàn bộ"
              size="small"
              text
              @click="clearInheritedRange()"
            />
            <i class="pi pi-filter text-gray-400" />
            <Select
              v-model="typeFilter"
              :options="filterTypeOptions"
              optionLabel="label"
              optionValue="value"
              class="w-56"
              @change="applyTypeFilter()"
            />
          </div>
        </div>
        <AppDataTable
          :value="entries"
          :loading="loading"
          :totalRecords="total"
          :first="page.first.value"
          :rows="page.rowsPerPage.value"
          :rows-per-page-options="page.pageSizes"
          data-key="id"
          sort-mode="single"
          removable-sort
          filter-toggle
          :global-filter-fields="['voucher_no', 'description', 'product_code', 'note']"
          stripedRows
          @page="page.onPage"
          @sort="page.onSort"
          @search="page.onSearch"
        >
          <Column field="posting_date" header="Ngày" sortable />
          <Column field="voucher_no" header="Số phiếu" sortable />
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
        <FormField v-if="form.entry_type === 'PC'" label="Chứng từ chuyển khoản" class="col-span-3">
          <InputText
            v-model="form.bank_code"
            placeholder="Số tài khoản / mã giao dịch (bỏ trống nếu trả tiền mặt)"
          />
          <p class="mt-1 text-xs text-amber-700">
            Khoản chi từ 5.000.000 đồng trở lên mà không có chứng từ thanh toán không dùng tiền mặt
            thì <b>không được trừ</b> khi tính thu nhập tính thuế (Điều 6 khoản 1 NĐ 68/2026).
          </p>
        </FormField>
        <FormField
          v-if="form.entry_type === 'PC'"
          label="Không được trừ khi tính thuế"
          class="col-span-3"
        >
          <div class="flex items-start gap-2">
            <Checkbox v-model="form.deductible" binary input-id="cash-deductible" />
            <div>
              <label for="cash-deductible" class="text-sm">
                Khoản chi này <b>được trừ</b> khi tính thu nhập tính thuế
              </label>
              <p class="text-xs text-gray-500">
                Bỏ chọn khi chi lương chủ hộ, chi cá nhân - gia đình, chi không có chứng từ (Điều 6
                khoản 2).
              </p>
            </div>
          </div>
          <Select
            v-if="!form.deductible"
            v-model="form.non_deductible_reason"
            :options="nonDeductibleReasons"
            option-label="label"
            option-value="value"
            placeholder="Chọn lý do"
            class="mt-2 w-full"
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
