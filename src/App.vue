<script setup lang="ts">
import { useAuthStore } from "@/stores/auth";
import { useProfileStore } from "@/stores/profile";
import { useBusinessStore } from "@/stores/business";
import LoginView from "@/views/LoginView.vue";

const route = useRoute();
const router = useRouter();
const auth = useAuthStore();
const profile = useProfileStore();
const business = useBusinessStore();
const toast = useToast();
const pageTitle = computed(() => route.meta?.title || "HKD Kế Toán");

// Luồng khởi động: nạp hồ sơ + prefs → thử auto-login → chọn hồ sơ (nhiều hồ sơ)
// → màn hình đăng nhập. `bootstrapped` giữ màn chờ cho tới khi hoàn tất.
const bootstrapped = ref(false);
const showPicker = ref(false);

async function bootstrap() {
  // Đổi hồ sơ từ màn Hồ sơ HKD reload cả app → vào thẳng màn hình đăng nhập
  // (bỏ qua picker và auto-login để đăng nhập lại đúng hồ sơ đã chọn).
  const enterLoginDirectly = sessionStorage.getItem("hkd.enterLogin") === "1";
  sessionStorage.removeItem("hkd.enterLogin");

  await Promise.all([auth.load(), profile.load(), profile.loadPrefs()]);

  if (!enterLoginDirectly) {
    const target = profile.autoLoginTarget;
    if (target) {
      try {
        if (profile.active?.key !== target.key) {
          await profile.select(target.key);
        }
        await auth.login(target.username, target.password);
      } catch {
        // Sai mật khẩu / tài khoản bị khóa → bỏ cấu hình auto-login.
        try {
          await profile.setAutoLogin(target.key, false);
        } catch {
          /* bỏ qua */
        }
      }
    }
    // Chưa đăng nhập & máy có nhiều hồ sơ → màn chọn hồ sơ (kiểu Chrome).
    if (!auth.isLoggedIn && profile.profiles.length > 1) {
      showPicker.value = true;
    }
  }
  bootstrapped.value = true;
}

async function onPickProfile(key: string) {
  try {
    await profile.select(key);
    showPicker.value = false; // LoginView sẽ hiển thị với hồ sơ vừa chọn
  } catch (e) {
    toast.add({
      severity: "error",
      summary: "Không mở được hồ sơ",
      detail: String(e),
      life: 4000,
    });
  }
}

// ─── Cổng thông tin hộ kinh doanh ───
// Ngay sau khi đăng nhập, nếu thông tin hộ kinh doanh (tên + MST) chưa được cập nhật:
// - Tài khoản có quyền (admin): bắt buộc cập nhật ngay — dialog không đóng được cho tới khi lưu.
// - Tài khoản khác: KHÔNG cho vào app — tự đăng xuất + yêu cầu đăng nhập bằng tài khoản có quyền.
const forceBusinessConfig = ref(false);
const gateBusy = ref(false);

watch(
  () => [auth.isLoggedIn, bootstrapped.value, profile.active?.key],
  async ([loggedIn, booted]) => {
    if (!booted || !loggedIn || gateBusy.value) return;
    gateBusy.value = true;
    try {
      await business.load();
      if (business.complete) return;
      if (auth.isAdmin) {
        forceBusinessConfig.value = true; // bắt cập nhật ngay
        return;
      }
      // Không có quyền → không cho vào app.
      if (profile.active) {
        try {
          await profile.setAutoLogin(profile.active.key, false); // tránh kẹt auto-login
        } catch {
          /* bỏ qua */
        }
      }
      await auth.logout();
      toast.add({
        severity: "error",
        summary: "Chưa thể sử dụng app",
        detail:
          "Thông tin hộ kinh doanh chưa được cập nhật. Vui lòng đăng nhập bằng tài khoản quản trị (admin) để cập nhật trước khi sử dụng.",
        life: 6000,
      });
    } finally {
      gateBusy.value = false;
    }
  },
);

