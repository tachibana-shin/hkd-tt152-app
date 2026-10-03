<script setup lang="ts">
import { api, inTauri } from "@/db";
import { useSettingsStore } from "@/stores/settings";
import { useKeepAliveRefresh } from "@/composables/useKeepAliveRefresh";
import { useAppUpdater } from "@/composables/useAppUpdater";
import { restartAutoTasks } from "@/composables/useAutoTasks";

const settings = useSettingsStore();
const toast = useToast();

const st = reactive({
  product_code_prefix: "SP",
  product_code_start: 1,
  product_code_digits: 4,
  vat_options_text: "1, 3, 5",
  vat_default: 1,
  import_options_text: "0, 5, 8, 10",
  import_default: 0,
  product_unit: "Cái",
  product_min_stock: 0,
  // Ba mốc doanh thu quyết định nhóm hộ (sửa được vì ngưỡng đã nhảy nhiều lần
  // qua các văn bản: 100 triệu → 500 triệu → 01 tỷ).
  tax_exempt: 1_000_000_000,
  tax_group3: 3_000_000_000,
  tax_group4: 50_000_000_000,
  // Việc chạy nền khi mở app (xem composable useAutoTasks).
  auto_check_update: true,
  auto_sync_enabled: false,
  auto_sync_interval_min: 15,
});

const saving = ref(false);
const loading = ref(false);

const updater = useAppUpdater();

/** Kiểm tra cập nhật: báo rõ là đang mới nhất hay có bản mới. */
async function onCheckUpdate() {
  const update = await updater.checkUpdate();
  if (updater.error.value) {
    toast.add({
      severity: "error",
      summary: "Không kiểm tra được cập nhật",
      detail: updater.error.value,
      life: 9000,
    });
    return;
  }
  toast.add({
    severity: update ? "info" : "success",
    summary: update ? `Có bản mới ${update.version}` : "Đang là bản mới nhất",
    detail: update ? "Bấm “Cài bản cập nhật” để tải và khởi động lại app." : "",
    life: 6000,
  });
}

function parseList(text: string): number[] {
  const nums = text
    .split(/[,;\s]+/)
    .map((t) => Number(t.trim()))
    .filter((n) => Number.isFinite(n));
  return [...new Set(nums)].sort((a, b) => a - b);
}

function listToText(options: number[]): string {
  return options.join(", ");
}

async function load() {
  loading.value = true;
  try {
    await settings.load();
    const s = settings.settings;
    if (!s) return;
    st.product_code_prefix = s.product_code_prefix;
    st.product_code_start = s.product_code_start;
    st.product_code_digits = s.product_code_digits;
    st.vat_options_text = listToText(s.product_vat_rate_options);
    st.vat_default = s.product_vat_rate_default;
    st.import_options_text = listToText(s.product_import_tax_options);
    st.import_default = s.product_import_tax_default;
    st.product_unit = s.product_unit;
    st.product_min_stock = s.product_min_stock;
    st.tax_exempt = s.tax_threshold_exempt;
    st.tax_group3 = s.tax_threshold_group3;
    st.tax_group4 = s.tax_threshold_group4;
    st.auto_check_update = s.auto_check_update;
    st.auto_sync_enabled = s.auto_sync_enabled;
    st.auto_sync_interval_min = s.auto_sync_interval_min;
  } finally {
    loading.value = false;
  }
}

