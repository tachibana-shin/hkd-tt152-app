<script setup lang="ts">
// Ô chọn sản phẩm dùng chung (bảng dòng LineItemsEditor, dialog kiểm kê…).
// Dropdown mỗi option hiện: Tên — [ĐVT] + "Còn X" (tồn đúng kho xuất trên dòng;
// warehouseCode rỗng → tổng mọi kho; dịch vụ/nhân công → chỉ ĐVT, không tồn).
// Tồn nạp lazy 1 lần qua catalog.loadOnhand() (guard trong store).
//
// F5 — tên khác (alias): mỗi sản phẩm đóng góp 1 option tên CHÍNH + 1 option cho
// MỖI tên khác (cùng mã sản phẩm). Ô vẫn ghi mã hàng qua `modelValue`, nhưng
// emit thêm `picked(tên)` để dòng dữ liệu ghi đúng tên người dùng đã chọn
// (`line_name`) — in / xem hóa đơn giữ nguyên tên đó, KHÔNG fallback về tên
// chính. Chọn tên chính → `picked("")` (= dùng tên sản phẩm).
// PrimeVue: `filter` mặc định lọc theo `optionLabel` nên gõ tên nào cũng ra.
import { useCatalogStore } from "@/stores/catalog";
import { fmtInt as fmt } from "@/utils/format";

/** 1 option trong dropdown: tên chính hoặc 1 tên khác, đều trỏ về cùng mã SP. */
interface PickedOption {
  code: string;
  name: string;
  unit: string;
  /** Đang hiển thị một "tên khác" chứ không phải tên chính của sản phẩm. */
  isAlias: boolean;
}

const props = withDefaults(
  defineProps<{
    /** Mã sản phẩm chọn (v-model). */
    modelValue?: string;
    /**
     * Tên người dùng đã chọn cho dòng này (line_name) — để mở lại vẫn hiện đúng
     * tên alias đã chọn thay vì quay về tên chính. Rỗng → tên chính.
     */
    pickedName?: string;
    /** Kho xuất trên dòng (rỗng → tồn tổng mọi kho). */
    warehouseCode?: string;
    size?: "small" | "large" | undefined;
    showClear?: boolean;
  }>(),
  {
    modelValue: "",
    pickedName: "",
    warehouseCode: "",
    size: undefined,
    showClear: true,
  },
);

const emit = defineEmits<{
  (e: "update:modelValue", code: string): void;
  /** Tên hiển thị dòng vừa chọn ("" = tên chính). */
  (e: "picked", name: string): void;
  (e: "change"): void;
}>();

const catalog = useCatalogStore();

/** Danh sách "tên khác" của 1 sản phẩm (lưu phân tách phẩy). */
function splitAliases(aliases?: string): string[] {
  if (!aliases) return [];
  return aliases
    .split(",")
    .map((a) => a.trim())
    .filter(Boolean);
}

const options = computed<PickedOption[]>(() => {
  const out: PickedOption[] = [];
  for (const p of catalog.products) {
    out.push({ code: p.code, name: p.name, unit: p.unit ?? "", isAlias: false });
    for (const alias of splitAliases(p.aliases)) {
      out.push({ code: p.code, name: alias, unit: p.unit ?? "", isAlias: true });
    }
  }
  return out;
});

function unitOf(code: string): string {
  return catalog.productByCode(code)?.unit ?? "";
}

/** Option đang hiển thị: ưu tiên đúng tên đã chọn (`pickedName`), không fallback. */
const selected = computed<PickedOption | null>(() => {
  const code = props.modelValue ?? "";
  if (!code) return null;
  const picked = (props.pickedName ?? "").trim();
  if (picked) {
    const hit = options.value.find((o) => o.code === code && o.name === picked);
    if (hit) return hit;
    // Tên đã lưu nhưng danh mục chưa nạp / tên khác vừa bị xóa → vẫn hiện tên đó.
    return { code, name: picked, unit: unitOf(code), isAlias: true };
  }
  const main = options.value.find((o) => o.code === code && !o.isAlias);
  if (main) return main;
  return {
    code,
    name: catalog.productByCode(code)?.name ?? code,
    unit: unitOf(code),
    isAlias: false,
  };
});

function onUpdate(v: PickedOption | null) {
  if (!v) {
    emit("update:modelValue", "");
    emit("picked", "");
    emit("change");
    return;
  }
  emit("update:modelValue", v.code);
  emit("picked", v.isAlias ? v.name : "");
  emit("change");
}

/** Tồn còn lại đúng kho xuất trên dòng; 0 cho dịch vụ/nhân công (không cần tồn). */
function onhandOf(code: string): number {
  const p = catalog.productByCode(code);
  if (!p || p.is_service) return 0;
  return catalog.onhandAt(code, props.warehouseCode ?? "");
}

void catalog.loadOnhand();
</script>

<template>
  <Select
    :model-value="selected"
    :options="options"
    optionLabel="name"
    filter
    :showClear="showClear"
    :size="size"
    class="w-full"
    @update:model-value="onUpdate"
  >
    <template #option="slotProps">
      <div class="flex w-full items-center justify-between gap-3">
        <span class="min-w-0 flex-1 truncate font-medium">{{ slotProps.option.name }}</span>
        <span v-if="slotProps.option.isAlias" class="whitespace-nowrap text-xs text-gray-400">
          tên khác
        </span>
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
