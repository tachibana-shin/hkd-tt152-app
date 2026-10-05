<script setup lang="ts">
// Bảng nhập dòng mặt hàng dùng chung cho phiếu nhập / phiếu xuất / hóa đơn.
import { useCatalogStore } from "@/stores/catalog";
import ProductDialog from "@/components/ProductDialog.vue";
import ProductAliasDialog from "@/components/ProductAliasDialog.vue";
import { fmtInt as fmt, fmtVnd } from "@/utils/format";
import { useClientPage } from "@/composables/useClientPage";
import type { IndustryGroup, Product, Warehouse } from "@/types";

interface LineItem {
  product_code: string;
  quantity: number;
  unit_price: number;
  industry_code?: string;
  warehouse_code?: string;
  /** Số tiền chiết khấu thương mại (đ) — cột "Tiền CK" của CT MH (TT88). */
  discount?: number;
  /**
   * Tên hiển thị dòng (tên khác / alias người dùng chọn trên ô sản phẩm).
   * Rỗng = tên chính. Hóa đơn giữ nguyên tên này khi in (F5).
   */
  line_name?: string;
}

const props = withDefaults(
  defineProps<{
    items: LineItem[];
    products: Product[];
    industryGroups?: IndustryGroup[];
    warehouses?: Warehouse[];
    canEdit?: boolean;
    showIndustry?: boolean;
    showWarehouse?: boolean;
    showAmount?: boolean;
    /** Hiện cột CK% + Tiền CK (thay cột Thành tiền) — dùng cho phiếu nhập mua. */
    showDiscount?: boolean;
    /** Hiện cột ĐVT (đơn vị tính) chỉ đọc trên từng dòng. */
    showUnit?: boolean;
    /**
     * Cột Thành tiền nhập trực tiếp (HĐĐT kiểu HKD): gõ số tiền cả dòng,
     * tự tính lại Đơn giá = Thành tiền ÷ SL để tổng không lệch.
     */
    amountInput?: boolean;
    /**
     * Chế độ gọn (bảng Mặt hàng của popup phiếu nhập kho): bảng size small,
     * cột số hẹp, cột sản phẩm co giãn rộng, không scroll ngang, và nhập liên tục
     * (pre-input) — điền xong dòng cuối tự thêm dòng trống, không cần nút "Thêm dòng".
     */
    compact?: boolean;
    /**
     * Nhập liên tục + size small; cột có độ rộng cố định để bảng không bị
     * phình (cột Sản phẩm lấy phần còn lại, scroll ngang khi chật). Dùng cho
     * phiếu xuất — có thêm cột Nhóm ngành + Kho xuất.
     */
    preInput?: boolean;
    priceField?: "cost_price" | "sale_price";
    info?: string;
    totalLabel?: string;
    /** Hiện nút "+ Thêm hàng hóa" (chưa có trong danh mục thì mở ProductDialog). */
    showAddProduct?: boolean;
  }>(),
  {
    canEdit: true,
    showIndustry: false,
    showWarehouse: false,
    showAmount: false,
    showDiscount: false,
    showUnit: false,
    amountInput: false,
    compact: false,
    preInput: false,
    showAddProduct: false,
    priceField: "cost_price",
    industryGroups: () => [],
    warehouses: () => [],
    totalLabel: "Tổng tiền:",
  },
);

const emit = defineEmits<{
  (e: "add"): void;
  (e: "remove", index: number): void;
  /** Người dùng vừa CHỌN hàng trên dòng (sau khi đã tự điền giá/kho). */
  (e: "picked", line: LineItem): void;
}>();

const catalog = useCatalogStore();
const toast = useToast();

// Phân trang phía client: đổ file Excel vào phiếu nhập có thể vài nghìn dòng —
// bảng chỉ render 1 trang (50 dòng) thay vì hàng nghìn <tr> (mỗi dòng có ô chọn
// hàng → sẽ treo trình duyệt). Dữ liệu ≤ 1 trang thì giữ nguyên như cũ, không
// hiện thanh phân trang thừa. Gọi hàng loạt xong vẫn ở trang đầu để soát từ trên.
//
// Ô tìm kiếm "Tìm kiếm…" ở header (AppDataTable có sẵn cho mọi bảng) lọc ngay
// trên máy theo tên/mã hàng — bảng dài mới tìm được dòng cần soát. PrimeVue
// không tự lọc vì bảng đang ở chế độ lazy (tự slicing), nên ta lọc ở đây; dòng
// trống (chưa chọn hàng) luôn giữ lại để còn nhập tiếp.
const keyword = ref("");
const {
  first,
  rows: pageRows,
  totalRows,
  paged,
  showPager,
  onPage,
  gotoLast,
} = useClientPage(() => {
  const q = keyword.value.trim().toLowerCase();
  if (!q) return props.items;
  return props.items.filter((it) => {
    if (!it.product_code) return true;
    const name = it.line_name || catalog.productByCode(it.product_code)?.name || "";
    return `${name} ${it.product_code}`.toLowerCase().includes(q);
  });
}, 50);