async function save() {
  saving.value = true;
  try {
    await settings.save({
      product_code_prefix: st.product_code_prefix.trim() || "SP",
      product_code_start: String(Math.max(1, Math.round(st.product_code_start))),
      product_code_digits: String(Math.min(12, Math.max(1, Math.round(st.product_code_digits)))),
      product_vat_rate_options: JSON.stringify(parseList(st.vat_options_text)),
      product_vat_rate_default: String(Math.max(0, Math.round(st.vat_default))),
      product_import_tax_options: JSON.stringify(parseList(st.import_options_text)),
      product_import_tax_default: String(Math.max(0, Math.round(st.import_default))),
      product_unit: st.product_unit.trim() || "Cái",
      product_min_stock: String(Math.max(0, st.product_min_stock)),
      tax_threshold_exempt: String(taxThresholds.value.exempt),
      tax_threshold_group3: String(taxThresholds.value.group3),
      tax_threshold_group4: String(taxThresholds.value.group4),
      auto_check_update: st.auto_check_update ? "1" : "0",
      auto_sync_enabled: st.auto_sync_enabled ? "1" : "0",
      // Ép về 1–1440 phút: giá trị rác sẽ làm timer không bao giờ reo hoặc reo liên tục.
      auto_sync_interval_min: String(
        Math.min(1440, Math.max(1, Math.round(st.auto_sync_interval_min) || 15)),
      ),
    });
    // Áp dụng lại việc chạy nền ngay (không phải mở lại app mới có hiệu lực).
    await restartAutoTasks();
    toast.add({
      severity: "success",
      summary: "Đã lưu cài đặt",
      detail: "Giá trị mặc định sẽ áp dụng cho sản phẩm mới",
      life: 2500,
    });
  } catch (e) {
    toast.add({ severity: "error", summary: "Lỗi", detail: String(e) });
  } finally {
    saving.value = false;
  }
}

const previewCode = computed(() => {
  const n = String(Math.max(1, st.product_code_start)).padStart(
    Math.min(12, Math.max(1, st.product_code_digits)),
    "0",
  );
  return `${st.product_code_prefix.trim() || "SP"}${n}`;
});

// ─── Ngưỡng doanh thu quyết định nhóm hộ ───
/** Bắt buộc tăng dần: mốc sau không thấp hơn mốc trước, ngưỡng ≤ 0 thì lấy mặc định. */
const taxThresholds = computed(() => {
  const exempt = st.tax_exempt > 0 ? Math.round(st.tax_exempt) : 1_000_000_000;
  const group3 = Math.max(exempt, st.tax_group3 > 0 ? Math.round(st.tax_group3) : 0);
  const group4 = Math.max(group3, st.tax_group4 > 0 ? Math.round(st.tax_group4) : 0);
  return { exempt, group3, group4 };
});
/** Mốc phải tăng dần — báo ngay khi người dùng nhập lộn để không lưu nhầm. */
const taxThresholdWarning = computed(() => {
  if (st.tax_exempt <= 0) return "Ngưỡng phải lớn hơn 0.";
  if (st.tax_group3 < st.tax_exempt)
    return "Mốc Nhóm 3 phải ≥ ngưỡng miễn thuế — sẽ lưu bằng ngưỡng miễn thuế.";
  if (st.tax_group4 < st.tax_group3)
    return "Mốc Nhóm 4 phải ≥ mốc Nhóm 3 — sẽ lưu bằng mốc Nhóm 3.";
  return "";
});
/** Chọn nhanh ngưỡng theo các mốc từng có hiệu lực để dò kịch bản. */
function setTaxExempt(value: number) {
  st.tax_exempt = value;
  if (st.tax_group3 <= value) st.tax_group3 = Math.max(value * 3, 3_000_000_000);
}
const formatVnd = (v: number) =>
  new Intl.NumberFormat("vi-VN", { maximumFractionDigits: 0 }).format(v);

/** Xem thử: mức thuế TNCN theo doanh thu dự kiến cuối năm (tỷ lệ ngành 0,5%). */
const taxPreview = computed(() => {
  const { exempt, group3, group4 } = taxThresholds.value;
  const rates = [
    { label: "500 triệu", revenue: 500_000_000 },
    { label: "1 tỷ", revenue: 1_000_000_000 },
    { label: "1,2 tỷ", revenue: 1_200_000_000 },
    { label: "1,5 tỷ", revenue: 1_500_000_000 },
    { label: "3 tỷ", revenue: 3_000_000_000 },
    { label: "3,5 tỷ", revenue: 3_500_000_000 },
  ];
  return rates.map((r) => {
    const taxable = Math.max(0, r.revenue - exempt);
    return {
      ...r,
      group: r.revenue <= exempt ? 1 : r.revenue <= group3 ? 2 : r.revenue <= group4 ? 3 : 4,
      taxable,
      vat: Math.round(r.revenue * 0.01),
      pit: Math.round(taxable * 0.005),
    };
  });
});

