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

  /**
   * Thông tin hộ kinh doanh đã đủ chưa.
   * Bắt buộc: tên + mã số thuế + ngày bắt đầu dùng HĐĐT.
   * Ngày HĐĐT nằm trong điều kiện này để hồ sơ đã tồn tại (đã có tên + MST từ
   * trước) vẫn bị popup lúc mở app yêu cầu khai báo — mốc lấp hóa đơn mua vào.
   */
  const complete = computed(() => {
    const c = config.value;
    return !!c && !!c.name?.trim() && !!c.tax_code?.trim() && !!c.hddt_start_date?.trim();
  });

  return { config, loading, load, save, complete };
});
