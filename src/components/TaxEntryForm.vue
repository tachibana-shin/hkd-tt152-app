<script setup lang="ts">
import { api } from "@/db";
import { useBusinessStore } from "@/stores/business";
import { describeRange } from "@/utils/period";
import {
  buildTaxEntryRows,
  exportTaxEntry,
  taxEntryFileName,
  taxEntryGroupLabel,
  unmappedRevenue,
  type TaxEntryPayload,
  type TaxEntryRow,
  type TaxEntryUnmapped,
} from "@/utils/taxEntry";

/**
 * **Tờ khai 01/CNKD (hộ kinh doanh)** — màn "Chọn địa điểm kinh doanh cần kê khai doanh thu"
 * của cổng thuế, dựng lại trong tab Kế toán HKD.
 *
 * Khác mục "Tờ khai thuế" phía trên (báo cáo số thuế phải nộp): mục này là
 * **bản kê khai** — 6 dòng nhóm ngành nghề cố định, 2 cột doanh thu, tự lấp từ
 * sổ nhưng sửa tay được, rồi in / xuất Excel đúng mẫu.
 *
 * Bản in nằm trong `<Teleport to="body">` để trở thành con trực tiếp của `<body>`:
 * chỉ khi bấm "In" mới thêm class `print-tax-entry` cho `<body>`, CSS toàn cục
 * (style.css) ẩn mọi phần còn lại → in ra đúng tờ khai, không in cả giao diện.
 */
const props = defineProps<{
  /** Khoảng ngày kê khai (ISO) — lấy cùng nguồn với mục "Tờ khai thuế". */
  periodFrom: string;
  periodTo: string;
  unitCode: string;
}>();

const business = useBusinessStore();
const toast = useToast();

const rows = ref<TaxEntryRow[]>([]);
const unmapped = ref<TaxEntryUnmapped[]>([]);
const loading = ref(false);

/**
 * Mã địa điểm kinh doanh (trên cổng là số, ví dụ `00999`) — lưu riêng trong
 * `app_setting` vì không thuộc `AppSettings`.
 */
const locationCode = ref("00999");

/**
 * Công tắc cạnh mã địa điểm trên cổng (cùng cái đang bật trong ảnh mẫu) —
 * tắt là **địa điểm này không tham gia kê khai** kỳ này.
 */
const enabled = ref(true);

/** Số doanh thu **do app tự lấp** — đối chiếu với số trên bảng để biết dòng nào đã sửa tay. */
const base = ref<TaxEntryRow[]>([]);
/** Bản ghi tay theo kỳ: `{ "<từ>|<đến>|<kỳ>: { "<thứ tự>": {vat, pit} } }`. */
const overrides = ref<Record<string, Record<string, { vat: number; pit: number }>>>({});

const SETTING_KEY = "tax_entry";
/** Chặn watcher ghi đè khi `reload()` vừa gán `rows` (đó là số tự lấp, không phải bản sửa). */
let hydrating = false;
let saveTimer: number | undefined;

const periodLabel = computed(() =>
  describeRange(new Date(props.periodFrom), new Date(props.periodTo)),
);
const taxpayer = computed(() => business.config?.name ?? "");
const exportPayload = (): TaxEntryPayload => ({
  periodLabel: periodLabel.value,
  locationCode: locationCode.value,
  taxpayer: taxpayer.value,
  enabled: enabled.value,
  rows: rows.value.map((r) => ({ ...r })),
  unmapped: unmapped.value,
  from: props.periodFrom,
  to: props.periodTo,
  fileName: taxEntryFileName(props.periodFrom, props.periodTo),
});

defineExpose({ exportPayload, reload });

/** Kỳ hiện tại — bản ghi tay của kỳ khác không áp vào kỳ này. */
const periodKey = () => `${props.periodFrom}|${props.periodTo}|${props.unitCode}`;

const num = (v: number | null | undefined) => (typeof v === "number" && Number.isFinite(v) ? v : 0);

/** Đặt số tự láp lên bảng theo bản ghi tay (nếu có) — số đã sửa không bị mất khi tải lại. */
function applyOverrides() {
  const saved = overrides.value[periodKey()] ?? {};
  rows.value = base.value.map((r) => {
    const o = saved[String(r.order)];
    return o ? { ...r, revenueVat: num(o.vat), revenuePit: num(o.pit) } : { ...r };
  });
}

/** Tính bản ghi tay: chỉ giữ các dòng **khác** số tự lấp — dòng chưa đụng tới
 *  vẫn tự theo sổ khi doanh thu thay đổi. */
