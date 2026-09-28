<script setup lang="ts">
// Dialog cấu hình hộ kinh doanh.
import { useBusinessStore } from "@/stores/business";
import { useAuthStore } from "@/stores/auth";
import { useSettingsStore } from "@/stores/settings";
import { api } from "@/db";
import { fmtVnd } from "@/utils/format";
import type { BusinessConfig, TaxInfo, TaxResult } from "@/types";

const props = defineProps<{
  visible: boolean;
  config: BusinessConfig | null;
  /** Chế độ bắt buộc (cập nhật thông tin HKD ngay khi đăng nhập): không đóng
   *  được cho tới khi lưu thành công — ẩn nút Hủy, không có nút X, không Esc. */
  required?: boolean;
}>();
const emit = defineEmits<{
  (e: "update:visible", value: boolean): void;
  (e: "saved"): void;
}>();

const business = useBusinessStore();
const auth = useAuthStore();
const settingsStore = useSettingsStore();
const toast = useToast();
const saving = ref(false);

const form = reactive({
  name: "",
  short_name: "",
  tax_code: "",
  address: "",
  ownership: "Tư nhân",
  province: "",
  tax_code_issued_on: "",
  phone: "",
  email: "",
  // Ngày bắt đầu sử dụng HĐĐT — mốc lấp hóa đơn mua vào (bắt buộc nhập).
  hddt_start_date: null as Date | null,
  // Ký hiệu (mẫu số) HĐĐT của hộ — tuỳ chọn, dùng làm mặc định khi liên kết
  // HĐĐT và tự cập nhật theo lần dùng gần nhất.
  hddt_symbol: "",
  // Công tắc khấu trừ GTGT đầu vào — mặc định TẮT (hộ nộp thuế theo doanh thu).
  vat_deduct: false,
  // Nhóm hộ 1–4: mở hộp thoại sẽ tự điền nhóm app xếp theo doanh thu, người
  // dùng xác nhận hoặc sửa tay nếu cơ quan thuế đã xếp khác.
  tax_group: null as number | null,
});

