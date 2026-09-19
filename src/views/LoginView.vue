<script setup lang="ts">
import { useAuthStore } from "@/stores/auth";
import { useProfileStore } from "@/stores/profile";
import { useThemeStore } from "@/stores/theme";
import { useToast } from "primevue/usetoast";
import { api, inTauri } from "@/db";

const auth = useAuthStore();
const profile = useProfileStore();
const theme = useThemeStore();
const toast = useToast();

const username = ref("");
const password = ref("");
const autoLogin = ref(false);
const submitting = ref(false);

// URL web server local — cho biết mở Chrome vào đâu để dùng app như web (chỉ trong app desktop).
const browserUrl = ref("");

// Tên đăng nhập nhập lần trước của hồ sơ này được ghi nhớ (lưu ở profile_prefs.json);
// nếu đã bật tự động đăng nhập thì tích sẵn ô "Tự động đăng nhập".
onMounted(async () => {
  await profile.loadPrefs(); // đảm bảo đã nạp (App cũng nạp khi khởi động)
  const key = profile.active?.key;
  if (!key) return;
  const pref = profile.prefsOf(key);
  username.value = pref.last_username ?? "";
  autoLogin.value = !!pref.auto_login && !!pref.username && !!pref.password;
  if (inTauri) {
    try {
      browserUrl.value = await api.webUrl();
    } catch {
      /* web server không khả dụng — bỏ qua */
    }
  }
});

async function doLogin() {
  const user = username.value.trim();
  if (!user || !password.value) {
    toast.add({
      severity: "warn",
      summary: "Thiếu thông tin",
      detail: "Nhập tên đăng nhập và mật khẩu",
      life: 2500,
    });
    return;
  }
  submitting.value = true;
  try {
    await auth.login(user, password.value);
    // Đăng nhập thành công → ghi nhớ tên đăng nhập cho hồ sơ này; lưu thêm
    // mật khẩu nếu người dùng bật tự động đăng nhập (tắt thì xóa mật khẩu cũ).
    const key = profile.active?.key;
    if (key) {
      await profile.setAutoLogin(
        key,
        autoLogin.value,
        user,
        autoLogin.value ? password.value : undefined,
      );
    }
  } catch (e) {
    toast.add({
      severity: "error",
      summary: "Đăng nhập thất bại",
      detail: String(e),
      life: 3500,
    });
  } finally {
    submitting.value = false;
  }
}

const roleHint = computed(() => {
  switch (auth.role) {
    case "admin":
      return "Quản trị — toàn quyền";
    case "ketoan":
      return "Kế toán — sổ sách, thuế, lương, thu/chi";
    case "kho":
      return "Kho — nhập/xuất/tồn & danh mục";
    default:
      return "Xem — chỉ đọc";
  }
});
</script>

<template>
  <div
    class="relative flex h-screen items-center justify-center bg-gradient-to-br from-gray-900 via-gray-800 to-primary-900"
  >
    <!-- Bật/tắt giao diện sáng-tối -->
    <button
      @click="theme.toggle"
      class="absolute top-4 right-4 w-10 h-10 flex items-center justify-center rounded-full text-white/70 hover:text-white hover:bg-white/10 transition-colors"
      :aria-label="theme.dark ? 'Chuyển sang giao diện sáng' : 'Chuyển sang giao diện tối'"
    >
      <i :class="theme.dark ? 'pi pi-sun' : 'pi pi-moon'" class="text-lg"></i>
    </button>
    <div class="w-full max-w-md px-6">
      <div class="bg-white rounded-2xl shadow-2xl overflow-hidden">
        <div class="bg-gray-900 px-8 py-6 text-white">
          <h1 class="text-2xl font-bold tracking-wide">HKD Kế Toán</h1>
          <p class="text-sm text-gray-400 mt-1">TT 152/2025 • Hộ kinh doanh</p>
          <p v-if="profile.activeName" class="text-xs text-gray-300 mt-2 flex items-center gap-1">
            <i class="pi pi-database"></i> Hồ sơ: <b>{{ profile.activeName }}</b>
          </p>
        </div>
        <form class="px-8 py-6 space-y-4" @submit.prevent="doLogin">
          <div>
            <label class="block text-sm font-medium text-gray-700 mb-1">Tên đăng nhập</label>
            <IconField>
              <InputIcon class="pi pi-user" />
              <InputText
                v-model="username"
                class="w-full"
                placeholder="admin"
                autocomplete="username"
                :disabled="submitting"
                data-testid="login-username"
              />
            </IconField>
          </div>
          <div>
            <label class="block text-sm font-medium text-gray-700 mb-1">Mật khẩu</label>
            <IconField>
              <InputIcon class="pi pi-lock" />
              <InputText
                v-model="password"
                type="password"
                class="w-full"
                placeholder="••••••••"
                autocomplete="current-password"
                :disabled="submitting"
                data-testid="login-password"
              />
            </IconField>
          </div>
          <label
            class="flex items-center gap-3 text-sm text-gray-600 cursor-pointer select-none"
            v-tooltip.top="
              'Khởi động app lần sau sẽ tự đăng nhập hồ sơ này bằng tài khoản vừa nhập'
            "
          >
            <ToggleSwitch v-model="autoLogin" :disabled="submitting" inputId="autoLogin" />
            <span>Tự động đăng nhập khi khởi động app</span>
          </label>
          <Button
            type="submit"
            label="Đăng nhập"
            icon="pi pi-sign-in"
            class="w-full"
            :loading="submitting"
          />
        </form>
        <div class="px-8 py-4 bg-gray-50 border-t border-gray-100 text-xs text-gray-500 space-y-1">
          <p>Tài khoản mặc định: <b>admin</b> / <b>admin123</b></p>
          <p v-if="auth.currentUser" class="text-primary-600 font-medium">
            Đã đăng nhập: {{ auth.currentUser.display_name }} ({{ roleHint }})
          </p>
          <p v-if="browserUrl" class="flex items-center gap-1">
            <i class="pi pi-globe"></i> Mở bằng Chrome: <b>{{ browserUrl }}</b>
          </p>
        </div>
      </div>
    </div>
  </div>
</template>
