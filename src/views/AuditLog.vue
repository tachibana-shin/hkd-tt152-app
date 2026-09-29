<script setup lang="ts">
import { api } from "@/db";
import type { AuditEntry } from "@/types";
import { useKeepAliveRefresh } from "@/composables/useKeepAliveRefresh";
import { useLazyPage } from "@/composables/useLazyPage";

const toast = useToast();

const loading = ref(false);
const entries = ref<AuditEntry[]>([]);
const total = ref(0);

// Nhật ký phình to dần theo thời gian (mỗi thao tác đều ghi 1 dòng, kèm nội
// dung chi tiết). Trước đây màn kéo sẵn tới 1000 dòng về rồi cắt trang ở trình
// duyệt nên bấm vào là đơ; giờ mỗi lần đổi trang/tìm kiếm chỉ lấy đúng trang đó.
const page = useLazyPage(async (lazy) => {
  loading.value = true;
  try {
    const res = await api.getAuditLogPage(lazy);
    entries.value = res.rows;
    total.value = res.total;
  } catch (e) {
    toast.add({
      severity: "error",
      summary: "Lỗi tải nhật ký hoạt động",
      detail: String(e),
    });
  } finally {
    loading.value = false;
  }
});

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

void page.init();

// Quay lại màn (KeepAlive giữ state) → nạp lại dữ liệu cho khỏi cũ.
useKeepAliveRefresh(page.init);
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
        <Button
          label="Tải lại"
          icon="pi pi-refresh"
          outlined
          :loading="loading"
          @click="page.reload()"
        />
      </template>
    </Toolbar>

    <Card>
      <template #content>
        <p class="text-sm text-gray-500">
          Tổng <b>{{ total.toLocaleString("vi-VN") }}</b> dòng nhật ký · mỗi lần đổi trang chỉ nạp
          đúng trang đó nên vẫn mở nhanh khi nhật ký dài ra.
        </p>
      </template>
    </Card>

    <Card>
      <template #content>
        <AppDataTable
          :value="entries"
          :loading="loading"
          :totalRecords="total"
          :first="page.first.value"
          :rows="page.rowsPerPage.value"
          :rows-per-page-options="page.pageSizes"
          data-key="id"
          sort-mode="single"
          removable-sort
          filter-toggle
          :global-filter-fields="['action', 'entity', 'detail', 'username']"
          stripedRows
          @page="page.onPage"
          @sort="page.onSort"
          @search="page.onSearch"
        >
          <Column field="ts" header="Thời gian" sortable>
            <template #body="{ data }">{{ fmtTs(data.ts) }}</template>
          </Column>
          <Column field="action" header="Thao tác" sortable>
            <template #body="{ data }">
              <Tag :value="actionLabel(data.action)" :severity="actionSeverity(data.action)" />
            </template>
          </Column>
          <Column field="entity" header="Đối tượng" sortable />
          <Column field="username" header="Người dùng" sortable>
            <template #body="{ data }">
              <span v-if="data.username" class="text-sm">{{ data.username }}</span>
              <span v-else class="text-xs text-gray-400">—</span>
            </template>
          </Column>
          <Column field="detail" header="Chi tiết" />
          <template #empty>
            <EmptyState
              :text="total ? 'Không có dòng nào khớp bộ lọc.' : 'Chưa có hoạt động nào.'"
              icon="pi pi-history"
            />
          </template>
        </AppDataTable>
      </template>
    </Card>
  </div>
</template>
