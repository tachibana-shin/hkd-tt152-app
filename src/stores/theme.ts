import { defineStore } from "pinia";

/** Lựa chọn giao diện theo máy (dùng chung mọi hồ sơ). */
const STORAGE_KEY = "hkd.theme";

function readSaved(): "dark" | "light" | null {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    return v === "dark" || v === "light" ? v : null;
  } catch {
    return null;
  }
}

function systemPrefersDark(): boolean {
  try {
    return window.matchMedia?.("(prefers-color-scheme: dark)")?.matches ?? false;
  } catch {
    return false;
  }
}

/** Áp theme lên <html> ngay (gọi trước khi mount để khỏi chớp sáng/tối khi khởi động). */
export function applyInitialTheme(): void {
  const saved = readSaved();
  const dark = saved ? saved === "dark" : systemPrefersDark();
  document.documentElement.classList.toggle("dark", dark);
  document.documentElement.style.colorScheme = dark ? "dark" : "light";
}

export const useThemeStore = defineStore("theme", () => {
  const saved = readSaved();
  const dark = ref(saved ? saved === "dark" : systemPrefersDark());

  function apply() {
    document.documentElement.classList.toggle("dark", dark.value);
    document.documentElement.style.colorScheme = dark.value ? "dark" : "light";
  }

  /** Bật/tắt giao diện tối; ghi nhớ lựa chọn theo máy. */
  function toggle() {
    dark.value = !dark.value;
    try {
      localStorage.setItem(STORAGE_KEY, dark.value ? "dark" : "light");
    } catch {
      /* bỏ qua */
    }
    apply();
  }

  /** Theo dõi đổi giao diện hệ thống (chỉ áp dụng khi người dùng chưa tự chọn). */
  function init() {
    apply();
    try {
      const mq = window.matchMedia?.("(prefers-color-scheme: dark)");
      mq?.addEventListener?.("change", (e) => {
        if (!readSaved()) {
          dark.value = e.matches;
          apply();
        }
      });
    } catch {
      /* trình duyệt cũ → bỏ qua */
    }
  }

  return { dark, apply, toggle, init };
});
