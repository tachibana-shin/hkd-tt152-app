<script setup lang="ts">
import { useViewport } from "@/composables/useViewport";

// Hộp thoại chuẩn: tiêu đề, nội dung (slot mặc định), chân trang Hủy + hành động.
// Có thể thay toàn bộ chân trang bằng <template #footer>.
const props = withDefaults(
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

// Màn hẹp: hộp thoại chiếm gần hết màn hình và tự cuộn, thay vì cao vừa
// khung khiến nội dung bị cắt.
const { isNarrow } = useViewport();
const dialogClass = computed(() =>
  [
    `w-full ${props.width}`,
    isNarrow.value ? "!max-w-full mx-0 rounded-none sm:rounded-lg" : "",
  ].join(" "),
);
</script>

<template>
  <Dialog
    :visible="visible"
    :header="header"
    :modal="true"
    :closable="closable"
    :close-on-escape="closeOnEscape"
    :class="dialogClass"
    :style="isNarrow ? { maxHeight: '96dvh' } : undefined"
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
          :disabled="!!saving"
          @click="emit('update:visible', false)"
        />
        <!-- `loading` chỉ hiện spinner, nút vẫn bấm được → bấm nhiều lần sẽ gọi
             action nhiều lần và tạo trùng bản ghi. Phải disable thật. -->
        <Button
          v-if="showAction"
          :label="actionLabel"
          :icon="actionIcon"
          :loading="saving"
          :disabled="!!saving || !!actionDisabled"
          @click="emit('action')"
        />
      </slot>
    </template>
  </Dialog>
</template>