// Cảnh báo thiếu tồn realtime (báo đỏ ngay trên dòng khi SL > tồn đúng kho):
// chỉ bật khi bảng có cột Kho xuất (kho trên dòng quyết định tồn so sánh).
// Không đồng bộ block; loadOnhand nạp 1 lần (có guard in catalog store).
if (props.showWarehouse) catalog.loadOnhand();

/** Sản phẩm trên dòng có cần tồn kho không (dịch vụ/nhân công → không). */
function needsStock(it: LineItem): boolean {
  if (!it.product_code) return false;
  const p = catalog.productByCode(it.product_code);
  return !!p && !p.is_service;
}

/**
 * Số lượng thiếu so với tồn theo đúng kho xuất trên dòng (kho rỗng → tổng mọi kho).
 * Trả về 0 khi: chưa chọn sản phẩm / là dịch vụ / chưa nạp tồn / đủ hàng.
 */
function shortageOf(it: LineItem): number {
  if (!it.quantity || !needsStock(it)) return 0;
  const avail = catalog.onhandAt(it.product_code, it.warehouse_code ?? "");
  return Math.max(0, it.quantity - avail);
}

/** Tồn còn lại tại kho trên dòng (cho tooltip "còn X"); 0 nếu chưa nạp. */
function onhandOf(it: LineItem): number {
  if (!it.product_code || !needsStock(it)) return 0;
  return catalog.onhandAt(it.product_code, it.warehouse_code ?? "");
}

// Thêm hàng hóa nhanh (chưa có trong danh mục) — mở ProductDialog chuẩn;
// lưu xong gắn sản phẩm mới vào dòng trống cuối (chế độ nhập liên tục) hoặc
// thêm dòng mới có sẵn sản phẩm đó.
const productDialog = ref(false);

function onProductSaved(code: string) {
  const last = props.items[props.items.length - 1];
  const row = last && !last.product_code ? last : null;
  if (row) {
    row.product_code = code;
    row.line_name = ""; // hàng mới → dùng tên chính
    onProductPick(row);
  } else {
    // Đủ các trường cho cả dòng nhập (discount) lẫn dòng xuất (ngành + kho).
    const created: LineItem = {
      product_code: code,
      quantity: 1,
      unit_price: 0,
      discount: 0,
      industry_code: "",
      warehouse_code: "",
    };
    props.items.push(created);
    onProductPick(created);
  }
}

// Tạo nhanh mặt hàng từ ô Sản phẩm (gõ tên CHƯA có trong danh mục → bấm dòng
// "Tạo nhanh"): mở form rút gọn, lưu xong gắn vào ĐÚNG dòng đang đứng. Giữ
// THAM CHIẾU dòng (không giữ vị trí) — bảng đã phân trang nên index theo trang
// không còn trỏ đúng dòng nữa.
const quickDialog = ref(false);
const quickName = ref("");
const quickRow = ref<LineItem | null>(null);

function onQuickCreate(row: LineItem, name: string) {
  quickRow.value = row;
  quickName.value = name;
  quickDialog.value = true;
}

function onQuickSaved(code: string) {
  const row = quickRow.value;
  quickRow.value = null;
  if (row) {
    // Dòng đang đứng đã có hàng khác → đổi sang hàng vừa tạo.
    row.product_code = code;
    row.line_name = ""; // hàng mới → dùng tên chính
    onProductPick(row);
    return;
  }
  onProductSaved(code);
}

// ─── Thêm "tên khác" ngay trên dòng (không phải ra màn Danh mục sản phẩm) ───
// Logic nằm trong ProductAliasDialog dùng chung; bảng chỉ giữ dòng nào đang mở
// và gắn tên vừa lưu vào dòng đó.
const aliasDialog = ref(false);
const aliasRow = ref<LineItem | null>(null);

/** Mã hàng của dòng đang thêm tên khác (rỗng nếu không có dòng nào đứng). */
const aliasProductCode = computed(() => aliasRow.value?.product_code ?? "");

function openAlias(row: LineItem) {
  aliasRow.value = row;
  aliasDialog.value = true;
}