// ─── Web server (dùng app qua trình duyệt) — production mặc định TẮT ───
const webEnabled = ref(false);
const webUrl = ref("");

async function loadWebStatus() {
  try {
    const url = await api.webUrl();
    webEnabled.value = !!url;
    webUrl.value = url || "";
  } catch {
    webEnabled.value = false;
    webUrl.value = "";
  }
}

/** Toggle áp dụng NGAY (không cần bấm "Lưu cài đặt") — lưu cài đặt + bật/tắt server.
 *  Chỉ dùng từ cửa sổ app desktop (IPC); trong trình duyệt toggle bị khóa. */
async function onWebToggle() {
  if (!inTauri) return;
  try {
    const res = JSON.parse(await api.setWebServer(webEnabled.value));
    webEnabled.value = !!res.enabled;
    webUrl.value = res.url ?? "";
    toast.add({
      severity: "success",
      summary: res.enabled ? "Đã bật web server" : "Đã tắt web server",
      detail: res.enabled
        ? `Mở bằng Chrome: ${webUrl.value}`
        : "App chỉ dùng được từ cửa sổ desktop cho đến khi bật lại.",
      life: 3000,
    });
  } catch (e) {
    webEnabled.value = !webEnabled.value; // trả lại trạng thái cũ nếu thất bại
    toast.add({ severity: "error", summary: "Lỗi web server", detail: String(e) });
  }
}

