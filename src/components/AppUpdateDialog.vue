<script setup lang="ts">
import { useAppUpdater } from "@/composables/useAppUpdater";

/**
 * Popup "Có bản cập nhật" — việc chạy nền (`useAutoTasks`) thấy bản mới là tự
 * mở, không còn bắt người dùng tự nhớ đường vào màn Cài đặt nữa.
 *
 * Nội dung quan trọng nhất là **nhật ký từng bước**: hỏi server → tìm thấy bản
 * → tải bao nhiêu MB → cài → khởi động lại. Nên bấm "Tải và cài" biết chắc máy
 * đang làm gì, và nếu hỏng thì dòng ✗ cuối cùng nói rõ vì sao.
 *
 * Bản web (mở bằng trình duyệt) không có updater nên popup này không bao giờ
 * được mở — `checkUpdate()` trả về ngay khi không phải cửa sổ Tauri.
 */
const updater = useAppUpdater();

const mode = computed<"determinate" | "indeterminate">(() =>
  updater.contentLength.value > 0 ? "determinate" : "indeterminate",
);

function setVisible(v: boolean) {
  updater.prompt.value = v;
}

const logEl = ref<HTMLElement | null>(null);
// Giữ dòng mới nhất trong khung như hộp thoại tiến độ xuất báo cáo.
watch(
  () => updater.log.value.length,
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
    :visible="updater.prompt.value"
    :header="
      updater.available.value
        ? `Có bản cập nhật ${updater.available.value.version}`
        : 'Cập nhật ứng dụng'
    "
    width="max-w-xl"
    :show-action="false"
    :closable="!updater.installing.value"
    :close-on-escape="!updater.installing.value"
    @update:visible="setVisible"
  >
    <div data-testid="update-dialog" class="space-y-3">
      <p class="text-sm text-gray-700">
        Đang chạy bản
        <b class="font-mono">{{ updater.current.value || "?" }}</b>
        <template v-if="updater.available.value">
          → bản mới <b class="font-mono">{{ updater.available.value.version }}</b>
        </template>
        . Tải về cài luôn hay để lần sau, việc chạy nền sẽ nhắc lại ở lần kiểm tra kế tiếp.
      </p>

      <div v-if="updater.installing.value" data-testid="update-progress">
        <ProgressBar
          :mode="mode"
          :value="updater.percent.value"
          :show-value="mode === 'determinate'"
          class="h-3"
        />
      </div>

      <!-- Ghi chú phát hành (nội dung do người phát hành để trong release). -->
      <div
        v-if="updater.available.value?.notes"
        class="max-h-40 overflow-y-auto whitespace-pre-line rounded-md border border-gray-200 bg-gray-50 p-3 text-sm text-gray-600"
      >
        {{ updater.available.value.notes }}
      </div>

      <p
        v-if="updater.error.value"
        class="rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700"
        data-testid="update-error"
      >
        {{ updater.error.value }}
      </p>

      <div
        ref="logEl"
        data-testid="update-log"
        class="h-40 overflow-y-auto rounded-md border border-slate-700 bg-slate-900 p-3 font-mono text-[11px] leading-relaxed text-slate-100"
      >
        <p v-for="(line, i) in updater.log.value" :key="i" class="whitespace-pre-wrap break-words">
          {{ line }}
        </p>
        <p v-if="!updater.log.value.length" class="text-slate-400">Chưa có dòng nào…</p>
      </div>
    </div>

    <template #footer>
      <div class="flex justify-end gap-2">
        <Button
          label="Để sau"
          icon="pi pi-times"
          severity="secondary"
          outlined
          data-testid="update-later"
          :disabled="updater.installing.value"
          @click="setVisible(false)"
        />
        <Button
          v-if="updater.available.value"
          :label="`Tải và cài bản ${updater.available.value.version}`"
          icon="pi pi-download"
          data-testid="update-install"
          :loading="updater.installing.value"
          :disabled="updater.installing.value"
          @click="updater.installUpdate()"
        />
      </div>
    </template>
  </AppDialog>
</template>
