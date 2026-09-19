<script setup lang="ts">
// Ô chọn sản phẩm dùng chung (bảng dòng LineItemsEditor, dialog kiểm kê…).
// Dropdown mỗi option hiện: Tên — [ĐVT] + "Còn X" (tồn đúng kho xuất trên dòng;
// warehouseCode rỗng → tổng mọi kho; dịch vụ/nhân công → chỉ ĐVT, không tồn).
// Tồn nạp lazy 1 lần qua catalog.loadOnhand() (guard trong store).
import { onMounted } from "vue";
import { useCatalogStore } from "@/stores/catalog";
import { fmtInt as fmt } from "@/utils/format";

const props = withDefaults(
  defineProps<{
    /** Mã sản phẩm chọn (v-model). */
    modelValue?: string;
    /** Kho xuất trên dòng (rỗng → tồn tổng mọi kho). */
    warehouseCode?: string;
    size?: "small" | "large" | undefined;
    showClear?: boolean;
  }>(),
  {
    modelValue: "",
    warehouseCode: "",
    size: undefined,
    showClear: true,
  },
);

const emit = defineEmits<{
  (e: "update:modelValue", code: string): void;
  (e: "change"): void;
}>();

const catalog = useCatalogStore();

onMounted(() => catalog.loadOnhand());

/** Tồn còn lại đúng kho xuất trên dòng; 0 cho dịch vụ/nhân công (không cần tồn). */
function onhandOf(code: string): number {
  const p = catalog.productByCode(code);
  if (!p || p.is_service) return 0;
  return catalog.onhandAt(code, props.warehouseCode ?? "");
}
</script>

<template>
  <Select
    :model-value="modelValue"
    :options="catalog.products"
    optionLabel="name"
    optionValue="code"
    filter
    :showClear="showClear"
    :size="size"
    class="w-full"
    @change="emit('change')"
    @update:model-value="emit('update:modelValue', $event)"
  >
    <template #option="slotProps">
      <div class="flex w-full items-center justify-between gap-3">
        <span class="min-w-0 flex-1 truncate font-medium">{{
          slotProps.option.name
        }}</span>
        <span
          v-if="onhandOf(slotProps.option.code) > 0"
          class="whitespace-nowrap text-xs text-gray-500"
        >
          {{ slotProps.option.unit || "—" }} · Còn
          {{ fmt(onhandOf(slotProps.option.code)) }}
        </span>
        <span v-else-if="slotProps.option.unit" class="whitespace-nowrap text-xs text-gray-400">
          {{ slotProps.option.unit }}
        </span>
      </div>
    </template>
  </Select>
</template>