function persistOverrides() {
  if (hydrating) return;
  const edited: Record<string, { vat: number; pit: number }> = {};
  for (const r of rows.value) {
    const b = base.value.find((x) => x.order === r.order);
    if (!b) continue;
    const vat = num(r.revenueVat);
    const pit = num(r.revenuePit);
    if (vat !== b.revenueVat || pit !== b.revenuePit) edited[String(r.order)] = { vat, pit };
  }
  const key = periodKey();
  // Không có gì đổi so với bản ghi sẵn → đừng ghi lại: vừa tránh một lượt lưu
  // vô ích, vừa khiến watcher của `reload()` không thể "số tự lấp thành bản sửa".
  if (canon(edited) === canon(overrides.value[key] ?? {})) return;
  const all = { ...overrides.value };
  if (Object.keys(edited).length) all[key] = edited;
  else delete all[key];
  overrides.value = all;
  scheduleSave();
}

/** So 2 bản ghi tay bất kể thứ tự khoá (JSON.stringify giữ thứ tự chèn). */
function canon(o: Record<string, { vat: number; pit: number }>): string {
  return JSON.stringify(
    Object.keys(o)
      .sort()
      .map((k) => [k, o[k].vat, o[k].pit]),
  );
}

// Sửa ô bất kỳ (InputNumber đổi model) → ghi lại bản ghi tay.
watch(rows, persistOverrides, { deep: true });

function scheduleSave() {
  window.clearTimeout(saveTimer);
  saveTimer = window.setTimeout(() => void saveSettings(), 600);
}

async function saveSettings() {
  try {
    await api.saveAppSettings({
      [SETTING_KEY]: JSON.stringify({
        location: locationCode.value,
        enabled: enabled.value,
        overrides: overrides.value,
      }),
    });
  } catch (e) {
    toast.add({
      severity: "error",
      summary: "Không lưu được bản ghi tay tờ khai",
      detail: String(e),
    });
  }
}

async function loadSettings() {
  try {
    const raw = await api.getRawAppSettings();
    const parsed: unknown = JSON.parse(raw[SETTING_KEY] ?? "{}");
    const obj = (parsed && typeof parsed === "object" ? parsed : {}) as {
      location?: unknown;
      enabled?: unknown;
      overrides?: unknown;
    };
    if (typeof obj.location === "string" && obj.location.trim())
      locationCode.value = obj.location.trim();
    // Chưa từng ghi công tắc (lần đầu mở màn) → giữ mặc định BẬT như trên cổng.
    if (typeof obj.enabled === "boolean") enabled.value = obj.enabled;
    overrides.value =
      obj.overrides && typeof obj.overrides === "object"
        ? (obj.overrides as Record<string, Record<string, { vat: number; pit: number }>>)
        : {};
  } catch {
    // Cấu hình hỏng/đang trống → giữ mặc định, vẫn cho kê khai bình thường.
    overrides.value = {};
  }
}

/** Nạp doanh thu từ sổ cho kỳ đang chọn rồi dựng 6 dòng mẫu. */
async function reload() {
  loading.value = true;
  try {
    const revenues = await api.getRevenueDeclaration(
      props.periodFrom,
      props.periodTo,
      props.unitCode,
    );
    base.value = buildTaxEntryRows(revenues);
    unmapped.value = unmappedRevenue(revenues);
    hydrating = true;
    applyOverrides();
    await nextTick();
  } catch (e) {
    toast.add({ severity: "error", summary: "Lỗi tải doanh thu kê khai", detail: String(e) });
  } finally {
    hydrating = false;
    loading.value = false;
  }
}

/** Bỏ bản ghi tay của kỳ này → về đúng số trên sổ. */
async function resetToBooks() {
  const all = { ...overrides.value };
  delete all[periodKey()];
  overrides.value = all;
  applyOverrides();
  await saveSettings();
  toast.add({ severity: "info", summary: "Đã lấy lại số doanh thu từ sổ", life: 2500 });
}

async function exportExcel() {
  try {
    await exportTaxEntry(exportPayload(), props.periodFrom, props.periodTo);
  } catch (e) {
    toast.add({ severity: "error", summary: "Lỗi xuất tờ khai Excel", detail: String(e) });
  }
}

/** In tờ khai — chỉ phần `.tax-entry-print` được phép in ra (xem style.css). */
function printEntry() {
  const cls = "print-tax-entry";
  const cleanup = () => document.body.classList.remove(cls);
  document.body.classList.add(cls);
  window.addEventListener("afterprint", cleanup, { once: true });
  window.print();
  // Không phải trình duyệt nào cũng bắn `afterprint` đúng lúc → gỡ lớp dự phòng.
  window.setTimeout(cleanup, 2000);
}

// Kỳ đổi (dùng chung khoảng ngày với mục "Tờ khai thuế") → nạp lại số.
watch(
  () => [props.periodFrom, props.periodTo, props.unitCode],
  () => void reload(),
);
watch(locationCode, scheduleSave);
watch(enabled, scheduleSave);

