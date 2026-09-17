<script setup lang="ts">
withDefaults(defineProps<{ text?: string; icon?: string }>(), {
  text: "Chưa có dữ liệu.",
  icon: "pi pi-inbox",
});
</script>

<template>
  <div class="flex flex-col items-center gap-2 py-6 text-gray-400">
    <i :class="icon" class="text-2xl" aria-hidden="true" />
    <span class="text-sm">{{ text }}</span>
  </div>
</template>
