<script setup lang="ts">
// Dialog sao lưu & khôi phục cơ sở dữ liệu.
import { api } from "@/db";
import { useAuthStore } from "@/stores/auth";

const props = defineProps<{ visible: boolean }>();
const emit = defineEmits<{ (e: "update:visible", value: boolean): void }>();

const auth = useAuthStore();
const toast = useToast();
const confirm = useConfirm();

const backups = ref<string[]>([]);
const loading = ref(false);

async function load() {
  loading.value = true;
  try {
    backups.value = await api.listBackups();
  } catch (e) {
    toast.add({
      severity: "error",
      summary: "Lỗi tải danh sách sao lưu",
      detail: String(e),
    });
  } finally {
    loading.value = false;
  }
}

watch(
  () => props.visible,
  (open) => {
    if (open) load();
  },
);

async function create() {
  try {
    const name = await api.createBackup();
    toast.add({ severity: "success", summary: "Đã tạo sao lưu", detail: name });
    await load();
  } catch (e) {
    toast.add({ severity: "error", summary: "Lỗi", detail: String(e) });
  }
}

function restore(filename: string) {
  confirm.require({
    message:
      "Sẽ ghi đè toàn bộ dữ liệu hiện tại. Cần khởi động lại ứng dụng sau khi khôi phục.",
    header: "Khôi phục sao lưu",
    icon: "pi pi-exclamation-triangle",
    acceptClass: "p-button-danger",
    accept: async () => {
      try {
        await api.restoreBackup(filename);
        toast.add({
          severity: "success",
          summary: "Đã khôi phục, vui lòng khởi động lại ứng dụng",
        });
      } catch (e) {
        toast.add({
          severity: "error",
          summary: "Lỗi khôi phục",
          detail: String(e),
        });
      }
    },
  });
}
</script>

<template>
  <AppDialog
    :visible="visible"
    header="Sao lưu & khôi phục"
    width="max-w-lg"
    cancel-label="Đóng"
    :show-action="false"
    @update:visible="emit('update:visible', $event)"
  >
    <div class="flex flex-col gap-4 py-2">
      <div>
        <Button
          v-if="auth.isAdmin"
          label="Tạo sao lưu"
          icon="pi pi-database"
          @click="create"
        />
      </div>
      <AppDataTable
        :value="backups"
        :loading="loading"
        stripedRows
        size="small"
      >
        <Column header="Tên file">
          <template #body="{ data }">{{ data }}</template>
        </Column>
        <Column header="Hành động">
          <template #body="{ data }">
            <Button
              v-if="auth.isAdmin"
              label="Khôi phục"
              icon="pi pi-undo"
              severity="danger"
              size="small"
              @click="restore(data)"
            />
          </template>
        </Column>
        <template #empty
          ><EmptyState text="Chưa có bản sao lưu nào." icon="pi pi-database"
        /></template>
      </AppDataTable>
    </div>
  </AppDialog>
</template>