void (async () => {
  await business.load();
  await loadSettings();
  await reload();
})();
</script>

<template>
  <SectionCard>
    <template #icon><i-mdi-clipboard-text-outline class="text-fuchsia-500" /></template>
    <template #title>
      <span>Tờ khai 01/CNKD (hộ kinh doanh)</span>
      <span class="text-xs font-normal text-gray-400"
        >Hộ kinh doanh, cá nhân kinh doanh — kê khai doanh thu theo từng địa điểm</span
      >
    </template>
    <template #actions>
      <Button
        label="Xuất Excel"
        icon="pi pi-file-excel"
        size="small"
        outlined
        @click="exportExcel"
      />
      <Button label="In tờ khai" icon="pi pi-print" size="small" outlined @click="printEntry" />
      <Button
        label="Lấy lại số từ sổ"
        icon="pi pi-refresh"
        size="small"
        text
        :loading="loading"
        @click="resetToBooks"
      />
    </template>

    <div
      class="mb-3 flex flex-wrap items-end gap-3 text-xs text-gray-500"
      data-testid="tax-entry-summary"
    >
      <span>
        Kỳ kê khai: <b class="text-primary-600">{{ periodLabel }}</b>
      </span>
      <span v-if="enabled">
        Tổng doanh thu GTGT:
        <b>{{ rows.reduce((s, r) => s + num(r.revenueVat), 0).toLocaleString("vi-VN") }}</b> · TNCN:
        <b>{{ rows.reduce((s, r) => s + num(r.revenuePit), 0).toLocaleString("vi-VN") }}</b>
      </span>
      <div class="ml-auto flex items-center gap-2">
        <label for="tax-entry-location" class="mb-0">Mã địa điểm kinh doanh</label>
        <InputText
          id="tax-entry-location"
          v-model="locationCode"
          class="w-28"
          placeholder="00999"
          data-testid="tax-entry-location"
        />
      </div>
    </div>

    <p class="mb-1 text-sm font-semibold">
      I. Hoạt động sản xuất, kinh doanh hàng hóa, cung cấp dịch vụ có địa điểm kinh doanh cố định
    </p>

    <!-- Công tắc đúng vị trí trên ảnh mẫu: bên trái dòng mã địa điểm kinh doanh. -->
    <div class="mb-2 flex items-center gap-3">
      <ToggleSwitch
        v-model="enabled"
        input-id="tax-entry-enabled"
        data-testid="tax-entry-enabled"
        aria-label="Kê khai địa điểm kinh doanh này"
      />
      <p class="mb-0 text-sm">
        <b>1. {{ locationCode || "—" }}</b
        ><span v-if="taxpayer"> - {{ taxpayer }}</span>
      </p>
    </div>

    <AppDataTable
      v-if="enabled"
      :value="rows"
      :loading="loading"
      stripedRows
      :filter-toggle="false"
      data-testid="tax-entry-table"
    >
      <Column header="Nhóm ngành nghề">
        <template #body="{ data }">
          <span class="whitespace-pre-line">{{ taxEntryGroupLabel(data as TaxEntryRow) }}</span>
        </template>
      </Column>
      <Column header="Doanh thu thuế giá trị gia tăng" align="right">
        <template #body="{ data }">
          <InputNumber
            v-model="data.revenueVat"
            mode="decimal"
            locale="vi-VN"
            :min="0"
            :max-fraction-digits="0"
            class="w-full"
            input-class="text-right"
            :aria-label="`Doanh thu thuế giá trị gia tăng dòng ${data.order}`"
          />
        </template>
      </Column>
      <Column header="Doanh thu thuế thu nhập cá nhân" align="right">
        <template #body="{ data }">
          <InputNumber
            v-model="data.revenuePit"
            mode="decimal"
            locale="vi-VN"
            :min="0"
            :max-fraction-digits="0"
            class="w-full"
            input-class="text-right"
            :aria-label="`Doanh thu thuế thu nhập cá nhân dòng ${data.order}`"
          />
        </template>
      </Column>
    </AppDataTable>

    <div
      v-else
      class="rounded-md border border-dashed border-gray-300 bg-gray-50 px-4 py-6 text-center text-sm text-gray-500"
      data-testid="tax-entry-disabled"
    >
      <i class="pi pi-toggle-off mr-1" />
      Công tắc tắt — địa điểm <b>{{ locationCode || "—" }}</b> không tham gia kê khai doanh thu kỳ
      này. Bật công tắc để hiện bảng kê khai.
    </div>

    <p v-if="unmapped.length" class="mt-2 text-xs text-orange-600">
      <i class="pi pi-exclamation-triangle mr-1" />
      Nhóm ngành của hồ sơ không có trong mẫu đã gộp vào "Hoạt động kinh doanh khác":
      {{ unmapped.map((u) => `${u.name} (${u.code})`).join(", ") }}
    </p>
    <p class="mt-3 text-xs text-gray-500">
      Số doanh thu tự lấp từ sổ theo kỳ đang chọn; sửa ô nào thì tờ khai in / xuất Excel theo số đã
      sửa. Nhóm ngành của hồ sơ (VD "Sản xuất, chế biến, chế tạo", "Vận tải, kho bãi") được gộp vào
      6 dòng mẫu theo tỷ lệ tương ứng — <b>Lấy lại số từ sổ</b> sẽ bỏ các bản sửa tay của kỳ này.
    </p>
  </SectionCard>

  <!-- Bản in: ẩn trên màn hình, chỉ hiện khi body nhận class `print-tax-entry`. -->
  <Teleport to="body">
    <div class="tax-entry-print">
      <h1>Chọn địa điểm kinh doanh cần kê khai doanh thu</h1>
      <p class="tax-entry-print-form">
        Mẫu số 01/CNKD — Tờ khai thuế đối với hộ kinh doanh, cá nhân kinh doanh
      </p>
      <p>Kỳ kê khai: {{ periodLabel }}</p>
      <p v-if="taxpayer">Người nộp thuế: {{ taxpayer }}</p>
      <p class="tax-entry-print-sec">
        I. Hoạt động sản xuất, kinh doanh hàng hóa, cung cấp dịch vụ có địa điểm kinh doanh cố định
      </p>
      <p class="tax-entry-print-loc">
        1. {{ locationCode || "—" }}<span v-if="taxpayer"> - {{ taxpayer }}</span>
        <span v-if="!enabled"> — KHÔNG KÊ KHAI ĐỊA ĐIỂM NÀY</span>
      </p>
      <p v-if="!enabled" class="tax-entry-print-note">
        Công tắc tắt: địa điểm {{ locationCode || "—" }} không tham gia kê khai doanh thu kỳ này.
      </p>
      <table v-else>
        <thead>
          <tr>
            <th class="tax-entry-print-name">Nhóm ngành nghề</th>
            <th>Doanh thu thuế giá trị gia tăng</th>
            <th>Doanh thu thuế thu nhập cá nhân</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="r in rows" :key="r.order">
            <td class="tax-entry-print-name">
              <span class="whitespace-pre-line">{{ taxEntryGroupLabel(r) }}</span>
            </td>
            <td class="tax-entry-print-num">{{ num(r.revenueVat).toLocaleString("vi-VN") }}</td>
            <td class="tax-entry-print-num">{{ num(r.revenuePit).toLocaleString("vi-VN") }}</td>
          </tr>
        </tbody>
      </table>
      <p v-if="unmapped.length" class="tax-entry-print-note">
        Nhóm ngành của hồ sơ không có trong mẫu đã gộp vào "Hoạt động kinh doanh khác":
        {{ unmapped.map((u) => `${u.name} (${u.code})`).join(", ") }}
      </p>
      <p class="tax-entry-print-sign">……, ngày …… tháng …… năm …………</p>
      <p class="tax-entry-print-sign"><b>Người nộp thuế ký tên</b></p>
    </div>
  </Teleport>
