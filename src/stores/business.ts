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

  const reportFrom = computed(() => config.value?.report_from || "");
  const reportTo = computed(() => config.value?.report_to || "");

  return { config, loading, load, save, reportFrom, reportTo };
});