function onForceConfigSaved() {
  // Đã lưu xong → thông tin đủ → đóng dialog bắt buộc.
  forceBusinessConfig.value = false;
}

type MenuItem = { label: string; icon: string; to: string; roles: string[] };

const menuItems: MenuItem[] = [
  { label: "Tổng quan", icon: "pi pi-home", to: "/", roles: ["admin", "ketoan", "kho", "xem"] },
  { label: "Sản phẩm", icon: "pi pi-box", to: "/products", roles: ["admin", "ketoan", "kho"] },
  { label: "Tài khoản", icon: "pi pi-list", to: "/accounts", roles: ["admin", "ketoan"] },
  { label: "Đối tác & Kho", icon: "pi pi-users", to: "/catalogs", roles: ["admin", "ketoan", "kho"] },
  { label: "Bảng lương", icon: "pi pi-id-card", to: "/payroll", roles: ["admin", "ketoan"] },
  { label: "Chấm công", icon: "pi pi-calendar", to: "/attendance", roles: ["admin", "ketoan"] },
  { label: "Thu / Chi", icon: "pi pi-wallet", to: "/cash", roles: ["admin", "ketoan"] },
  { label: "Sổ nhật ký", icon: "pi pi-book", to: "/ledger", roles: ["admin", "ketoan", "kho", "xem"] },
  { label: "Nhập liệu", icon: "pi pi-file-excel", to: "/import", roles: ["admin", "ketoan"] },
  { label: "Nhập kho", icon: "pi pi-arrow-down", to: "/inbound", roles: ["admin", "ketoan", "kho"] },
  { label: "Xuất kho", icon: "pi pi-arrow-up", to: "/outbound", roles: ["admin", "ketoan", "kho"] },
  { label: "Tồn kho", icon: "pi pi-inbox", to: "/inventory", roles: ["admin", "ketoan", "kho", "xem"] },
  { label: "Hóa đơn", icon: "pi pi-file", to: "/invoices", roles: ["admin", "ketoan"] },
  { label: "Nhật ký HĐ", icon: "pi pi-history", to: "/audit", roles: ["admin", "ketoan", "xem"] },
  { label: "Kế toán HKD", icon: "pi pi-calculator", to: "/accounting", roles: ["admin", "ketoan"] },
  { label: "Người dùng", icon: "pi pi-user-edit", to: "/users", roles: ["admin"] },
  { label: "Hồ sơ HKD", icon: "pi pi-database", to: "/profiles", roles: ["admin", "ketoan", "kho", "xem"] },
  { label: "Cài đặt", icon: "pi pi-cog", to: "/settings", roles: ["admin", "ketoan"] },
];

const visibleMenu = computed(() =>
  menuItems.filter((m) => m.roles.includes(auth.role)),
);

const ROLE_LABEL: Record<string, string> = {
  admin: "Quản trị",
  ketoan: "Kế toán",
  kho: "Kho",
  xem: "Xem",
};

const isActive = (to: string) => route.path === to;

async function doLogout() {
  // Đăng xuất thủ công → tắt tự động đăng nhập (tránh khởi động app vào lại ngay).
  if (profile.active) {
    try {
      await profile.setAutoLogin(profile.active.key, false);
    } catch {
      /* bỏ qua */
    }
  }
  await auth.logout();
  toast.add({ severity: "info", summary: "Đã đăng xuất", life: 2000 });
  router.push("/");
}

onMounted(bootstrap);
</script>

