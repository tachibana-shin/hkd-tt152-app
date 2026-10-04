<script setup lang="ts">
import type { JobProgress } from "@/types";

/**
 * Hộp thoại tiến độ + nhật ký cho việc backend chạy lâu (xuất báo cáo kỳ thuế).
 *
 * Không dùng sự kiện: app vừa chạy ở cửa sổ Tauri vừa chạy web server mở bằng
 * trình duyệt, mà sự kiện Tauri không tới được trình duyệt — nên màn hình chỉ
 * **hỏi lại** `getJobProgress` mỗi 300ms và vẽ ảnh chụp nhận được.
 *
 * Chân trang đổi theo trạng thái:
 * * đang chạy → "Chạy nền" (đóng hộp thoại mà việc vẫn tiếp tục ở backend);
 * * xong/lỗi  → "Đóng".
 *
 * Hộp thoại **không tự đóng** khi xong: dòng cuối nhật ký là "✓ Hoàn tất" hoặc
 * "✗ …" kèm cảnh báo PDF/XML backend gặp phải — tự đóng là mất phần người ta
 * đang ngồi chờ để đọc.
 */
const props = withDefaults(
  defineProps<{
    visible: boolean;
    /** Ảnh chụp backend trả về — `null` = backend chưa bắt đầu việc. */
    job: JobProgress | null;
    /** Dòng frontend tự ghi trước khi backend nhận việc (chạy ở máy client). */
    preLog?: string[];
    /** Việc còn đang chạy — quyết định chân trang. */
    running?: boolean;
    header?: string;
  }>(),
  { preLog: () => [], running: false, header: "Đang xử lý…" },
);

const emit = defineEmits<{
  (e: "update:visible", value: boolean): void;
}>();

/** Nhật ký = dòng frontend ghi trước + dòng backend ghi sau (theo thứ tự thời gian). */
const lines = computed(() => [...props.preLog, ...(props.job?.log ?? [])]);
const step = computed(() => props.job?.step ?? "Đang gửi yêu cầu…");
const percent = computed(() =>
  props.job && props.job.total > 0
    ? Math.min(100, Math.round((props.job.done / props.job.total) * 100))
    : 0,
);
const mode = computed<"determinate" | "indeterminate">(() =>
  props.job && props.job.total > 0 ? "determinate" : "indeterminate",
);

const logEl = ref<HTMLElement | null>(null);
// Luôn giữ dòng MỚI NHẤT trong khung — đó mới là thứ người dùng cần thấy.
watch(
  () => lines.value.length,
  () => {
    void nextTick(() => {
      const el = logEl.value;
      if (el) el.scrollTop = el.scrollHeight;
    });
  },
  { immediate: true },
);
</script>

<template>
  <AppDialog
    :visible="visible"
    :header="header"
    width="max-w-2xl"
    :show-action="false"
    :closable="false"
    :close-on-escape="false"
    @update:visible="emit('update:visible', $event)"
  >
    <div data-testid="export-progress" class="space-y-3">
      <!-- Chưa đếm được tổng việc (backend vừa nhận) → vòng quay vô hạn thay vì
           hiện % bịa. Có tổng rồi thì tỉ lệ thật: đã xong / tổng đã biết. -->
      <div data-testid="export-progress-bar">
        <ProgressBar
          :mode="mode"
          :value="percent"
          :show-value="mode === 'determinate'"
          class="h-3"
        />
      </div>

      <div class="flex items-baseline justify-between gap-3 text-sm text-gray-600">
        <span data-testid="export-progress-step" class="truncate">{{ step }}</span>
        <span v-if="job?.total" class="shrink-0 font-mono text-xs tabular-nums">
          {{ Math.min(job.done, job.total) }}/{{ job.total }}
        </span>
      </div>

      <p
        v-if="job?.error"
        class="rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700"
      >
        {{ job.error }}
      </p>

      <div
        ref="logEl"
        data-testid="export-progress-log"
        class="h-56 overflow-y-auto rounded-md border border-slate-700 bg-slate-900 p-3 font-mono text-[11px] leading-relaxed text-slate-100"
      >
        <p v-for="(line, i) in lines" :key="i" class="whitespace-pre-wrap break-words">
          {{ line }}
        </p>
        <p v-if="!lines.length" class="text-slate-400">Chưa có dòng nào…</p>
      </div>
    </div>

    <template #footer>
      <div class="flex justify-end gap-2">
        <Button
          v-if="running"
          label="Chạy nền"
          icon="pi pi-window-minimize"
          severity="secondary"
          outlined
          data-testid="export-progress-background"
          @click="emit('update:visible', false)"
        />
        <Button
          v-else
          label="Đóng"
          icon="pi pi-check"
          data-testid="export-progress-close"
          @click="emit('update:visible', false)"
        />
      </div>
    </template>
  </AppDialog>
</template>
