import { api } from "@/db";
import type { CurrentUser, Role } from "@/types";

export const useAuthStore = defineStore("auth", () => {
  const currentUser = ref<CurrentUser | null>(null);
  const loading = ref(true);

  const role = computed<Role>(() => currentUser.value?.role ?? "xem");
  const isLoggedIn = computed(() => currentUser.value !== null);
  const isAdmin = computed(() => role.value === "admin");
  /** admin + ketoan: mọi thao tác kế toán/thuế/lương */
  const canAccounting = computed(() => ["admin", "ketoan"].includes(role.value));
  /** admin + ketoan + kho: nhập/xuất/tồn kho & danh mục */
  const canStock = computed(() => ["admin", "ketoan", "kho"].includes(role.value));
  /** bất kỳ tài khoản nào khác "xem" đều được ghi dữ liệu tương ứng menu được phép */
  const canWrite = computed(() => role.value !== "xem");
  const isXem = computed(() => role.value === "xem");

  async function load() {
    loading.value = true;
    try {
      currentUser.value = await api.getCurrentUser();
    } catch {
      currentUser.value = null;
    } finally {
      loading.value = false;
    }
  }

  async function login(username: string, password: string) {
    currentUser.value = await api.login(username, password);
  }

  async function logout() {
    try {
      await api.logout();
    } finally {
      currentUser.value = null;
    }
  }

  return {
    currentUser,
    loading,
    role,
    isLoggedIn,
    isAdmin,
    canAccounting,
    canStock,
    canWrite,
    isXem,
    load,
    login,
    logout,
  };
});
