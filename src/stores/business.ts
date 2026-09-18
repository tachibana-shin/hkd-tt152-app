import { api } from "@/db";
import type { BusinessConfig } from "@/types";

export const useBusinessStore = defineStore("business", () => {
  const config = ref<BusinessConfig | null>(null);
  const loading = ref(false);

  async function load() {
    loading.value = true;
    try {
      config.value = await api.getBusinessConfig();
    } finally {
      loading.value = false;
    }
  }

  async function save(patch: Partial<BusinessConfig>) {
    await api.saveBusinessConfig({ ...config.value, ...patch });
    await load();
  }

  /** Thông tin hộ kinh doanh đã đủ chưa (tên + mã số thuế — 2 trường bắt buộc). */
  const complete = computed(() => {
    const c = config.value;
    return !!c && !!c.name?.trim() && !!c.tax_code?.trim();
  });

  return { config, loading, load, save, complete };
});