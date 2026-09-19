<script setup lang="ts">
// ProductDialog — dialog chuẩn thêm / sửa sản phẩm (hàng hóa, dịch vụ).
// Dùng chung cho màn Danh mục sản phẩm, và mọi nơi cần thêm nhanh hàng hóa
// (phiếu nhập kho…) — tận dụng: tự sinh mã SP, mặc định đơn vị/giá/thuế từ
// cấu hình, nhóm ngành, thuế nhập khẩu.
import { useCatalogStore } from "@/stores/catalog";
import { useSettingsStore } from "@/stores/settings";
import type { Product } from "@/types";

const props = withDefaults(
  defineProps<{
    visible: boolean;
    /** Đang sửa sản phẩm (null = thêm mới — tự sinh mã). */
    product?: Product | null;
    showAction?: boolean;
  }>(),
  { product: null, showAction: true },
);

const emit = defineEmits<{
  (e: "update:visible", value: boolean): void;
  (e: "saved", code: string): void;
}>();

const catalog = useCatalogStore();
const settings = useSettingsStore();
const toast = useToast();

const saving = ref(false);
const dialogTitle = computed(() => (props.product ? "Sửa sản phẩm" : "Thêm sản phẩm"));
const form = reactive({
  id: null as number | null,
  code: "",
  name: "",
  unit: "Cái",
  sale_price: 0,
  cost_price: 0,
  min_stock: 0,
  vat_rate: 10,
  import_tax_rate: 0,
  is_service: false,
  industry_code: "PPHH",
});

// Thuế nhập khẩu dùng AutoComplete: chọn nhanh từ danh sách cấu hình hoặc gõ tùy ý.
const importTaxText = ref("0");
const importTaxSuggestions = ref<string[]>([]);

function onImportTaxComplete() {
  importTaxSuggestions.value = settings.importTaxOptions
    .map((n) => String(n))
    .filter((o) => o.includes(importTaxText.value.trim()));
}

function syncImportTaxText() {
  importTaxText.value = String(form.import_tax_rate);
}

function readImportTax() {
  const n = parseFloat(importTaxText.value.replace(",", "."));
  form.import_tax_rate = Number.isFinite(n) ? Math.max(0, Math.round(n)) : 0;
}

watch(
  () => props.visible,
  async (open) => {
    if (!open) return;
    saving.value = false;
    const p = props.product;
    if (p) {
      Object.assign(form, {
        id: p.id,
        code: p.code,
        name: p.name,
        unit: p.unit,
        sale_price: p.sale_price,
        cost_price: p.cost_price,
        min_stock: p.min_stock,
        vat_rate: p.vat_rate * 100,
        import_tax_rate: p.import_tax_rate * 100,
        is_service: p.is_service ?? false,
        industry_code: p.industry_code ?? "PPHH",
      });
    } else {
      // Thêm mới: tự sinh mã SP tiếp theo + mặc định từ cấu hình hộ kinh doanh.
      try {
        const [code] = await Promise.all([
          settings.nextProductCode(),
          settings.load(),
        ]);
        Object.assign(form, {
          id: null,
          code,
          name: "",
          unit: settings.defaultUnit,
          sale_price: 0,
          cost_price: 0,
          min_stock: settings.defaultMinStock,
          vat_rate: settings.vatRateDefault,
          import_tax_rate: settings.importTaxDefault,
          is_service: false,
          industry_code: "PPHH",
        });
      } catch {
        Object.assign(form, {
          id: null,
          code: "",
          name: "",
          unit: "Cái",
          sale_price: 0,
          cost_price: 0,
          min_stock: 0,
          vat_rate: 10,
          import_tax_rate: 0,
          is_service: false,
          industry_code: "PPHH",
        });
      }
    }
    syncImportTaxText();
    // Select Nhóm ngành cần dữ liệu — khi gọi từ phiếu nhập (chưa load) thì nạp thêm.
    catalog.loadIndustryGroups();
  },
);

