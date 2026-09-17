<script setup lang="ts">
import { useAuthStore } from "@/stores/auth";
import { api } from "@/db";
import type { AppUser, Role } from "@/types";
import { useToast } from "primevue/usetoast";
import { useConfirm } from "primevue/useconfirm";

const auth = useAuthStore();
const toast = useToast();
const confirm = useConfirm();

const users = ref<AppUser[]>([]);
const loading = ref(false);

const ROLES: { value: Role; label: string; desc: string }[] = [
  { value: "admin", label: "Quản trị (admin)", desc: "Toàn quyền, quản lý người dùng, backup" },
  { value: "ketoan", label: "Kế toán (ketoan)", desc: "Sổ sách, thuế, lương, thu/chi, chấm công" },
  { value: "kho", label: "Kho (kho)", desc: "Nhập/xuất/tồn, danh mục sản phẩm" },
  { value: "xem", label: "Xem (xem)", desc: "Chỉ đọc, không ghi dữ liệu" },
];

const ROLE_HINT = "admin: toàn quyền · ketoan: sổ sách/thuế/lương · kho: nhập/xuất/tồn · xem: chỉ đọc";

async function load() {
  loading.value = true;
  try {
    users.value = await api.listUsers();
  } catch (e) {
    toast.add({ severity: "error", summary: "Lỗi", detail: String(e), life: 3000 });
  } finally {
    loading.value = false;
  }
}

// ── Add / Edit dialog ──
const dialogVisible = ref(false);
const editing = ref<AppUser | null>(null);
const form = ref({ username: "", display_name: "", password: "", role: "ketoan" as Role, active: true });

function openCreate() {
  editing.value = null;
  form.value = { username: "", display_name: "", password: "", role: "ketoan", active: true };
  dialogVisible.value = true;
}

function openEdit(u: AppUser) {
  if (u.role === "admin" && auth.currentUser?.id !== u.id) {
    toast.add({ severity: "warn", summary: "Không cho phép", detail: "Chỉ quản trị viên sở hữu tài khoản này mới sửa được", life: 3000 });
    return;
  }
  editing.value = u;
  form.value = { username: u.username, display_name: u.display_name, password: "", role: u.role, active: u.active };
  dialogVisible.value = true;
}

async function save() {
  try {
    await api.saveUser({
      username: form.value.username,
      display_name: form.value.display_name,
      password: form.value.password,
      role: form.value.role,
      active: form.value.active,
    });
    toast.add({ severity: "success", summary: "Đã lưu", detail: "Thông tin người dùng đã được cập nhật", life: 2500 });
    dialogVisible.value = false;
    await load();
    // Nếu đang sửa chính mình thì làm mới phiên để phản ánh vai trò mới
    if (editing.value?.username === auth.currentUser?.username) {
      await auth.load();
    }
  } catch (e) {
    toast.add({ severity: "error", summary: "Lỗi lưu", detail: String(e), life: 4000 });
  }
}

function remove(u: AppUser) {
  confirm.require({
    message: `Xóa người dùng "${u.username}"?`,
    header: "Xác nhận xóa",
    acceptLabel: "Xóa",
    rejectLabel: "Hủy",
    accept: async () => {
      try {
        await api.deleteUser(u.id);
        toast.add({ severity: "success", summary: "Đã xóa", detail: `Đã xóa ${u.username}`, life: 2500 });
        await load();
      } catch (e) {
        toast.add({ severity: "error", summary: "Không xóa được", detail: String(e), life: 4000 });
      }
    },
  });
}

// ── Đổi mật khẩu của chính mình ──
const pwDialog = ref(false);
const pw = ref({ old: "", next: "" });
const pwSaving = ref(false);

function openPw() {
  pw.value = { old: "", next: "" };
  pwDialog.value = true;
}

async function savePw() {
  if (pw.value.next.length < 4) {
    toast.add({ severity: "warn", summary: "Mật khẩu quá ngắn", detail: "Tối thiểu 4 ký tự", life: 2500 });
    return;
  }
  pwSaving.value = true;
  try {
    await api.changePassword(pw.value.old, pw.value.next);
    toast.add({ severity: "success", summary: "Đã đổi mật khẩu", life: 2500 });
    pwDialog.value = false;
  } catch (e) {
    toast.add({ severity: "error", summary: "Đổi mật khẩu thất bại", detail: String(e), life: 3500 });
  } finally {
    pwSaving.value = false;
  }
}

