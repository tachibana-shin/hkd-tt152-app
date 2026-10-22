<script setup lang="ts">
// Ô chọn sản phẩm dùng chung (bảng dòng LineItemsEditor, dialog kiểm kê…).
// Dropdown mỗi option hiện: Tên — [ĐVT] + "Còn X" (tồn đúng kho xuất trên dòng;
// warehouseCode rỗng → tổng mọi kho; dịch vụ/nhân công → chỉ ĐVT, không tồn).
// Tồn nạp lazy 1 lần qua catalog.loadOnhand() (guard trong store).
//
// DANH SÁCH LỚN: bật VirtualScroller (`virtualScrollerOptions`) — mặt hàng +
// tên khác dễ lên hàng nghìn option, render đủ liền là khựng dấu; scroller chỉ
// dựng các dòng đang thấy (itemSize 38px khớp độ cao một dòng option).
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
  /**
   * Dòng "Tạo nhanh" — KHÔNG nằm trong danh sách option (render ở footer),
   * bấm nó là báo với người gọi rằng người dùng muốn tạo hàng mới từ tên vừa gõ.
   */
  isQuick?: boolean;
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
    /**
     * Bật dòng "Tạo nhanh" ở cuối dropdown khi người dùng gõ tên CHƯA có trong
     * danh mục (mặc định tắt — màn không được thêm sản phẩm thì không hiện).
     */
    allowCreate?: boolean;
    /**
     * Chỉ hiện hàng CHẾ TẠO (có định mức vật tư) — dùng cho popup tạo lô sản
     * xuất: chọn hàng không có định mức thì không có NVL để trừ kho.
     */
    onlyManufactured?: boolean;
  }>(),
  {
    modelValue: "",
    pickedName: "",
    warehouseCode: "",
    size: undefined,
    showClear: true,
    allowCreate: false,
    onlyManufactured: false,
  },
);

