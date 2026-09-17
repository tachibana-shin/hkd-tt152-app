<script setup lang="ts">
// Thẻ nội dung có tiêu đề + icon + ghi chú (tooltip) và vùng hành động bên phải.
defineProps<{ title?: string; tooltip?: string }>();
</script>

<template>
  <Card>
    <template v-if="title || $slots.title || $slots.icon || $slots.actions" #title>
      <div class="flex w-full items-center gap-2">
        <slot name="icon" />
        <slot name="title">{{ title }}</slot>
        <i
          v-if="tooltip"
          class="pi pi-info-circle cursor-help text-xs text-gray-400"
          v-tooltip="tooltip"
          aria-hidden="true"
        />
        <div class="ml-auto flex items-center gap-2">
          <slot name="actions" />
        </div>
      </div>
    </template>
    <template #content>
      <slot />
    </template>
  </Card>
</template>