async function save() {
  if (!form.code || !form.name) {
    toast.add({
      severity: "warn",
      summary: "Thiếu thông tin",
      detail: "Mã và tên sản phẩm là bắt buộc",
    });
    return;
  }
  saving.value = true;
  try {
    readImportTax();
    await catalog.saveProduct({
      id: form.id ?? undefined,
      code: form.code,
      name: form.name,
      unit: form.unit,
      sale_price: form.sale_price,
      cost_price: form.cost_price,
      min_stock: form.min_stock,
      vat_rate: form.vat_rate / 100,
      import_tax_rate: form.import_tax_rate / 100,
      is_service: form.is_service,
      industry_code: form.industry_code,
    });
    toast.add({
      severity: "success",
      summary: "Đã lưu",
      detail: `Sản phẩm ${form.code}`,
    });
    emit("update:visible", false);
    emit("saved", form.code);
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
    :header="dialogTitle"
    width="max-w-xl"
    action-label="Lưu"
    :saving="saving"
    :show-action="showAction"
    @update:visible="emit('update:visible', $event)"
    @action="save"
  >
    <div class="grid grid-cols-2 gap-4 py-2">
      <FormField label="Mã sản phẩm" required>
        <InputText
          size="small"
          v-model="form.code"
          placeholder="SP001"
          :disabled="!!product"
        />
      </FormField>
      <FormField label="Đơn vị tính">
        <InputText size="small" v-model="form.unit" placeholder="Cái" />
      </FormField>
      <FormField label="Tên sản phẩm" required class="col-span-2">
        <InputText
          size="small"
          v-model="form.name"
          placeholder="Tên hàng hóa / dịch vụ"
          :disabled="!!product"
        />
        <p v-if="product" class="text-xs text-gray-400 mt-1">
          Tên sản phẩm không đổi được sau khi tạo — tránh lệch với phiếu nhập /
          xuất và hóa đơn đã lập.
        </p>
      </FormField>
      <FormField label="Loại">
        <Select
          v-model="form.is_service"
          :options="[
            { label: 'Hàng hóa (theo dõi tồn kho)', value: false },
            {
              label: 'Dịch vụ (không nhập/xuất kho — vd nhân công)',
              value: true,
            },
          ]"
          option-label="label"
          option-value="value"
          size="small"
          class="w-full"
        />
        <p class="text-xs text-gray-400 mt-1">
          Thuế suất GTGT là thuế trên hóa đơn <b>mua vào</b> (đầu vào). HKD
          không xuất VAT khi bán ra — thuế bán ra tính theo nhóm ngành khi ghi
          phiếu xuất / hóa đơn.
        </p>
      </FormField>
      <FormField label="Nhóm ngành (cơ sở tính thuế bán ra)">
        <Select
          v-model="form.industry_code"
          :options="catalog.industryGroups"
          option-label="name"
          option-value="code"
          show-clear
          size="small"
          class="w-full"
        />
        <p class="text-xs text-gray-400 mt-1">
          Tự điền vào dòng phiếu xuất / hóa đơn.
        </p>
      </FormField>
      <FormField label="Giá bán">
        <InputNumber
          v-model="form.sale_price"
          :min="0"
          mode="currency"
          currency="VND"
          locale="vi-VN"
          size="small"
          class="w-full"
        />
      </FormField>
      <FormField label="Giá vốn">
        <InputNumber
          v-model="form.cost_price"
          :min="0"
          mode="currency"
          currency="VND"
          locale="vi-VN"
          size="small"
          class="w-full"
        />
      </FormField>
      <FormField label="Tồn tối thiểu">
        <InputNumber v-model="form.min_stock" :min="0" size="small" class="w-full" />
      </FormField>
      <FormField label="Thuế suất GTGT đầu vào (%)">
        <Select
          v-model="form.vat_rate"
          :options="settings.vatRateOptions"
          size="small"
          class="w-full"
        />
      </FormField>
      <FormField label="Thuế nhập khẩu (%)">
        <AutoComplete
          v-model="importTaxText"
          :suggestions="importTaxSuggestions"
          @complete="onImportTaxComplete"
          placeholder="Gõ hoặc chọn tỷ lệ"
          size="small"
          class="w-full"
        />
      </FormField>
    </div>
  </AppDialog>
</template>