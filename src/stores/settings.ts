import { api } from "@/db";
import type { AppSettings } from "@/types";

// Cài đặt mặc định toàn cục (bảng app_setting — thông tư thay đổi liên tục).
export const useSettingsStore = defineStore("settings", () => {
  const settings = ref<AppSettings | null>(null);

  async function load(force = false) {
    if (settings.value && !force) return;
    settings.value = await api.getAppSettings();
  }

  const vatRateOptions = computed(() => settings.value?.product_vat_rate_options ?? [5, 8, 10]);
  const vatRateDefault = computed(() => settings.value?.product_vat_rate_default ?? 10);
  const importTaxOptions = computed(
    () => settings.value?.product_import_tax_options ?? [0, 5, 8, 10],
  );
  const importTaxDefault = computed(() => settings.value?.product_import_tax_default ?? 0);
  const defaultUnit = computed(() => settings.value?.product_unit ?? "Cái");
  const defaultMinStock = computed(() => settings.value?.product_min_stock ?? 0);

  async function nextProductCode() {
    return api.nextProductCode();
  }

  async function save(values: Record<string, string>) {
    await api.saveAppSettings(values);
    await load(true);
  }

  return {
    settings,
    load,
    vatRateOptions,
    vatRateDefault,
    importTaxOptions,
    importTaxDefault,
    defaultUnit,
    defaultMinStock,
    nextProductCode,
    save,
  };
});