/**
 * Lưu xong từ dialog: dòng giữ NGAY tên vừa thêm (F5 in đúng tên người dùng
 * chọn) và bảng tự báo kết quả — gộp 1 toast để khỏi nói một đằng khác một nẻo.
 */
function onAliasSaved(code: string, added: string[]) {
  const row = aliasRow.value;
  aliasRow.value = null;
  const attached = !!row && row.product_code === code;
  if (row && attached && added[0]) row.line_name = added[0];
  const detail = `${added.join(", ")} → ${catalog.productByCode(code)?.name ?? code}`;
  toast.add({
    severity: "success",
    summary: "Đã thêm tên khác",
    detail: `${detail} · ${attached ? "gắn vào dòng hóa đơn" : "lưu vào danh mục"}`,
  });
}

/**
 * Gán sản phẩm vào 1 dòng từ NGOÀI (popup khác — ví dụ tạo lô sản xuất xong —
 * muốn gắn hàng vào bảng): điền mã + tên chính rồi chạy lại bước tự điền
 * giá bán / nhóm ngành / kho xuất như đang chọn bằng ô dropdown.
 */
function assignProduct(index: number, code: string) {
  const row = props.items[index];
  if (!row) return;
  row.product_code = code;
  row.line_name = "";
  // `false`: vừa tạo lô xong không cần soi thiếu tồn lần nữa (vòng lặp gợi ý).
  onProductPick(row, false);
}

defineExpose({ assignProduct });

/**
 * Đổi mã hàng trên dòng → bỏ tên hiển thị cũ (tên của hàng trước không còn
 * nghĩa). Chạy TRƯỚC event `picked` để tên alias vừa chọn được ghi đè sau.
 */
function onProductChange(it: LineItem, code: string) {
  if (it.product_code === code) return;
  it.product_code = code;
  it.line_name = "";
}

/** Ghi tên người dùng chọn (tên chính → ""). */
function onProductPicked(it: LineItem, name: string) {
  it.line_name = name;
}

/**
 * Tự điền giá / nhóm ngành / kho xuất cho dòng vừa chọn hàng.
 *
 * `notify = false` khi gán từ NGOÀI (popup khác ví dụ tạo lô xong) — màn cha
 * chỉ muốn soi thiếu tồn khi NGƯỜI DÙNG tự chọn, không gợi ý lô lần nữa.
 */
function onProductPick(it: LineItem, notify = true) {
  const p = catalog.productByCode(it.product_code);
  if (!p) return;
  it.unit_price = props.priceField === "sale_price" ? p.sale_price : p.cost_price;
  // Tự điền nhóm ngành theo sản phẩm (cơ sở tỷ lệ thuế bán ra) — vẫn sửa tay được.
  if (p.industry_code) it.industry_code = p.industry_code;
  // Tự điền kho xuất theo kho mặc định của sản phẩm — nếu dòng chưa chọn kho.
  if (!it.warehouse_code && p.default_warehouse_id) {
    const w = props.warehouses.find((x) => x.id === p.default_warehouse_id);
    if (w) it.warehouse_code = w.code;
  }
  if (notify) emit("picked", it);
}

/** Giá trị thực tế 1 dòng (giá trị nhập kho): Thành tiền − Tiền CK. */
function lineNet(it: LineItem): number {
  const d = props.showDiscount ? (it.discount ?? 0) : 0;
  return it.quantity * it.unit_price - d;
}

/** Gõ thẳng Thành tiền cả dòng → quy về Đơn giá (SL × Đơn giá luôn khớp). */
function onAmountChange(it: LineItem, amount: number | null) {
  const qty = it.quantity || 1;
  it.unit_price = Math.round(((amount ?? 0) / qty) * 100) / 100;
}

/**
 * Xóa dòng: ô đứng đang theo TRANG (table chỉ render slice của trang) nên phải
 * dịch ngược về VỊ TRÍ thật trong `items` rồi mới báo lên màn gọi.
 */
function removeLine(it: LineItem) {
  const i = props.items.indexOf(it);
  if (i >= 0) emit("remove", i);
}

/**
 * Nút "Thêm dòng" khi bảng đã phân trang: thêm rồi nhảy trang cuối — dòng mới
 * thêm luôn ở cuối mảng nên đứng nguyên trang đầu sẽ không thấy nó.
 */
function addRowAndPage() {
  emit("add");
  nextTick(() => gotoLast());
}

const total = computed(() => props.items.reduce((s, it) => s + lineNet(it), 0));

