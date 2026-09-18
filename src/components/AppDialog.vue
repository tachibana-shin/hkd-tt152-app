<script setup lang="ts">
// Hộp thoại chuẩn: tiêu đề, nội dung (slot mặc định), chân trang Hủy + hành động.
// Có thể thay toàn bộ chân trang bằng <template #footer>.
withDefaults(
  defineProps<{
    visible: boolean;
    header: string;
    /** Lớp Tailwind cho chiều rộng, vd "max-w-3xl". */
    width?: string;
    /** Đặt null để ẩn nút hủy. */
    cancelLabel?: string | null;
    actionLabel?: string;
    actionIcon?: string;
    saving?: boolean;
    actionDisabled?: boolean;
    showAction?: boolean;
    showFooter?: boolean;
    /** Ẩn nút X ở góc (dùng cho dialog bắt buộc phải hoàn tất). */
    closable?: boolean;
    /** Cho phép bấm Esc đóng dialog hay không. */
    closeOnEscape?: boolean;
  }>(),
  {
    width: "max-w-2xl",
    cancelLabel: "Hủy",
    actionLabel: "Lưu",
    actionIcon: "pi pi-check",
    showAction: true,
    showFooter: true,
    closable: true,
    closeOnEscape: true,
  },
);

const emit = defineEmits<{
  (e: "update:visible", value: boolean): void;
  (e: "action"): void;
}>();
</script>

<template>
  <Dialog
    :visible="visible"
    :header="header"
    :modal="true"
    :closable="closable"
    :close-on-escape="closeOnEscape"
    :class="`w-full ${width}`"
    @update:visible="emit('update:visible', $event)"
  >
    <slot />
    <template v-if="showFooter" #footer>
      <slot name="footer">
        <Button
          v-if="cancelLabel !== null"
          :label="cancelLabel"
          icon="pi pi-times"
          severity="secondary"
          outlined
          @click="emit('update:visible', false)"
        />
        <Button
          v-if="showAction"
          :label="actionLabel"
          :icon="actionIcon"
          :loading="saving"
          :disabled="actionDisabled"
          @click="emit('action')"
        />
      </slot>
    </template>
  </Dialog>
</template>
