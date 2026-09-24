<script setup lang="ts">
// Nhãn + control. `inputId` gắn label với input qua thuộc tính `for`/`id` —
// bắt buộc cho a11y và để test/automation định vị theo nhãn hiển thị.
withDefaults(defineProps<{ label: string; required?: boolean; inputId?: string }>(), {
  required: false,
  inputId: undefined,
});
</script>

<template>
  <div class="flex flex-col gap-1.5">
    <label class="text-sm font-medium" :for="inputId">
      {{ label }}<span v-if="required" class="text-red-500"> *</span>
    </label>
    <slot :input-id="inputId" />
  </div>
</template>
