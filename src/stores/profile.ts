// Hồ sơ HKD (multi-profile): một máy xử lý nhiều hộ kinh doanh,
// mỗi hồ sơ = 1 file DB riêng (`app_data_dir/profiles/<key>/hkd.db`).
import { api } from "@/db";
import type { Profile } from "@/types";

export const useProfileStore = defineStore("profile", () => {
  const profiles = ref<Profile[]>([]);
  const loading = ref(true);
  const switching = ref(false);

  const active = computed<Profile | null>(
    () => profiles.value.find((p) => p.active) ?? null,
  );
  const activeName = computed(() => active.value?.name ?? "");

  async function load() {
    loading.value = true;
    try {
      profiles.value = await api.getProfiles();
    } catch {
      profiles.value = [];
    } finally {
      loading.value = false;
    }
  }

  async function create(name: string): Promise<Profile> {
    const p = await api.createProfile(name);
    await load();
    return p;
  }

  async function rename(key: string, name: string) {
    await api.renameProfile(key, name);
    const p = profiles.value.find((x) => x.key === key);
    if (p) p.name = name; // cập nhật local ngay (activeName tự cập nhật qua computed)
  }

  async function remove(key: string) {
    await api.deleteProfile(key);
    profiles.value = profiles.value.filter((p) => p.key !== key);
  }

  /**
   * Mở hồ sơ khác: backend swap pool SQLite ngay khi đang chạy + xóa phiên đăng nhập.
   * Frontend reload toàn bộ → app quay về màn hình đăng nhập với hồ sơ mới.
   */
  async function switchTo(key: string): Promise<{ name: string }> {
    switching.value = true;
    try {
      const res = await api.switchProfile(key);
      return { name: res.name };
    } finally {
      switching.value = false;
    }
  }

  return {
    profiles,
    loading,
    switching,
    active,
    activeName,
    load,
    create,
    rename,
    remove,
    switchTo,
  };
});