/** yyyy-mm-dd — DatePicker trả Date, DB lưu chuỗi ngày ISO. */
function isoDate(d: Date | null): string {
  if (!d) return "";
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

/** Chuỗi ngày trong DB → Date để nạp vào DatePicker. */
function parseDate(s: string): Date | null {
  if (!s?.trim()) return null;
  const d = new Date(`${s.trim()}T00:00:00`);
  return Number.isNaN(d.getTime()) ? null : d;
}

// Nhóm hộ theo NĐ 68/2026 + NĐ 141/2026 — app tự xếp từ tổng doanh thu cả năm,
// rồi tự điền vào ô chọn bên dưới để người dùng xác nhận/sửa tay.
const groupOptions = [
  { label: "Nhóm 1 — doanh thu ≤ 1 tỷ (miễn thuế GTGT + TNCN)", value: 1 },
  { label: "Nhóm 2 — doanh thu > 1 tỷ đến 3 tỷ", value: 2 },
  { label: "Nhóm 3 — doanh thu > 3 tỷ đến 50 tỷ", value: 3 },
  { label: "Nhóm 4 — doanh thu > 50 tỷ (khai theo tháng)", value: 4 },
];
const autoGroup = ref<number | null>(null);
const groupConfirmed = ref(false);
const yearRevenue = ref(0);
const loadingGroup = ref(false);

async function loadGroup(stored: number | null) {
  loadingGroup.value = true;
  try {
    const info = await api.getTaxGroup(new Date().getFullYear());
    autoGroup.value = info.auto_group;
    groupConfirmed.value = info.confirmed;
    yearRevenue.value = info.revenue_year;
    // Ưu tiên nhóm đã chốt trong hồ sơ; hộ mới/chưa chốt thì lấy nhóm tự xếp.
    form.tax_group = stored ?? info.group;
  } catch {
    form.tax_group = stored;
  } finally {
    loadingGroup.value = false;
  }
}

async function fillVatSetting() {
  await settingsStore.load();
  form.vat_deduct = settingsStore.deductVat;
}

// ─── Tra cứu MST hộ kinh doanh ───
const lookupLoading = ref(false);
const lookupResults = ref<TaxResult[]>([]);
const lookupSelectedUrl = ref("");

watch(
  () => props.visible,
  (open) => {
    const c = props.config;
    if (!open || !c) return;
    Object.assign(form, {
      name: c.name,
      short_name: c.short_name,
      tax_code: c.tax_code,
      address: c.address,
      ownership: c.ownership,
      province: c.province,
      tax_code_issued_on: c.tax_code_issued_on,
      phone: c.phone,
      email: c.email,
      hddt_start_date: parseDate(c.hddt_start_date),
      hddt_symbol: c.hddt_symbol ?? "",
    });
    lookupLoading.value = false;
    lookupResults.value = [];
    lookupSelectedUrl.value = "";
    void fillVatSetting();
    void loadGroup(c.tax_group ?? null);
  },
);

/** Điền thông tin tra cứu được vào form. */
function applyTaxInfo(info: TaxInfo) {
  const t = (v?: string) => (v ?? "").trim();
  const name = t(info.name);
  const addr = t(info.address);
  const phone = t(info.phone);
  const ownership = t(info.company_type);
  const issued = t(info.managed_by) || t(info.issued_on);
  if (name) form.name = name;
  if (addr) form.address = addr;
  if (phone) form.phone = phone;
  if (ownership) form.ownership = ownership;
  if (issued) form.tax_code_issued_on = issued;
}

async function runLookup() {
  const mst = form.tax_code.trim();
  if (!mst) {
    toast.add({
      severity: "warn",
      summary: "Thiếu MST",
      detail: "Nhập mã số thuế trước khi tra cứu",
    });
    return;
  }
  lookupLoading.value = true;
  lookupResults.value = [];
  lookupSelectedUrl.value = "";
  try {
    const outcome = await api.lookupTaxCode(mst);
    if (outcome.kind === "single") {
      applyTaxInfo(outcome.info);
      toast.add({
        severity: "success",
        summary: "Đã tra cứu",
        detail: `Tìm thấy: ${outcome.info.name ?? outcome.info.tax_code ?? mst}`,
      });
    } else if (outcome.kind === "multiple") {
      lookupResults.value = outcome.results;
      toast.add({
        severity: "info",
        summary: "Nhiều kết quả",
        detail: `Có ${outcome.results.length} kết quả — chọn đúng bên dưới`,
      });
    } else {
      toast.add({
        severity: "warn",
        summary: "Không tìm thấy",
        detail: outcome.message,
      });
    }
  } catch (e) {
    toast.add({ severity: "error", summary: "Lỗi tra cứu", detail: String(e) });
  } finally {
    lookupLoading.value = false;
  }
}

async function applyLookupResult(url: string) {
  if (!url) return;
  try {
    const info = await api.lookupTaxDetail(url);
    applyTaxInfo(info);
    lookupSelectedUrl.value = url;
    toast.add({
      severity: "success",
      summary: "Đã điền thông tin",
      detail: info.name ?? "Xong",
    });
  } catch (e) {
    toast.add({ severity: "error", summary: "Lỗi lấy chi tiết", detail: String(e) });
  }
}

async function save() {
  // Tên, MST và ngày bắt đầu HĐĐT là bắt buộc (nhất là chế độ bắt buộc sau đăng nhập).
  if (!form.name.trim() || !form.tax_code.trim()) {
    toast.add({
      severity: "warn",
      summary: "Thiếu thông tin bắt buộc",
      detail: "Tên hộ kinh doanh và mã số thuế không được để trống",
      life: 3000,
    });
    return;
  }
  if (!form.hddt_start_date) {
    toast.add({
      severity: "warn",
      summary: "Thiếu ngày bắt đầu HĐĐT",
      detail:
        "Cần nhập ngày bắt đầu sử dụng hóa đơn điện tử — app dùng mốc này để tìm hóa đơn mua vào từ cổng HĐĐT.",
      life: 4000,
    });
    return;
  }
  saving.value = true;
  try {
    const { vat_deduct, hddt_start_date, ...rest } = form;
    await Promise.all([
      business.save({ ...rest, hddt_start_date: isoDate(hddt_start_date) }),
      api.saveAppSettings({ vat_deduct: vat_deduct ? "1" : "0" }),
    ]);
    toast.add({ severity: "success", summary: "Đã lưu thông tin hộ kinh doanh" });
    emit("update:visible", false);
    emit("saved");
  } catch (e) {
    toast.add({ severity: "error", summary: "Lỗi", detail: String(e) });
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <AppDialog
    :visible="visible"
    header="Cấu hình hộ kinh doanh"
    width="max-w-2xl"
    action-label="Lưu cấu hình"
    :saving="saving"
    :show-action="auth.isAdmin"
    :cancel-label="required ? null : undefined"
    :closable="!required"
    :close-on-escape="!required"
    @update:visible="emit('update:visible', $event)"
    @action="save"
  >
    <div class="grid grid-cols-1 sm:grid-cols-2 gap-4 py-2">
      <FormField label="Tên hộ kinh doanh" required class="col-span-2">
        <InputText v-model="form.name" placeholder="Ví dụ: Hộ kinh doanh Nguyễn Văn A" />
      </FormField>
      <FormField label="Tên viết tắt">
        <InputText v-model="form.short_name" />
      </FormField>
      <FormField label="Mã số thuế" required>
        <div class="flex gap-2">
          <InputText v-model="form.tax_code" placeholder="VD: 0123456789" class="min-w-0 flex-1" />
          <Button
            label="Tra cứu"
            icon="pi pi-search"
            severity="secondary"
            class="shrink-0 whitespace-nowrap"
            :loading="lookupLoading"
            :disabled="!form.tax_code.trim()"
            @click="runLookup"
          />
        </div>
      </FormField>
      <FormField v-if="lookupResults.length" label="Kết quả tra cứu" class="col-span-2">
        <Select
          v-model="lookupSelectedUrl"
          :options="lookupResults"
          option-label="name"
          option-value="url"
          placeholder="Có nhiều kết quả — chọn đúng hộ kinh doanh…"
          class="w-full"
          @change="applyLookupResult($event.value)"
        />
      </FormField>
      <FormField label="Địa chỉ" class="col-span-2">
        <InputText v-model="form.address" />
      </FormField>
      <FormField label="Loại hình">
        <InputText v-model="form.ownership" />
      </FormField>
      <div class="col-span-2">
        <FormField label="Nhóm hộ kinh doanh" class="w-full">
          <div data-testid="hkd-tax-group" class="w-full">
            <Select
              v-model="form.tax_group"
              :options="groupOptions"
              optionLabel="label"
              optionValue="value"
              class="w-full"
              aria-label="Nhóm hộ kinh doanh"
              :disabled="loadingGroup"
            />
          </div>
          <p class="mt-1 text-xs text-gray-500">
            <template v-if="loadingGroup"> Đang xếp nhóm theo doanh thu… </template>
            <template v-else>
              Doanh thu cả năm <b>{{ fmtVnd(yearRevenue) }}</b> → app xếp <b>Nhóm {{ autoGroup }}</b
              >.
              <template v-if="form.tax_group !== autoGroup">
                Đang dùng <b>Nhóm {{ form.tax_group }}</b> theo lựa chọn của bạn.
              </template>
              <template v-else-if="!groupConfirmed">
                Chưa chốt — lưu cấu hình để ghi nhóm này vào hồ sơ hộ.
              </template>
              <template v-else> Đã chốt trong hồ sơ hộ.</template>
              Sửa tay ở đây khi cơ quan thuế đã xếp khác (NĐ 68/2026 + NĐ 141/2026).
            </template>
          </p>
        </FormField>
      </div>
      <FormField label="Tỉnh/TP">
        <InputText v-model="form.province" />
      </FormField>
      <FormField label="Nơi cấp MST">
        <InputText v-model="form.tax_code_issued_on" />
      </FormField>
      <FormField label="Ngày bắt đầu dùng HĐĐT" required input-id="hddt-start-date">
        <template #default="{ inputId }">
          <DatePicker
            v-model="form.hddt_start_date"
            :input-id="inputId"
            dateFormat="dd/mm/yy"
            :maxDate="new Date()"
            showIcon
            showClear
            class="w-full"
            placeholder="Ngày bắt đầu phát hành hóa đơn điện tử"
          />
          <p class="mt-1 text-xs text-gray-500">
            App dùng ngày này làm mốc bắt đầu khi lấp hóa đơn mua vào từ cổng HĐĐT.
          </p>
        </template>
      </FormField>
      <FormField label="Ký hiệu HĐĐT của hộ">
        <InputText v-model="form.hddt_symbol" placeholder="VD: 1C26TT152" class="w-full" />
        <p class="mt-1 text-xs text-gray-500">
          Mỗi hộ có một ký hiệu riêng. Hộp thoại "Liên kết HĐĐT" lấy giá trị này làm mặc định và tự
          cập nhật theo lần dùng gần nhất.
        </p>
      </FormField>
      <FormField label="Số điện thoại">
        <InputText v-model="form.phone" />
      </FormField>
      <FormField label="Email" class="col-span-2">
        <InputText v-model="form.email" />
      </FormField>
    </div>

    <div class="rounded-lg border border-gray-200 bg-surface-50 p-3 space-y-3">
      <div class="flex items-center justify-between gap-4">
        <div>
          <p class="text-sm font-semibold text-gray-800">Khấu trừ thuế GTGT đầu vào</p>
          <p class="text-xs text-gray-500">
            Mặc định <b>TẮT</b> — hộ nộp thuế theo doanh thu không được khấu trừ, giá nhập kho đã
            gồm thuế. Bật khi hộ nộp thuế theo lợi nhuận (xác định được chi phí): nhập kho tách thuế
            vào TK 133, giá nhập = giá chưa thuế.
          </p>
        </div>
        <ToggleSwitch v-model="form.vat_deduct" class="shrink-0" />
      </div>
    </div>
  </AppDialog>
</template>
