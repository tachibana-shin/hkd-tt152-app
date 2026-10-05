<script setup lang="ts">
import ReleaseNotes from "@/components/ReleaseNotes.vue";
import { useAppUpdater } from "@/composables/useAppUpdater";

/**
 * Popup "Có bản cập nhật" — việc chạy nền (`useAutoTasks`) thấy bản mới là tự
 * mở, không còn bắt người dùng tự nhớ đường vào màn Cài đặt nữa.
 *
 * Thân hộp thoại = **ghi chú phát hành** (release body, Markdown) render sẵn:
 * người dùng đọc được ngay có gì mới rồi quyết định cài. Tiến độ chỉ hiện khi
 * đang tải, dòng lỗi hiện khi có.
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
      <div v-if="updater.installing.value" data-testid="update-progress">
        <ProgressBar
          :mode="mode"
          :value="updater.percent.value"
          :show-value="mode === 'determinate'"
          class="h-3"
        />
        <p class="mt-1 text-xs text-gray-500">
          Đang tải và cài bản mới… xong app tự khởi động lại.
        </p>
      </div>

      <!-- Ghi chú phát hành (Markdown) do người phát hành để trong release. -->
      <ReleaseNotes :notes="updater.available.value?.notes" />

      <p
        v-if="updater.error.value"
        class="rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700"
        data-testid="update-error"
      >
        {{ updater.error.value }}
      </p>
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
