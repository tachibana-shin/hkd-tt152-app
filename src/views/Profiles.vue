<script setup lang="ts">
import { useProfileStore } from "@/stores/profile";
import { useAuthStore } from "@/stores/auth";
import { useBusinessStore } from "@/stores/business";
import { useToast } from "primevue/usetoast";
import { useConfirm } from "primevue/useconfirm";
import { useKeepAliveRefresh } from "@/composables/useKeepAliveRefresh";

const profile = useProfileStore();
const auth = useAuthStore();
const business = useBusinessStore();
const toast = useToast();
const confirm = useConfirm();

// ── Tạo hồ sơ ──
const createVisible = ref(false);
const newName = ref("");
const creating = ref(false);

function openCreate() {
  newName.value = "";
  createVisible.value = true;
}

async function doCreate() {
  const name = newName.value.trim();
  if (!name) {
    toast.add({
      severity: "warn",
      summary: "Thiếu tên",
      detail: "Nhập tên hồ sơ HKD",
      life: 2500,
    });
    return;
  }
  creating.value = true;
  try {
    await profile.create(name);
    toast.add({
      severity: "success",
      summary: "Đã tạo hồ sơ",
      detail: `"${name}" — nhấn Mở để đăng nhập`,
      life: 3000,
    });
    createVisible.value = false;
  } catch (e) {
    toast.add({
      severity: "error",
      summary: "Không tạo được",
      detail: String(e),
      life: 4000,
    });
  } finally {
    creating.value = false;
  }
}

// ── Đổi tên ──
const renameVisible = ref(false);
const renameKey = ref("");
const renameName = ref("");
const renaming = ref(false);

function openRename(p: { key: string; name: string }) {
  renameKey.value = p.key;
  renameName.value = p.name;
  renameVisible.value = true;
}

async function doRename() {
  const name = renameName.value.trim();
  if (!name) {
    toast.add({
      severity: "warn",
      summary: "Thiếu tên",
      detail: "Nhập tên hồ sơ HKD",
      life: 2500,
    });
    return;
  }
  renaming.value = true;
  try {
    await profile.rename(renameKey.value, name);
    toast.add({ severity: "success", summary: "Đã đổi tên", life: 2500 });
    renameVisible.value = false;
  } catch (e) {
    toast.add({
      severity: "error",
      summary: "Không đổi được",
      detail: String(e),
      life: 4000,
    });
  } finally {
    renaming.value = false;
  }
}

// ── Xóa hồ sơ ──
function doDelete(p: { key: string; name: string }) {
  confirm.require({
    message: `Xóa hồ sơ "${p.name}" và toàn bộ dữ liệu kế toán của hộ này?`,
    header: "Xác nhận xóa hồ sơ",
    acceptLabel: "Xóa",
    rejectLabel: "Hủy",
    accept: async () => {
      try {
        await profile.remove(p.key);
        toast.add({
          severity: "success",
          summary: "Đã xóa hồ sơ",
          detail: p.name,
          life: 2500,
        });
      } catch (e) {
        toast.add({
          severity: "error",
          summary: "Không xóa được",
          detail: String(e),
          life: 4000,
        });
      }
    },
  });
}

// ── Mở hồ sơ (đổi hồ sơ đang dùng) ──
function switchTo(p: { key: string; name: string }) {
  confirm.require({
    message: `Mở hồ sơ "${p.name}"? Đăng nhập lại bằng tài khoản của hồ sơ này.`,
    header: "Chuyển hồ sơ HKD",
    acceptLabel: "Mở",
    rejectLabel: "Hủy",
    accept: async () => {
      try {
        const res = await profile.switchTo(p.key);
        toast.add({
          severity: "success",
          summary: "Đã mở hồ sơ",
          detail: `"${res.name}" — đang làm mới...`,
          life: 1500,
        });
        // Sau reload phải vào thẳng màn đăng nhập của hồ sơ mới
        // (không hiện picker, không auto-login).
        sessionStorage.setItem("hkd.enterLogin", "1");
        setTimeout(() => window.location.reload(), 900);
      } catch (e) {
        toast.add({
          severity: "error",
          summary: "Không mở được",
          detail: String(e),
          life: 4000,
        });
      }
    },
  });
}

// ── Tự động đăng nhập theo hồ sơ ──
const autoKey = ref("");
const autoUsername = ref("");
const autoPassword = ref("");
const autoVisible = ref(false);
const autoSaving = ref(false);

