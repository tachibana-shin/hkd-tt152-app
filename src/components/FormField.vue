<script setup lang="ts">
withDefaults(defineProps<{ label: string; required?: boolean }>(), { required: false });
</script>

<template>
  <div class="flex flex-col gap-1.5">
    <label class="text-sm font-medium"
      >{{ label }}<span v-if="required" class="text-red-500"> *</span></label
    >
    <slot />
  </div>
</template>