</template>

<style scoped>
/* Bản in — khổ A4, đen trên trắng, không phụ thuộc giao diện đang bật sáng/tối. */
.tax-entry-print {
  display: none;
  font-family: "Times New Roman", serif;
  font-size: 13pt;
  color: #111;
  background: #fff;
  padding: 15mm;
}

.tax-entry-print h1 {
  margin: 0 0 6px;
  font-size: 14pt;
  text-align: center;
  text-transform: uppercase;
}

.tax-entry-print p {
  margin: 5px 0;
}

/* Dòng nêu mã mẫu ngay dưới tiêu đề — người cầm bản in biết đây là mẫu nào. */
.tax-entry-print-form {
  margin: 0 0 10px;
  font-size: 11pt;
  text-align: center;
}

.tax-entry-print-sec {
  font-weight: bold;
}

.tax-entry-print-loc {
  font-weight: bold;
  margin-bottom: 10px;
}

.tax-entry-print table {
  width: 100%;
  border-collapse: collapse;
}

.tax-entry-print th,
.tax-entry-print td {
  border: 1px solid #111;
  padding: 6px 8px;
  vertical-align: middle;
}

.tax-entry-print th {
  text-align: center;
}

.tax-entry-print-name {
  width: 55%;
  text-align: left;
}

.tax-entry-print-num {
  text-align: right;
}

.tax-entry-print-note {
  font-size: 10pt;
  font-style: italic;
}

.tax-entry-print-sign {
  text-align: right;
  margin-top: 18px;
}
</style>
