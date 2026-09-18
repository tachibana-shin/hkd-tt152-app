<script setup lang="ts">
import { useSettingsStore } from "@/stores/settings";

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
});

const saving = ref(false);
const loading = ref(false);

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
    });
    toast.add({ severity: "success", summary: "Đã lưu cài đặt", detail: "Giá trị mặc định sẽ áp dụng cho sản phẩm mới", life: 2500 });
  } catch (e) {
    toast.add({ severity: "error", summary: "Lỗi", detail: String(e) });
  } finally {
    saving.value = false;
  }
}

const previewCode = computed(() => {
  const n = String(Math.max(1, st.product_code_start)).padStart(Math.min(12, Math.max(1, st.product_code_digits)), "0");
  return `${st.product_code_prefix.trim() || "SP"}${n}`;
});

onMounted(load);
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
            v-tooltip.right="'Các thông tư / chính sách thuế thay đổi liên tục — giá trị mặc định được cấu hình tại đây thay vì sửa trong code.'"
          />
        </div>
      </template>
      <template #end>
        <Button label="Lưu cài đặt" icon="pi pi-check" :loading="saving" :disabled="loading" @click="save" />
      </template>
    </Toolbar>

    <Card>
      <template #content>
        <div v-if="loading" class="flex justify-center py-10"><ProgressSpinner /></div>
        <div v-else class="space-y-6">
          <!-- Mã sản phẩm tự sinh -->
          <section>
            <h4 class="font-semibold text-gray-700 mb-3 flex items-center gap-2">
              <i class="pi pi-tag text-primary-500" /> Mã sản phẩm tự sinh
            </h4>
            <div class="grid grid-cols-3 gap-4">
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
              Mã tiếp theo sẽ có dạng <b class="text-gray-700">{{ previewCode }}</b> (tự tăng theo mã lớn nhất đã có).
            </p>
          </section>

          <Divider />

          <!-- Thuế suất GTGT -->
          <section>
            <h4 class="font-semibold text-gray-700 mb-3 flex items-center gap-2">
              <i class="pi pi-percentage text-primary-500" /> Thuế suất GTGT đầu vào (%) — trên hóa đơn mua hàng
            </h4>
            <p class="text-xs text-gray-500 mb-3">
              Ghi nhận thuế suất trên hóa đơn <b>mua vào</b> (nhà cung cấp cộng %
              này khi xuất hàng cho hộ). HKD không xuất VAT khi bán ra — thuế
              bán ra theo tỷ lệ nhóm ngành trên doanh thu.
            </p>
            <div class="grid grid-cols-2 gap-4">
              <FormField label="Danh sách lựa chọn (cách nhau bởi dấu phẩy)">
                <InputText v-model="st.vat_options_text" class="w-full" />
              </FormField>
              <FormField label="Mặc định khi thêm sản phẩm">
                <InputNumber v-model="st.vat_default" :min="0" class="w-full" />
              </FormField>
            </div>
          </section>

          <Divider />

          <!-- Thuế nhập khẩu -->
          <section>
            <h4 class="font-semibold text-gray-700 mb-3 flex items-center gap-2">
              <i class="pi pi-box text-primary-500" /> Thuế nhập khẩu (%) — hàng nhập về
            </h4>
            <div class="grid grid-cols-2 gap-4">
              <FormField label="Danh sách lựa chọn (cách nhau bởi dấu phẩy)">
                <InputText v-model="st.import_options_text" class="w-full" />
              </FormField>
              <FormField label="Mặc định khi thêm sản phẩm">
                <InputNumber v-model="st.import_default" :min="0" class="w-full" />
              </FormField>
            </div>
          </section>

          <Divider />

          <!-- Mặc định khác -->
          <section>
            <h4 class="font-semibold text-gray-700 mb-3 flex items-center gap-2">
              <i class="pi pi-sliders-h text-primary-500" /> Mặc định khác
            </h4>
            <div class="grid grid-cols-2 gap-4">
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
  </div>
</template>