<script setup lang="ts">
import { useAuthStore } from "@/stores/auth";
import { useProfileStore } from "@/stores/profile";
import { useToast } from "primevue/usetoast";

const auth = useAuthStore();
const profile = useProfileStore();
const toast = useToast();

const username = ref("");
const password = ref("");
const submitting = ref(false);

async function doLogin() {
  if (!username.value.trim() || !password.value) {
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
    await auth.login(username.value.trim(), password.value);
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
    class="flex h-screen items-center justify-center bg-gradient-to-br from-gray-900 via-gray-800 to-primary-900"
  >
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
              />
            </IconField>
          </div>
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
        </div>
      </div>
    </div>
  </div>
</template>