<script setup lang="ts">
import { useProfileStore } from "@/stores/profile";
import { useAuthStore } from "@/stores/auth";
import { useToast } from "primevue/usetoast";
import { useConfirm } from "primevue/useconfirm";

const profile = useProfileStore();
const auth = useAuthStore();
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

profile.load();
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
        label="Tạo hồ sơ mới"
        icon="pi pi-plus"
        @click="openCreate"
      />
    </div>

    <Card>
      <template #content>
        <AppDataTable
          :value="profile.profiles"
          :loading="profile.loading"
          stripedRows
          size="small"
          :row-class="
            (row: { active: boolean }) => (row.active ? 'highlight-row' : '')
          "
        >
          <Column field="name" header="Tên hồ sơ">
            <template #body="{ data }">
              <span class="font-medium">{{ data.name }}</span>
              <i
                v-if="data.active"
                class="pi pi-check-circle text-primary-600 ml-1"
              ></i>
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
          <span
            >Dữ liệu riêng biệt; tài khoản quản trị mặc định
            <b>admin/admin123</b>.</span
          >
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
        <InputText
          v-model="renameName"
          class="w-full"
          @keyup.enter="doRename"
        />
      </FormField>
    </AppDialog>
  </div>
</template>

<style scoped>
:deep(.highlight-row) {
  background: var(--p-primary-50);
}
</style>