function openAutoLogin(p: { key: string }) {
  autoKey.value = p.key;
  autoUsername.value = profile.prefsOf(p.key).last_username ?? "";
  autoPassword.value = "";
  autoVisible.value = true;
}

async function doAutoLogin() {
  const user = autoUsername.value.trim();
  if (!user || !autoPassword.value) {
    toast.add({
      severity: "warn",
      summary: "Thiếu thông tin",
      detail: "Nhập tên đăng nhập và mật khẩu của hồ sơ này",
      life: 2500,
    });
    return;
  }
  autoSaving.value = true;
  try {
    await profile.setAutoLogin(autoKey.value, true, user, autoPassword.value);
    toast.add({
      severity: "success",
      summary: "Đã bật tự động đăng nhập",
      detail: `Khởi động app sẽ tự đăng nhập vào hồ sơ này bằng "${user}"`,
      life: 3000,
    });
    autoVisible.value = false;
  } catch (e) {
    toast.add({
      severity: "error",
      summary: "Không lưu được",
      detail: String(e),
      life: 4000,
    });
  } finally {
    autoSaving.value = false;
  }
}

async function toggleAutoLogin(p: { key: string }) {
  const pref = profile.prefsOf(p.key);
  if (pref.auto_login && pref.username && pref.password) {
    try {
      await profile.setAutoLogin(p.key, false);
      toast.add({
        severity: "info",
        summary: "Đã tắt tự động đăng nhập",
        detail: "Mật khẩu đã lưu cũng được xóa",
        life: 2500,
      });
    } catch (e) {
      toast.add({ severity: "error", summary: "Lỗi", detail: String(e), life: 4000 });
    }
  } else {
    // Chưa có thông tin đăng nhập → hỏi tài khoản + mật khẩu của hồ sơ đó.
    openAutoLogin(p);
  }
}

function reload() {
  void profile.load();
  void profile.loadPrefs();
  void business.load();
}

void reload();

// Quay lại màn (KeepAlive giữ state) → nạp lại dữ liệu cho khỏi cũ.
useKeepAliveRefresh(reload);

// ── Cài đặt thông tin hộ kinh doanh (tên, MST, địa chỉ…) ──
const bizConfigVisible = ref(false);
</script>