function reload() {
  void load();
  void loadWebStatus();
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
          <i class="pi pi-cog text-xl text-primary-500" />
          <h3 class="font-semibold">Cài đặt mặc định</h3>
          <i
            class="pi pi-info-circle cursor-help text-xs text-gray-400"
            v-tooltip.right="
              'Các thông tư / chính sách thuế thay đổi liên tục — giá trị mặc định được cấu hình tại đây thay vì sửa trong code.'
            "
          />
        </div>
      </template>
      <template #end>
        <Button
          label="Lưu cài đặt"
          icon="pi pi-check"
          :loading="saving"
          :disabled="loading"
          @click="save"
        />
      </template>
    </Toolbar>

    <Card>
      <template #content>
        <div v-if="loading" class="flex justify-center py-10"><ProgressSpinner /></div>
        <div v-else class="space-y-6">
          <!-- Dùng app qua trình duyệt (web server) — production mặc định TẮT -->
          <section>
            <h4 class="font-semibold text-gray-700 mb-3 flex items-center gap-2">
              <i class="pi pi-globe text-primary-500" /> Dùng app qua trình duyệt (Chrome)
            </h4>
            <div
              class="flex items-center justify-between gap-4 rounded-lg border border-gray-200 bg-gray-50 p-4"
            >
              <div class="flex items-center gap-3">
                <ToggleSwitch v-model="webEnabled" :disabled="!inTauri" @change="onWebToggle" />
                <div>
                  <p class="font-medium">Bật web server (http://127.0.0.1)</p>
                  <p class="text-xs text-gray-500 mt-0.5">
                    <template v-if="webEnabled">
                      Đang chạy — mở bằng Chrome: <b>{{ webUrl }}</b> (cùng dữ liệu & phiên đăng
                      nhập, chỉ dùng được trên máy này){{
                        inTauri ? "" : " — bật/tắt từ cửa sổ app desktop."
                      }}
                    </template>
                    <template v-else>
                      Web server đang TẮT — app chỉ dùng được từ cửa sổ desktop. Bật lên để mở app
                      bằng Chrome trên cùng máy.
                    </template>
                  </p>
                </div>
              </div>
              <i
                class="pi pi-info-circle cursor-help text-xs text-gray-400"
                v-tooltip.right="
                  'Bản cài đặt (production) mặc định tắt web server vì lý do bảo mật; bản dev chạy `bun run tauri dev` thì luôn bật. Web server chỉ chạy trên 127.0.0.1 — không lộ ra mạng ngoài. Toggle chỉ dùng được từ cửa sổ app desktop.'
                "
              />
            </div>
          </section>

          <Divider />

          <!-- Mã sản phẩm tự sinh -->
          <section>
            <h4 class="font-semibold text-gray-700 mb-3 flex items-center gap-2">
              <i class="pi pi-tag text-primary-500" /> Mã sản phẩm tự sinh
            </h4>
            <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
              <FormField label="Tiền tố">
                <InputText v-model="st.product_code_prefix" placeholder="SP" class="w-full" />
              </FormField>
              <FormField label="Số bắt đầu">
                <InputNumber v-model="st.product_code_start" :min="1" class="w-full" />
              </FormField>
              <FormField label="Số chữ số">
                <InputNumber v-model="st.product_code_digits" :min="1" :max="12" class="w-full" />
              </FormField>
            </div>
            <p class="text-xs text-gray-500 mt-1">
              Mã tiếp theo sẽ có dạng <b class="text-gray-700">{{ previewCode }}</b> (tự tăng theo
              mã lớn nhất đã có).
            </p>
          </section>

          <Divider />

          <!-- Thuế suất GTGT -->
          <section>
            <h4 class="font-semibold text-gray-700 mb-3 flex items-center gap-2">
              <i class="pi pi-percentage text-primary-500" /> Thuế suất GTGT đầu vào (%) — trên hóa
              đơn mua hàng
            </h4>
            <p class="text-xs text-gray-500 mb-3">
              Ghi nhận thuế suất trên hóa đơn <b>mua vào</b> (nhà cung cấp cộng % này khi xuất hàng
              cho hộ). HKD không xuất VAT khi bán ra — thuế bán ra theo tỷ lệ nhóm ngành trên doanh
              thu.
            </p>
            <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
              <FormField label="Danh sách lựa chọn (cách nhau bởi dấu phẩy)">
                <InputText v-model="st.vat_options_text" class="w-full" />
              </FormField>
              <FormField label="Mặc định khi thêm sản phẩm">
                <InputNumber v-model="st.vat_default" :min="0" suffix="%" class="w-full" />
              </FormField>
            </div>
          </section>

          <Divider />

          <!-- Thuế nhập khẩu -->
          <section>
            <h4 class="font-semibold text-gray-700 mb-3 flex items-center gap-2">
              <i class="pi pi-box text-primary-500" /> Thuế nhập khẩu (%) — hàng nhập về
            </h4>
            <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
              <FormField label="Danh sách lựa chọn (cách nhau bởi dấu phẩy)">
                <InputText v-model="st.import_options_text" class="w-full" />
              </FormField>
              <FormField label="Mặc định khi thêm sản phẩm">
                <InputNumber v-model="st.import_default" :min="0" suffix="%" class="w-full" />
              </FormField>
            </div>
          </section>

          <Divider />

          <!-- Mặc định khác -->
          <section>
            <h4 class="font-semibold text-gray-700 mb-3 flex items-center gap-2">
              <i class="pi pi-sliders-h text-primary-500" /> Mặc định khác
            </h4>
            <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
              <FormField label="Đơn vị tính">
                <InputText v-model="st.product_unit" placeholder="Cái" class="w-full" />
              </FormField>
              <FormField label="Tồn tối thiểu">
                <InputNumber v-model="st.product_min_stock" :min="0" class="w-full" />
              </FormField>
            </div>
          </section>
        </div>
      </template>
    </Card>

    <!-- Ngưỡng doanh thu: quyết định nhóm hộ + khoản miễn thuế, nên cho sửa
         ở đây thay vì gắn cứng trong app (ngưỡng đã nhảy 100tr → 500tr → 1 tỷ). -->
    <Card>
      <template #content>
        <section>
          <h4 class="font-semibold text-gray-700 mb-3 flex items-center gap-2">
            <i class="pi pi-percentage" /> Ngưỡng doanh thu quyết định nhóm hộ
          </h4>
          <p class="text-sm text-gray-600 mb-4">
            Mặc định theo NĐ 141/2026/NĐ-CP (hiệu lực 01/01/2026): doanh thu năm từ
            <b>01 tỷ</b> trở lên phải nộp thuế. Sửa ở đây khi luật đổi mốc, hoặc để dò xem thử kịch
            bản doanh thu cuối năm. Nhóm trong hồ sơ HKD vẫn được tính cho mọi kỳ; vượt mốc giữa năm
            thì năm đó giữ nguyên nhóm, năm sau mới chuyển.
          </p>
          <div class="grid gap-4 md:grid-cols-3">
            <div>
              <label class="block text-sm font-medium text-gray-700 mb-1">
                Ngưỡng không phải nộp thuế (đồng)
              </label>
              <InputNumber
                v-model="st.tax_exempt"
                :min="0"
                :step="50_000_000"
                :use-grouping="true"
                class="w-full"
                data-testid="tax-threshold-exempt"
              />
              <div class="mt-2 flex flex-wrap gap-2">
                <Button label="500 triệu" size="small" text @click="setTaxExempt(500_000_000)" />
                <Button label="1 tỷ" size="small" text @click="setTaxExempt(1_000_000_000)" />
                <Button label="1,5 tỷ" size="small" text @click="setTaxExempt(1_500_000_000)" />
              </div>
            </div>
            <div>
              <label class="block text-sm font-medium text-gray-700 mb-1">
                Mốc Nhóm 3 — thuế suất TNCN 17% (đồng)
              </label>
              <InputNumber
                v-model="st.tax_group3"
                :min="0"
                :step="1_000_000_000"
                :use-grouping="true"
                class="w-full"
                data-testid="tax-threshold-group3"
              />
              <p class="mt-2 text-xs text-gray-500">
                Doanh thu năm vượt ngưỡng và tới mốc này: Nhóm 2 (15%).
              </p>
            </div>
            <div>
              <label class="block text-sm font-medium text-gray-700 mb-1">
                Mốc Nhóm 4 — thuế suất TNCN 20% (đồng)
              </label>
              <InputNumber
                v-model="st.tax_group4"
                :min="0"
                :step="10_000_000_000"
                :use-grouping="true"
                class="w-full"
                data-testid="tax-threshold-group4"
              />
              <p class="mt-2 text-xs text-gray-500">
                Vượt mốc Nhóm 3: Nhóm 3 (17%); vượt mốc này: Nhóm 4 (20%).
              </p>
            </div>
          </div>
          <p v-if="taxThresholdWarning" class="mt-3 text-sm text-amber-600">
            {{ taxThresholdWarning }}
          </p>

          <!-- Xem thử: bảng thuế theo doanh thu dự kiến cuối năm, giả sử tỷ lệ
               ngành 1% GTGT + 0,5% TNCN (đổi ở Cài đặt mặc định sản phẩm). -->
          <div class="mt-5">
            <h5 class="font-medium text-gray-700 mb-2">Xem thử theo doanh thu cuối năm</h5>
            <div class="overflow-x-auto">
              <table class="w-full text-sm" data-testid="tax-threshold-preview">
                <thead>
                  <tr class="border-b border-gray-200 text-left">
                    <th class="py-2 pr-3">Doanh thu năm</th>
                    <th class="py-2 pr-3">Nhóm</th>
                    <th class="py-2 pr-3">DT tính thuế TNCN</th>
                    <th class="py-2 pr-3">Thuế GTGT</th>
                    <th class="py-2">Thuế TNCN</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="r in taxPreview" :key="r.label" class="border-b border-gray-100">
                    <td class="py-1.5 pr-3">{{ r.label }}</td>
                    <td class="py-1.5 pr-3">Nhóm {{ r.group }}</td>
                    <td class="py-1.5 pr-3">{{ formatVnd(r.taxable) }} đ</td>
                    <td class="py-1.5 pr-3">{{ formatVnd(r.vat) }} đ</td>
                    <td class="py-1.5">{{ formatVnd(r.pit) }} đ</td>
                  </tr>
                </tbody>
              </table>
            </div>
            <p class="mt-2 text-xs text-gray-500">
              Bảng chỉ để dò kịch bản; thuế thật lấy từ số liệu từng kỳ ở màn Kế toán → Thuế.
            </p>
          </div>
        </section>
      </template>
    </Card>

    <!-- Cập nhật OTA: chỉ có ở bản cài (Tauri), bản web không tự cập nhật được. -->
    <Card>
      <template #content>
        <section>
          <h4 class="font-semibold text-gray-700 mb-3 flex items-center gap-2">
            <i class="pi pi-cloud-download" /> Cập nhật ứng dụng
          </h4>
          <div
            v-if="!updater.inTauri"
            class="rounded-lg border border-gray-200 bg-gray-50 p-4 text-sm text-gray-600"
          >
            Đang chạy bản web (mở bằng trình duyệt) nên không tự cập nhật được. Cài bản ứng dụng
            (.exe / .dmg / .AppImage) để nhận cập nhật mới.
          </div>
          <div v-else class="space-y-3">
            <div class="flex flex-wrap items-center gap-3 text-sm">
              <span class="text-gray-600">
                Phiên bản đang chạy: <b>{{ updater.current.value || "—" }}</b>
              </span>
              <Button
                label="Kiểm tra cập nhật"
                icon="pi pi-search"
                size="small"
                outlined
                :loading="updater.checking.value"
                :disabled="updater.busy.value"
                @click="onCheckUpdate"
              />
              <Button
                v-if="updater.available.value"
                :label="`Cài bản ${updater.available.value.version}`"
                icon="pi pi-download"
                size="small"
                :loading="updater.installing.value"
                :disabled="updater.busy.value"
                @click="updater.installUpdate()"
              />
            </div>
            <div v-if="updater.installing.value" class="text-sm">
              <ProgressBar
                :value="updater.percent.value"
                :show-value="true"
                class="h-2 w-full max-w-md"
              />
              <div class="mt-1 text-xs text-gray-500">
                Đang tải bản cập nhật… xong sẽ tự khởi động lại app.
              </div>
            </div>
            <p
              v-if="updater.available.value?.notes"
              class="text-sm text-gray-600 whitespace-pre-line"
            >
              {{ updater.available.value.notes }}
            </p>
            <p v-if="updater.error.value" class="text-sm text-red-600">
              {{ updater.error.value }}
            </p>
          </div>
        </section>
      </template>
    </Card>

    <!-- Việc chạy nền khi mở app: kiểm tra cập nhật + quét cổng HĐĐT (useAutoTasks). -->
    <Card>
      <template #content>
        <section>
          <h4 class="font-semibold text-gray-700 mb-1 flex items-center gap-2">
            <i class="pi pi-sync" /> Tự động khi mở ứng dụng
          </h4>
          <p class="text-xs text-gray-500 mb-3">
            Chạy ngay khi mở app rồi lặp lại sau mỗi khoảng thời gian bên dưới. Quét chỉ kéo danh
            sách hóa đơn từ cổng về — việc nhập kho vẫn tự tay.
          </p>
          <div class="space-y-3">
            <div class="flex items-center justify-between gap-4">
              <label for="auto-check-update" class="cursor-pointer">
                <span class="font-medium">Tự kiểm tra cập nhật</span>
                <span class="block text-xs text-gray-500 mt-0.5">
                  Chỉ bản cài (.exe / .dmg / .AppImage); không có bản mới hay lỗi mạng đều im lặng.
                </span>
              </label>
              <ToggleSwitch input-id="auto-check-update" v-model="st.auto_check_update" />
            </div>
            <div class="flex items-center justify-between gap-4">
              <label for="auto-sync-enabled" class="cursor-pointer">
                <span class="font-medium">Tự đồng bộ hóa đơn HĐĐT</span>
                <span class="block text-xs text-gray-500 mt-0.5">
                  Cần đã đăng nhập cổng HĐĐT và khai ngày bắt đầu dùng HĐĐT trong hồ sơ.
                </span>
              </label>
              <ToggleSwitch input-id="auto-sync-enabled" v-model="st.auto_sync_enabled" />
            </div>
            <div class="flex items-center justify-between gap-4">
              <label for="auto-sync-interval" class="cursor-pointer">
                <span class="font-medium">Khoảng thời gian tự gọi lại</span>
                <span class="block text-xs text-gray-500 mt-0.5">
                  Từ 1 tới 1440 phút, áp dụng cho cả hai việc trên.
                </span>
              </label>
              <InputNumber
                v-model="st.auto_sync_interval_min"
                :min="1"
                :max="1440"
                :show-buttons="true"
                :step="5"
                suffix=" phút"
                input-id="auto-sync-interval"
                class="w-44"
              />
            </div>
          </div>
        </section>
      </template>
    </Card>
  </div>
</template>
