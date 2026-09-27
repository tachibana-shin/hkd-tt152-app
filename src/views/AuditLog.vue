<script setup lang="ts">
import { api } from "@/db";
import type { AuditEntry } from "@/types";
import { useKeepAliveRefresh } from "@/composables/useKeepAliveRefresh";

const toast = useToast();

const loading = ref(false);
const entries = ref<AuditEntry[]>([]);

const limit = ref(200);
const limitOptions = [100, 200, 500, 1000].map((n) => ({
  label: String(n),
  value: n,
}));
const filterText = ref("");

async function load() {
  loading.value = true;
  try {
    entries.value = await api.getAuditLog(limit.value);
  } catch (e) {
    toast.add({
      severity: "error",
      summary: "Lỗi tải nhật ký hoạt động",
      detail: String(e),
    });
  } finally {
    loading.value = false;
  }
}

// Lọc text client-side theo action/entity/detail
const filteredEntries = computed(() => {
  const q = filterText.value.trim().toLowerCase();
  if (!q) return entries.value;
  return entries.value.filter(
    (e) =>
      e.action.toLowerCase().includes(q) ||
      e.entity.toLowerCase().includes(q) ||
      e.detail.toLowerCase().includes(q) ||
      (e.username || "").toLowerCase().includes(q),
  );
});

// Backend đã trả mới nhất trước (ORDER BY id DESC) — giữ nguyên thứ tự.

function actionSeverity(a: string): string {
  switch (a) {
    case "save":
      return "success";
    case "backup":
      return "info";
    case "restore":
      return "danger";
    case "import":
      return "warn";
    case "link_hddt":
      return "primary";
    default:
      return "secondary";
  }
}

function actionLabel(a: string): string {
  switch (a) {
    case "save":
      return "Lưu";
    case "backup":
      return "Sao lưu";
    case "restore":
      return "Khôi phục";
    case "import":
      return "Nhập";
    case "link_hddt":
      return "Liên kết HĐĐT";
    case "create_user":
      return "Tạo người dùng";
    case "update_user":
      return "Sửa người dùng";
    case "delete_user":
      return "Xóa người dùng";
    case "change_password":
      return "Đổi mật khẩu";
    case "login":
      return "Đăng nhập";
    case "logout":
      return "Đăng xuất";
    default:
      return a;
  }
}

// ts dạng "2026-09-17 14:30:00" (datetime localtime) → "17/09/2026 14:30"
function fmtTs(ts: string): string {
  const m = ts.match(/^(\d{4})-(\d{2})-(\d{2})[T ](\d{2}):(\d{2})/);
  if (!m) return ts;
  return `${m[3]}/${m[2]}/${m[1]} ${m[4]}:${m[5]}`;
}

void load();

// Quay lại màn (KeepAlive giữ state) → nạp lại dữ liệu cho khỏi cũ.
useKeepAliveRefresh(load);
</script>

<template>
  <div class="space-y-4">
    <Toolbar>
      <template #start>
        <div class="flex items-center gap-2">
          <i-mdi-history class="text-xl text-amber-500" />
          <h3 class="font-semibold">Nhật ký hoạt động</h3>
        </div>
      </template>
      <template #end>
        <Button label="Tải lại" icon="pi pi-refresh" outlined :loading="loading" @click="load" />
      </template>
    </Toolbar>

    <Card>
      <template #content>
        <div class="flex flex-wrap items-end gap-3">
          <div>
            <label class="text-xs text-gray-500 block mb-1">Số dòng</label>
            <Select v-model="limit" :options="limitOptions" class="w-36" />
          </div>
          <div class="flex-1 min-w-52">
            <label class="text-xs text-gray-500 block mb-1"
              >Tìm theo thao tác / đối tượng / chi tiết</label
            >
            <InputText v-model="filterText" placeholder="Nhập chuỗi cần tìm..." class="w-full" />
          </div>
          <Button label="Xem" icon="pi pi-search" @click="load" />
        </div>
      </template>
    </Card>

    <Card>
      <template #content>
        <AppDataTable :value="filteredEntries" :loading="loading" stripedRows paginator :rows="20">
          <Column field="ts" header="Thời gian">
            <template #body="{ data }">{{ fmtTs(data.ts) }}</template>
          </Column>
          <Column field="action" header="Thao tác">
            <template #body="{ data }">
              <Tag :value="actionLabel(data.action)" :severity="actionSeverity(data.action)" />
            </template>
          </Column>
          <Column field="entity" header="Đối tượng" />
          <Column field="username" header="Người dùng">
            <template #body="{ data }">
              <span v-if="data.username" class="text-sm">{{ data.username }}</span>
              <span v-else class="text-xs text-gray-400">—</span>
            </template>
          </Column>
          <Column field="detail" header="Chi tiết" />
          <template #empty>
            <EmptyState
              :text="entries.length ? 'Không có dòng nào khớp bộ lọc.' : 'Chưa có hoạt động nào.'"
              icon="pi pi-history"
            />
          </template>
        </AppDataTable>
      </template>
    </Card>
  </div>
</template>