<template>
  <div class="space-y-4">
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-2">
        <i class="pi pi-database text-lg text-primary-600"></i>
        <h3 class="text-lg font-semibold text-gray-800">Hồ sơ hộ kinh doanh</h3>
        <i
          class="pi pi-info-circle text-sm text-gray-400 cursor-help"
          v-tooltip.right="
            'Mỗi hồ sơ có file dữ liệu riêng (hóa đơn, kho, lương, thuế, người dùng). Chuyển hồ sơ phải đăng nhập lại bằng tài khoản của hồ sơ đó.'
          "
        ></i>
      </div>
      <Button
        v-if="auth.isAdmin"
        label="Cài đặt thông tin HKD"
        icon="pi pi-building"
        severity="secondary"
        @click="bizConfigVisible = true"
      />
      <Button v-if="auth.isAdmin" label="Tạo hồ sơ mới" icon="pi pi-plus" @click="openCreate" />
    </div>

    <Card>
      <template #content>
        <AppDataTable
          :value="profile.profiles"
          :loading="profile.loading"
          stripedRows
          size="small"
          :row-class="(row: { active: boolean }) => (row.active ? 'highlight-row' : '')"
        >
          <Column field="name" header="Tên hồ sơ">
            <template #body="{ data }">
              <span class="font-medium">{{ data.name }}</span>
              <i v-if="data.active" class="pi pi-check-circle text-primary-600 ml-1"></i>
            </template>
          </Column>
          <Column field="key" header="Thư mục dữ liệu" />
          <Column field="created_at" header="Ngày tạo" />
          <Column header="Trạng thái">
            <template #body="{ data }">
              <Tag
                :value="data.active ? 'Đang mở' : 'Đóng'"
                :severity="data.active ? 'success' : 'secondary'"
              />
            </template>
          </Column>
          <Column header="Tự động đăng nhập" style="min-width: 170px">
            <template #body="{ data }">
              <div class="flex flex-col items-start gap-1">
                <ToggleSwitch
                  :model-value="!!profile.prefsOf(data.key).auto_login"
                  :disabled="profile.switching"
                  v-tooltip.top="
                    profile.prefsOf(data.key).auto_login
                      ? 'Tự đăng nhập bằng tài khoản đã lưu mỗi lần khởi động app'
                      : 'Bật để khởi động app tự đăng nhập hồ sơ này (cần khai báo tài khoản)'
                  "
                  @update:model-value="toggleAutoLogin(data)"
                />
                <span
                  v-if="profile.prefsOf(data.key).auto_login && profile.prefsOf(data.key).username"
                  class="text-xs text-gray-400"
                >
                  Tài khoản: <b>{{ profile.prefsOf(data.key).username }}</b>
                </span>
                <span v-else class="text-xs text-gray-400">Chưa cấu hình</span>
              </div>
            </template>
          </Column>
          <Column header="Thao tác">
            <template #body="{ data }">
              <div class="flex gap-1">
                <Button
                  icon="pi pi-folder-open"
                  text
                  rounded
                  size="small"
                  :label="data.active ? 'Đang mở' : 'Mở'"
                  :disabled="data.active || profile.switching"
                  v-tooltip.top="'Mở hồ sơ này'"
                  @click="switchTo(data)"
                />
                <Button
                  v-if="auth.isAdmin"
                  icon="pi pi-pencil"
                  text
                  rounded
                  size="small"
                  v-tooltip.top="'Đổi tên'"
                  @click="openRename(data)"
                />
                <Button
                  v-if="auth.isAdmin"
                  icon="pi pi-trash"
                  text
                  rounded
                  size="small"
                  severity="danger"
                  v-tooltip.top="'Xóa hồ sơ'"
                  :disabled="data.active"
                  @click="doDelete(data)"
                />
              </div>
            </template>
          </Column>
        </AppDataTable>
      </template>
    </Card>

    <!-- Tạo hồ sơ -->
    <AppDialog
      v-model:visible="createVisible"
      header="Tạo hồ sơ HKD mới"
      width="max-w-[460px]"
      action-label="Tạo hồ sơ"
      :saving="creating"
      @action="doCreate"
    >
      <div class="space-y-3">
        <div class="flex items-center gap-2 text-sm text-gray-500">
          <i class="pi pi-info-circle shrink-0 text-sky-500" />
          <span>Dữ liệu riêng biệt; tài khoản quản trị mặc định <b>admin/admin123</b>.</span>
        </div>
        <FormField label="Tên hồ sơ" required>
          <InputText
            v-model="newName"
            class="w-full"
            placeholder="vd: Tạp hóa Minh Anh — 268B Trần Phú"
            @keyup.enter="doCreate"
          />
        </FormField>
      </div>
    </AppDialog>

    <!-- Đổi tên -->
    <AppDialog
      v-model:visible="renameVisible"
      header="Đổi tên hồ sơ"
      width="max-w-[420px]"
      action-label="Lưu"
      :saving="renaming"
      @action="doRename"
    >
      <FormField label="Tên hồ sơ" required>
        <InputText v-model="renameName" class="w-full" @keyup.enter="doRename" />
      </FormField>
    </AppDialog>

    <!-- Bật tự động đăng nhập (cần tài khoản + mật khẩu của hồ sơ) -->
    <AppDialog
      v-model:visible="autoVisible"
      header="Tự động đăng nhập"
      width="max-w-[420px]"
      action-label="Bật"
      :saving="autoSaving"
      @action="doAutoLogin"
    >
      <div class="space-y-3">
        <div class="flex items-center gap-2 text-sm text-gray-500">
          <i class="pi pi-info-circle shrink-0 text-sky-500" />
          <span
            >Khởi động app lần sau sẽ tự đăng nhập hồ sơ này bằng đúng tài khoản bên dưới (mật khẩu
            được lưu trên máy này).</span
          >
        </div>
        <FormField label="Tên đăng nhập" required>
          <InputText
            v-model="autoUsername"
            class="w-full"
            placeholder="admin"
            @keyup.enter="doAutoLogin"
          />
        </FormField>
        <FormField label="Mật khẩu" required>
          <InputText
            v-model="autoPassword"
            type="password"
            class="w-full"
            placeholder="••••••••"
            @keyup.enter="doAutoLogin"
          />
        </FormField>
      </div>
    </AppDialog>

    <!-- Cài đặt thông tin hộ kinh doanh -->
    <BusinessConfigDialog
      v-model:visible="bizConfigVisible"
      :config="business.config"
      @saved="business.load"
    />
  </div>
</template>

<style scoped>
:deep(.highlight-row) {
  background: var(--p-primary-50);
}
</style>