<template>
  <!-- Đang khôi phục phiên đăng nhập → màn chờ -->
  <div v-if="auth.loading || !bootstrapped" class="flex h-screen items-center justify-center bg-gray-50">
    <ProgressSpinner />
  </div>

  <!-- Nhiều hồ sơ & chưa đăng nhập → màn chọn hồ sơ (kiểu Chrome) -->
  <ProfilePicker
    v-else-if="showPicker"
    :profiles="profile.profiles"
    :active-key="profile.active?.key"
    @select="onPickProfile"
  />

  <!-- Chưa đăng nhập → màn hình đăng nhập (che toàn bộ app) -->
  <LoginView v-else-if="!auth.isLoggedIn" />

  <div v-else class="flex h-screen overflow-hidden bg-gray-50">
    <!-- Sidebar -->
    <aside class="w-64 bg-gray-900 text-white flex flex-col shadow-xl">
      <!-- Logo -->
      <div class="px-5 py-5 border-b border-gray-700 flex items-center gap-3">
        <i class="pi pi-calculator text-2xl text-primary-400 shrink-0"></i>
        <div class="leading-tight">
          <h1 class="text-lg font-bold tracking-wide">HKD Kế Toán</h1>
          <p class="text-xs text-gray-400 mt-0.5">TT 152/2025</p>
        </div>
      </div>

      <!-- Navigation -->
      <nav class="flex-1 px-3 py-4 space-y-1 overflow-y-auto">
        <button
          v-for="item in visibleMenu"
          :key="item.to"
          @click="router.push(item.to)"
          class="w-full flex items-center gap-3 px-4 py-2.5 rounded-lg text-sm transition-all duration-150"
          :class="
            isActive(item.to)
              ? 'bg-primary-500 text-white font-medium shadow-md'
              : 'text-gray-300 hover:bg-gray-800 hover:text-white'
          "
        >
          <i :class="item.icon" class="text-base"></i>
          <span>{{ item.label }}</span>
        </button>
      </nav>

      <!-- Footer -->
      <div class="px-4 py-3 border-t border-gray-700 text-xs text-gray-500">
        v0.3.0 · SQLite
      </div>
    </aside>

    <!-- Main content -->
    <main class="flex-1 flex flex-col overflow-hidden">
      <!-- Header -->
      <header class="bg-white border-b border-gray-200 px-6 py-3 shadow-sm flex items-center justify-between">
        <h2 class="text-xl font-semibold text-gray-800">{{ pageTitle }}</h2>
        <div class="flex items-center gap-3">
          <!-- HKD đang đăng nhập → tên + MST; nhấn để quản lý/chuyển hồ sơ -->
          <button
            @click="router.push('/profiles')"
            class="flex flex-col items-end leading-tight hover:opacity-80 transition-opacity"
            v-tooltip.bottom="'Quản lý / chuyển hồ sơ hộ kinh doanh'"
          >
            <span
              class="flex items-center gap-1.5 text-sm font-semibold text-gray-800"
            >
              <i class="pi pi-building text-primary-600"></i>
              <span>{{
                business.config?.name?.trim() ||
                profile.activeName ||
                "Chưa có thông tin HKD"
              }}</span>
            </span>
            <span v-if="business.config?.tax_code" class="text-[11px] text-gray-400">
              MST: {{ business.config.tax_code }}
            </span>
          </button>
          <Divider layout="vertical" />
          <span class="text-sm text-gray-500">
            {{ auth.currentUser?.display_name || auth.currentUser?.username }}
          </span>
          <Tag :value="ROLE_LABEL[auth.role] ?? auth.role" :severity="auth.isAdmin ? 'danger' : 'info'" />
          <Avatar
            :label="(auth.currentUser?.display_name || auth.currentUser?.username || '?').slice(0, 1).toUpperCase()"
            shape="circle"
            class="bg-primary-500"
          />
          <Button
            icon="pi pi-sign-out"
            text
            rounded
            v-tooltip.bottom="'Đăng xuất'"
            aria-label="Đăng xuất"
            @click="doLogout"
          />
        </div>
      </header>

      <!-- Page content -->
      <div class="flex-1 overflow-auto p-6">
        <router-view />
      </div>
    </main>
  </div>

  <Toast position="top-right" />
  <ConfirmDialog />

  <!-- Bắt buộc cập nhật thông tin hộ kinh doanh ngay sau khi đăng nhập (chỉ admin) -->
  <BusinessConfigDialog
    :visible="forceBusinessConfig"
    :config="business.config"
    required
    @saved="onForceConfigSaved"
  />
</template>