const roleLabel = (r: string) => ROLES.find((x) => x.value === r)?.label.split(" (")[0] ?? r;

load();
</script>

<template>
  <div class="space-y-4">
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-2">
        <i class="pi pi-user-edit text-lg text-primary-600"></i>
        <h3 class="text-lg font-semibold text-gray-800">Người dùng &amp; vai trò</h3>
        <i
          class="pi pi-info-circle text-sm text-gray-400 cursor-help"
          v-tooltip.right="ROLE_HINT"
        ></i>
      </div>
      <div class="flex gap-2">
        <Button label="Đổi mật khẩu" icon="pi pi-key" severity="secondary" outlined @click="openPw" />
        <Button label="Thêm người dùng" icon="pi pi-plus" @click="openCreate" />
      </div>
    </div>

    <Card>
      <template #content>
        <DataTable :value="users" :loading="loading" stripedRows size="small">
          <Column field="username" header="Tên đăng nhập" />
          <Column field="display_name" header="Họ tên" />
          <Column header="Vai trò">
            <template #body="{ data }">
              <Tag
                :value="roleLabel(data.role)"
                :severity="
                  data.role === 'admin'
                    ? 'danger'
                    : data.role === 'ketoan'
                      ? 'info'
                      : data.role === 'kho'
                        ? 'warning'
                        : 'secondary'
                "
              />
            </template>
          </Column>
          <Column header="Trạng thái">
            <template #body="{ data }">
              <Tag :value="data.active ? 'Hoạt động' : 'Khóa'" :severity="data.active ? 'success' : 'danger'" />
            </template>
          </Column>
          <Column header="Thao tác" style="width: 160px">
            <template #body="{ data }">
              <div class="flex gap-1">
                <Button
                  icon="pi pi-pencil"
                  text
                  rounded
                  size="small"
                  v-tooltip.top="'Sửa'"
                  @click="openEdit(data)"
                />
                <Button
                  icon="pi pi-trash"
                  text
                  rounded
                  size="small"
                  severity="danger"
                  v-tooltip.top="'Xóa'"
                  :disabled="data.id === auth.currentUser?.id"
                  @click="remove(data)"
                />
              </div>
            </template>
          </Column>
        </DataTable>
      </template>
    </Card>

    <!-- Add / Edit dialog -->
    <AppDialog
      v-model:visible="dialogVisible"
      :header="editing ? 'Sửa người dùng' : 'Thêm người dùng'"
      width="max-w-[480px]"
      action-label="Lưu"
      @action="save"
    >
      <div class="space-y-3">
        <FormField label="Tên đăng nhập" required>
          <InputText v-model="form.username" class="w-full" :disabled="!!editing" placeholder="vd: ketoan01" />
        </FormField>
        <FormField label="Họ tên hiển thị">
          <InputText v-model="form.display_name" class="w-full" placeholder="vd: Nguyễn Thị Kế Toán" />
        </FormField>
        <FormField :label="editing ? 'Mật khẩu (để trống = giữ nguyên)' : 'Mật khẩu'" :required="!editing">
          <InputText v-model="form.password" type="password" class="w-full" :placeholder="editing ? '••••••••' : 'Tối thiểu 4 ký tự'" />
        </FormField>
        <FormField label="Vai trò" required>
          <Select v-model="form.role" :options="ROLES" option-label="label" option-value="value" class="w-full" />
          <p class="mt-1 text-xs text-gray-500">
            {{ ROLES.find((r) => r.value === form.role)?.desc }}
          </p>
        </FormField>
        <div class="flex items-center gap-2">
          <Checkbox v-model="form.active" :binary="true" input-id="user-active" />
          <label for="user-active" class="text-sm text-gray-700">Tài khoản hoạt động</label>
        </div>
      </div>
    </AppDialog>

    <!-- Change own password -->
    <AppDialog
      v-model:visible="pwDialog"
      header="Đổi mật khẩu"
      width="max-w-[420px]"
      action-label="Đổi mật khẩu"
      action-icon="pi pi-key"
      :saving="pwSaving"
      @action="savePw"
    >
      <div class="space-y-3">
        <FormField label="Mật khẩu hiện tại">
          <InputText v-model="pw.old" type="password" class="w-full" />
        </FormField>
        <FormField label="Mật khẩu mới">
          <InputText v-model="pw.next" type="password" class="w-full" />
        </FormField>
      </div>
    </AppDialog>
  </div>
</template>
