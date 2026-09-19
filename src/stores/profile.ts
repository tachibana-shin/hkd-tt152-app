// Hồ sơ HKD (multi-profile): một máy xử lý nhiều hộ kinh doanh,
// mỗi hồ sơ = 1 file DB riêng (`app_data_dir/profiles/<key>/hkd.db`).
import { api } from "@/db";
import type { Profile, ProfilePrefs } from "@/types";

export const useProfileStore = defineStore("profile", () => {
  const profiles = ref<Profile[]>([]);
  const loading = ref(true);
  const switching = ref(false);

  const active = computed<Profile | null>(() => profiles.value.find((p) => p.active) ?? null);
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

  // ─── TÙY CHỌN ĐĂNG NHẬP THEO HỒ SƠ (tên ghi nhớ + tự động đăng nhập) ───
  const prefs = ref<Record<string, ProfilePrefs>>({});

  async function loadPrefs() {
    try {
      prefs.value = await api.getProfilePrefs();
    } catch {
      prefs.value = {};
    }
  }

  function prefsOf(key: string): ProfilePrefs {
    return (
      prefs.value[key] ?? {
        auto_login: false,
        last_username: null,
        username: null,
        password: null,
      }
    );
  }

  async function savePref(key: string, p: ProfilePrefs) {
    prefs.value[key] = p;
    await api.saveProfilePref(key, p);
  }

  /**
   * Bật/tắt tự động đăng nhập cho 1 hồ sơ.
   * - Bật: lưu kèm tài khoản + mật khẩu để khởi động app tự đăng nhập.
   * - Tắt: xóa mật khẩu đã lưu (giữ lại tên đăng nhập cuối).
   * Truyền `username` để cập nhật tên đăng nhập cuối theo lần đăng nhập.
   */
  async function setAutoLogin(key: string, enabled: boolean, username?: string, password?: string) {
    const cur = prefsOf(key);
    await savePref(key, {
      last_username: username || cur.last_username,
      auto_login: enabled,
      username: enabled ? username || cur.username || null : null,
      password: enabled ? password || cur.password || null : null,
    });
  }

  /**
   * Hồ sơ cần tự động đăng nhập khi khởi động (có lưu tài khoản + mật khẩu):
   * ưu tiên hồ sơ đang mở; nếu không, chỉ tự đăng nhập khi chỉ đúng 1 hồ sơ
   * cấu hình auto-login (tránh đoán mò khi có nhiều).
   */
  const autoLoginTarget = computed<{
    key: string;
    username: string;
    password: string;
  } | null>(() => {
    const candidates = profiles.value
      .map((p) => ({ p, pref: prefs.value[p.key] }))
      .filter((x) => x.pref?.auto_login && x.pref.username && x.pref.password);
    if (!candidates.length) return null;
    const t =
      candidates.find((c) => c.p.active) ?? (candidates.length === 1 ? candidates[0] : null);
    return t ? { key: t.p.key, username: t.pref.username!, password: t.pref.password! } : null;
  });

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

  /**
   * Mở hồ sơ từ màn hình chọn hồ sơ lúc KHỞI ĐỘNG (chưa cần đăng nhập):
   * backend đổi hồ sơ đang mở + swap pool; cập nhật active local ngay để
   * màn hình đăng nhập hiển thị đúng hồ sơ (không cần reload cả app).
   */
  async function select(key: string): Promise<{ name: string }> {
    switching.value = true;
    try {
      const res = await api.selectProfile(key);
      profiles.value = profiles.value.map((p) => ({
        ...p,
        active: p.key === key,
      }));
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
    prefs,
    load,
    loadPrefs,
    prefsOf,
    savePref,
    setAutoLogin,
    autoLoginTarget,
    create,
    rename,
    remove,
    switchTo,
    select,
  };
});