const emit = defineEmits<{
  (e: "update:modelValue", code: string): void;
  /** Tên hiển thị dòng vừa chọn ("" = tên chính). */
  (e: "picked", name: string): void;
  (e: "change"): void;
  /** Người bấm "Tạo nhanh" với đúng tên vừa gõ — cha mở form thêm sản phẩm. */
  (e: "create", name: string): void;
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
    // Chỉ hàng chế tạo (có định mức) — dropdown của popup tạo lô sản xuất.
    if (props.onlyManufactured && !catalog.hasBom(p.code)) continue;
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

/**
 * Chữ người dùng đang gõ vào ô lọc (PrimeVue giữ nội bộ — bắt qua event filter).
 * Giữ NGUYÊN khoảng trắng: nhãn "Tạo nhanh" phải chứa đúng chuỗi bộ lọc của
 * PrimeVue (contains) thì dòng mới không bị loại mất.
 */
const filterText = ref("");

function onFilter(e: { value?: string }) {
  filterText.value = e?.value ?? "";
}

/** Đóng dropdown là bỏ chữ đang gõ (PrimeVue cũng tự xóa nhờ resetFilterOnHide). */
function onHide() {
  filterText.value = "";
}

/**
 * Dòng "Tạo nhanh" — chỉ hiện khi gõ tên MỚI (chưa trùng tên chính/tên khác/mã
 * nào). Để ở slot #footer của overlay (không trong danh sách): virtual scroller
 * chỉ dựng các option đang thấy nên đặt CUỐI danh sách là bị khuất phải cuộn —
 * ở footer dòng này luôn đứng sẵn dưới cùng, gõ nhìn thấy ngay.
 */
const quickOption = computed<PickedOption | null>(() => {
  if (!props.allowCreate) return null;
  const text = filterText.value.trim();
  if (!text) return null;
  const key = text.toLowerCase();
  const taken = options.value.some(
    (o) => o.name.trim().toLowerCase() === key || o.code.trim().toLowerCase() === key,
  );
  if (taken) return null;
  return {
    code: "",
    name: `Tạo nhanh “${text}”`,
    unit: "",
    isAlias: false,
    isQuick: true,
  };
});

/** Bấm dòng "Tạo nhanh" (trong footer) — không đổi mã hàng, chỉ báo cha tạo mới. */
function onQuickClick(e?: MouseEvent) {
  const text = filterText.value.trim();
  if (!text) return;
  // Đóng overlay trực tiếp qua method hide() của Select (instance Options API):
  // giả phím Escape lên ô lọc qua dispatchEvent không đủ hiệu lực trên
  // WebKitGTK — popup còn lại sau khi dialog mở. Giữ thêm cặp phím Escape trên
  // ô lọc của CHÍNH overlay này như phương án phụ (nhiều Select cùng màn thì
  // phải khoanh đúng vùng).
  const overlay = (e?.currentTarget as HTMLElement | null)?.closest(".p-select-overlay");
  const box = overlay?.querySelector('[role="searchbox"]');
  if (box) box.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  select.value?.hide?.();
  filterText.value = "";
  emit("create", text);
}

/** Instance Select (Options API) — dùng để đóng overlay khi bấm "Tạo nhanh". */
const select = ref<{ hide?: (event?: unknown, focusAfterSelect?: boolean) => void } | null>(null);

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

/** Chữ trong dropdown khi danh mục rỗng / gõ không khớp (slot #empty). */
const emptyText = computed(() =>
  props.onlyManufactured
    ? "Chưa có mặt hàng nào khai định mức vật tư — mở màn Sản phẩm → Định mức để khai."
    : "Chưa có mặt hàng nào.",
);
const emptyFilterText = computed(() =>
  props.onlyManufactured
    ? "Không có hàng chế tạo nào khớp — chỉ nhận hàng đã khai định mức."
    : "Không tìm thấy mặt hàng phù hợp.",
);

void catalog.loadOnhand();
// Lọc theo định mức (popup tạo lô): nạp định mức 1 lần để hasBom có dữ liệu —
// store tự gộp request trùng, không bắn query mới nếu đang tải.
if (props.onlyManufactured) void catalog.loadBoms();
</script>

<template>
  <!-- Lọc theo TÊN và MÃ: người dùng quen gõ SP001 — mặc định PrimeVue chỉ lọc
       theo optionLabel (tên) nên gõ mã ra dropdown trống. -->
  <Select
    ref="select"
    :model-value="selected"
    :options="options"
    :virtual-scroller-options="{ itemSize: 38 }"
    optionLabel="name"
    :filter-fields="['name', 'code']"
    filter
    reset-filter-on-hide
    :showClear="showClear"
    :size="size"
    overlay-class="product-select-overlay"
    class="w-full"
    @update:model-value="onUpdate"
    @filter="onFilter"
    @hide="onHide"
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
    <!-- Dòng "Tạo nhanh" (gõ tên chưa có): đứng ở footer để luôn thấy —
         scroller chỉ render option đang thấy nên đặt cuối danh sách sẽ bị
         khuất phải cuộn. Bấm là báo cha mở form thêm hàng; vì đã đọc xong
         filterText nên lần click kế tiếp (nếu có) là no-op. -->
    <template #footer>
      <div
        v-if="quickOption"
        class="flex w-full cursor-pointer items-center gap-2 border-t border-gray-100 px-3 py-1.5 font-semibold text-primary-600 hover:bg-primary-50"
        data-testid="product-quick-create"
        @click="onQuickClick($event)"
      >
        <i class="pi pi-plus-circle" aria-hidden="true" />
        <span class="min-w-0 flex-1 truncate">{{ quickOption.name }}</span>
      </div>
    </template>
    <!-- Dropdown rỗng / gõ không khớp: nói rõ vì sao (hàng phải có định mức). -->
    <template #empty>
      <span class="block text-sm text-gray-500">{{ emptyText }}</span>
    </template>
    <template #emptyfilter>
      <span class="block text-sm text-gray-500">{{ emptyFilterText }}</span>
    </template>
  </Select>
</template>