// Chế độ nhập liên tục (pre-input): mở form có sẵn 1 dòng trống, điền xong
// dòng cuối (đã chọn sản phẩm) thì tự thêm dòng trống mới, không cần nút
// "Thêm dòng". Bật qua `compact` (phiếu nhập) hoặc `preInput` (phiếu xuất).
const tableRef = ref<{ $el: HTMLElement } | null>(null);

/** Cuộn bảng xuống đúng dòng mới được thêm (dòng trống cuối) để luôn thấy. */
function scrollToNewRow() {
  nextTick(() => {
    const root = tableRef.value?.$el;
    const last = root?.querySelector<HTMLElement>(".p-datatable-tbody tr:last-child");
    last?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  });
}

watch(
  () =>
    props.items.length
      ? `${props.items.length}|${props.items[props.items.length - 1]?.product_code ?? ""}`
      : "0",
  (now, prev) => {
    if (!(props.compact || props.preInput) || !props.canEdit || now === prev) return;
    const last = props.items[props.items.length - 1];
    if (!props.items.length || last?.product_code) {
      emit("add");
      scrollToNewRow();
    }
  },
  { immediate: true },
);
</script>

<template>
  <div class="mt-4 flex items-center justify-between">
    <h4 class="flex items-center gap-1.5 text-sm font-semibold">
      Mặt hàng
      <i
        v-if="info"
        class="pi pi-info-circle cursor-help text-xs text-gray-400"
        v-tooltip="info"
        aria-hidden="true"
      />
    </h4>
    <div class="flex items-center gap-2">
      <!-- Nút của màn gọi (popup hóa đơn: thêm tên khác / tạo lô nhanh). -->
      <slot name="actions" />
      <Button
        v-if="showAddProduct && canEdit"
        label="Thêm hàng hóa"
        icon="pi pi-plus"
        size="small"
        text
        @click="productDialog = true"
      />
      <Button
        v-if="canEdit && ((!compact && !preInput) || showPager)"
        label="Thêm dòng"
        icon="pi pi-plus"
        size="small"
        text
        @click="addRowAndPage"
      />
    </div>
  </div>

  <AppDataTable
    ref="tableRef"
    :value="paged"
    :first="first"
    :rows="pageRows"
    :total-records="totalRows"
    :paginator="showPager"
    :rows-per-page-options="[20, 50, 100, 200]"
    class="mt-2"
    :resizable-columns="false"
    :sortable="false"
    :size="compact || preInput ? 'small' : 'large'"
    :style="compact || preInput ? 'overflow-x: hidden' : undefined"
    :table-style="compact || preInput ? 'table-layout: fixed; width: 100%' : undefined"
    @page="onPage"
    @search="keyword = $event"
  >
    <Column
      header="Sản phẩm"
      :style="compact ? 'min-width: 230px' : preInput ? 'min-width: 260px' : undefined"
    >
      <template #body="{ data }">
        <div class="flex items-center gap-1">
          <ProductSelect
            class="min-w-0 flex-1"
            :model-value="data.product_code"
            :picked-name="data.line_name ?? ''"
            :warehouse-code="data.warehouse_code ?? ''"
            :size="compact || preInput ? 'small' : undefined"
            :allow-create="showAddProduct && canEdit"
            @update:model-value="onProductChange(data, $event)"
            @picked="onProductPicked(data, $event)"
            @change="onProductPick(data)"
            @create="onQuickCreate(data, $event)"
          />
          <!-- Thêm "tên khác" ngay trên dòng: gán tên cho CHÍNH mã hàng này. -->
          <Button
            v-if="canEdit && data.product_code"
            icon="pi pi-tag"
            severity="secondary"
            text
            rounded
            size="small"
            class="shrink-0"
            data-testid="line-add-alias"
            aria-label="Thêm tên khác"
            v-tooltip="'Thêm tên khác cho mặt hàng này'"
            @click="openAlias(data)"
          />
        </div>
      </template>
    </Column>
    <Column
      v-if="showUnit"
      header="ĐVT"
      :style="compact || preInput ? 'width: 88px' : 'width: 120px'"
    >
      <template #body="{ data }">
        <span class="text-gray-600">{{
          catalog.productByCode(data.product_code)?.unit || "—"
        }}</span>
      </template>
    </Column>
    <Column
      header="SL"
      :style="
        compact
          ? 'width: 72px'
          : preInput
            ? 'width: 80px'
            : showIndustry
              ? 'width: 100px'
              : 'width: 110px'
      "
    >
      <template #body="{ data }">
        <div class="flex w-full flex-col gap-0.5">
          <InputNumber
            v-model="data.quantity"
            :min="0"
            :size="compact || preInput ? 'small' : undefined"
            class="w-full"
            :class="{ '!ring-2 !ring-red-500 !border-red-500': shortageOf(data) > 0 }"
            :tooltip="
              showWarehouse && onhandOf(data) > 0 ? `Còn ${fmt(onhandOf(data))}` : undefined
            "
          />
          <span
            v-if="shortageOf(data) > 0"
            class="text-[11px] font-semibold leading-none text-red-600"
          >
            Thiếu {{ fmt(shortageOf(data)) }}
          </span>
        </div>
      </template>
    </Column>
    <Column
      header="Đơn giá"
      :style="
        compact
          ? 'width: 110px'
          : preInput
            ? 'width: 140px'
            : showIndustry
              ? 'width: 140px'
              : 'width: 150px'
      "
    >
      <template #body="{ data }">
        <InputNumber
          v-model="data.unit_price"
          :min="0"
          mode="currency"
          currency="VND"
          locale="vi-VN"
          :size="compact || preInput ? 'small' : undefined"
          class="w-full"
        />
      </template>
    </Column>
    <Column
      v-if="showIndustry"
      header="Nhóm ngành"
      :style="preInput ? 'width: 160px' : compact ? 'width: 140px' : undefined"
    >
      <template #body="{ data }">
        <Select
          v-model="data.industry_code"
          :options="industryGroups"
          optionLabel="name"
          optionValue="code"
          filter
          :size="compact || preInput ? 'small' : undefined"
          class="w-full"
        />
      </template>
    </Column>
    <Column
      v-if="showWarehouse"
      header="Kho xuất"
      :style="preInput ? 'width: 160px' : compact ? 'width: 150px' : undefined"
    >
      <template #body="{ data }">
        <Select
          v-model="data.warehouse_code"
          :options="warehouses"
          optionLabel="name"
          optionValue="code"
          filter
          showClear
          :size="compact || preInput ? 'small' : undefined"
          class="w-full"
          placeholder="Kho mặc định"
        />
      </template>
    </Column>
    <Column v-if="showDiscount" header="Tiền CK" align="right" style="width: 120px">
      <template #body="{ data }">
        <InputNumber
          v-model="data.discount"
          :min="0"
          mode="currency"
          currency="VND"
          locale="vi-VN"
          size="small"
          class="w-full"
        />
      </template>
    </Column>
    <Column
      v-if="showAmount"
      header="Thành tiền"
      align="right"
      :style="compact ? 'width: 130px' : undefined"
    >
      <template #body="{ data }">
        <InputNumber
          v-if="amountInput"
          :model-value="data.quantity * data.unit_price"
          @update:model-value="(v) => onAmountChange(data, v)"
          :min="0"
          mode="currency"
          currency="VND"
          locale="vi-VN"
          :size="compact || preInput ? 'small' : undefined"
          class="w-full"
        />
        <span v-else class="font-medium">{{ fmtVnd(data.quantity * data.unit_price) }}</span>
      </template>
    </Column>
    <Column header="" :style="compact || preInput ? 'width: 48px' : undefined">
      <template #body="{ data }">
        <Button
          v-if="canEdit"
          icon="pi pi-times"
          text
          rounded
          size="small"
          severity="danger"
          @click="removeLine(data)"
        />
      </template>
    </Column>
    <template #empty><EmptyState text="Chưa có dòng nào" icon="pi pi-list" /></template>
  </AppDataTable>

  <div class="mt-4 flex items-center justify-end gap-3">
    <span class="text-sm text-gray-500">{{ totalLabel }}</span>
    <span class="text-lg font-bold text-primary-600">{{ fmt(total) }} đ</span>
  </div>

  <!-- Thêm hàng hóa nhanh — dùng chung dialog chuẩn (tự sinh mã + nhóm ngành + thuế) -->
  <ProductDialog v-model:visible="productDialog" :show-action="canEdit" @saved="onProductSaved" />
  <!-- Tạo nhanh từ ô Sản phẩm: form rút gọn, tên đã gõ sẵn. -->
  <ProductDialog
    v-model:visible="quickDialog"
    quick
    :preset-name="quickName"
    :show-action="canEdit"
    @saved="onQuickSaved"
  />
  <!-- Thêm "tên khác" — dùng chung dialog (mở từ dòng hoặc từ header bảng). -->
  <ProductAliasDialog
    v-model:visible="aliasDialog"
    :product-code="aliasProductCode"
    @saved="onAliasSaved"
  />
</template>
