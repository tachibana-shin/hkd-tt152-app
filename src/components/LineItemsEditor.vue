<script setup lang="ts">
// Bảng nhập dòng mặt hàng dùng chung cho phiếu nhập / phiếu xuất / hóa đơn.
import { useCatalogStore } from "@/stores/catalog";
import ProductDialog from "@/components/ProductDialog.vue";
import { fmtInt as fmt, fmtVnd } from "@/utils/format";
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
}>();

const catalog = useCatalogStore();
const toast = useToast();

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
function shortageAt(index: number): number {
  const it = props.items[index];
  if (!it.quantity || !needsStock(it)) return 0;
  const avail = catalog.onhandAt(it.product_code, it.warehouse_code ?? "");
  return Math.max(0, it.quantity - avail);
}

/** Tồn còn lại tại kho trên dòng (cho tooltip "còn X"); 0 nếu chưa nạp. */
function onhandLineAt(index: number): number {
  const it = props.items[index];
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
    onProductPick(props.items.length - 1);
  } else {
    // Đủ các trường cho cả dòng nhập (discount) lẫn dòng xuất (ngành + kho).
    props.items.push({
      product_code: code,
      quantity: 1,
      unit_price: 0,
      discount: 0,
      industry_code: "",
      warehouse_code: "",
    });
    onProductPick(props.items.length - 1);
  }
}

// Tạo nhanh mặt hàng từ ô Sản phẩm (gõ tên CHƯA có trong danh mục → bấm dòng
// "Tạo nhanh"): mở form rút gọn, lưu xong gắn vào ĐÚNG dòng đang đứng.
const quickDialog = ref(false);
const quickName = ref("");
const quickRow = ref<number | null>(null);

function onQuickCreate(index: number, name: string) {
  quickRow.value = index;
  quickName.value = name;
  quickDialog.value = true;
}

function onQuickSaved(code: string) {
  const index = quickRow.value;
  quickRow.value = null;
  if (index != null && props.items[index]) {
    const row = props.items[index];
    // Dòng đang đứng đã có hàng khác → đổi sang hàng vừa tạo.
    row.product_code = code;
    row.line_name = ""; // hàng mới → dùng tên chính
    onProductPick(index);
    return;
  }
  onProductSaved(code);
}

// ─── Thêm "tên khác" ngay trên dòng (không phải ra màn Danh mục sản phẩm) ───
const aliasDialog = ref(false);
const aliasRow = ref<number | null>(null);
const aliasText = ref("");
const aliasSaving = ref(false);

/** Sản phẩm của dòng đang thêm tên khác (hiện tên + mã ở dialog). */
const aliasProduct = computed<Product | undefined>(() => {
  const index = aliasRow.value;
  const code = index == null ? "" : (props.items[index]?.product_code ?? "");
  return catalog.productByCode(code);
});

const aliasHeader = computed(() =>
  aliasProduct.value ? `Thêm tên khác — ${aliasProduct.value.name}` : "Thêm tên khác",
);

function openAlias(index: number) {
  aliasRow.value = index;
  aliasText.value = "";
  aliasDialog.value = true;
}

async function saveAlias() {
  const index = aliasRow.value;
  const row = index == null ? null : props.items[index];
  const wanted = aliasText.value
    .split(",")
    .map((a) => a.trim())
    .filter(Boolean);
  if (!row || !row.product_code || !wanted.length) {
    toast.add({
      severity: "warn",
      summary: "Thiếu tên",
      detail: "Gõ tên khác muốn gán cho mặt hàng này",
    });
    return;
  }
  const code = row.product_code;
  const product = catalog.productByCode(code);
  aliasSaving.value = true;
  try {
    // Thêm (giữ tên cũ), nạp lại danh mục — backend bỏ tên trùng tên chính/mã.
    const added = await catalog.addProductAliases(code, wanted);
    // Dòng giữ NGAY tên vừa thêm — hóa đơn in đúng tên người dùng chọn.
    if (added[0]) row.line_name = added[0];
    toast.add({
      severity: added.length ? "success" : "warn",
      summary: added.length ? "Đã thêm tên khác" : "Tên này không cần thêm",
      detail: added.length
        ? `${added.join(", ")} → ${product?.name ?? code}`
        : `${wanted[0]} đã là tên chính hoặc tên khác của ${product?.name ?? code}`,
    });
    aliasDialog.value = false;
  } catch (e) {
    toast.add({ severity: "error", summary: "Lỗi", detail: String(e) });
  } finally {
    aliasSaving.value = false;
  }
}

/**
 * Đổi mã hàng trên dòng → bỏ tên hiển thị cũ (tên của hàng trước không còn
 * nghĩa). Chạy TRƯỚC event `picked` để tên alias vừa chọn được ghi đè sau.
 */
function onProductChange(index: number, code: string) {
  const it = props.items[index];
  if (it.product_code === code) return;
  it.product_code = code;
  it.line_name = "";
}

/** Ghi tên người dùng chọn (tên chính → ""). */
function onProductPicked(index: number, name: string) {
  props.items[index].line_name = name;
}

function onProductPick(index: number) {
  const p = catalog.productByCode(props.items[index].product_code);
  if (!p) return;
  props.items[index].unit_price = props.priceField === "sale_price" ? p.sale_price : p.cost_price;
  // Tự điền nhóm ngành theo sản phẩm (cơ sở tỷ lệ thuế bán ra) — vẫn sửa tay được.
  if (p.industry_code) props.items[index].industry_code = p.industry_code;
  // Tự điền kho xuất theo kho mặc định của sản phẩm — nếu dòng chưa chọn kho.
  if (!props.items[index].warehouse_code && p.default_warehouse_id) {
    const w = props.warehouses.find((x) => x.id === p.default_warehouse_id);
    if (w) props.items[index].warehouse_code = w.code;
  }
}

/** Giá trị thực tế 1 dòng (giá trị nhập kho): Thành tiền − Tiền CK. */
function lineNet(it: LineItem): number {
  const d = props.showDiscount ? (it.discount ?? 0) : 0;
  return it.quantity * it.unit_price - d;
}

/** Gõ thẳng Thành tiền cả dòng → quy về Đơn giá (SL × Đơn giá luôn khớp). */
function onAmountChange(index: number, amount: number | null) {
  const qty = props.items[index].quantity || 1;
  props.items[index].unit_price = Math.round(((amount ?? 0) / qty) * 100) / 100;
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
      <Button
        v-if="showAddProduct && canEdit"
        label="Thêm hàng hóa"
        icon="pi pi-plus"
        size="small"
        text
        @click="productDialog = true"
      />
      <Button
        v-if="canEdit && !compact && !preInput"
        label="Thêm dòng"
        icon="pi pi-plus"
        size="small"
        text
        @click="emit('add')"
      />
    </div>
  </div>

  <AppDataTable
    ref="tableRef"
    :value="items"
    class="mt-2"
    :resizable-columns="false"
    :sortable="false"
    :size="compact || preInput ? 'small' : 'large'"
    :style="compact || preInput ? 'overflow-x: hidden' : undefined"
    :table-style="compact || preInput ? 'table-layout: fixed; width: 100%' : undefined"
  >
    <Column
      header="Sản phẩm"
      :style="compact ? 'min-width: 230px' : preInput ? 'min-width: 260px' : undefined"
    >
      <template #body="{ index }">
        <div class="flex items-center gap-1">
          <ProductSelect
            class="min-w-0 flex-1"
            :model-value="items[index].product_code"
            :picked-name="items[index].line_name ?? ''"
            :warehouse-code="items[index].warehouse_code ?? ''"
            :size="compact || preInput ? 'small' : undefined"
            :allow-create="showAddProduct && canEdit"
            @update:model-value="onProductChange(index, $event)"
            @picked="onProductPicked(index, $event)"
            @change="onProductPick(index)"
            @create="onQuickCreate(index, $event)"
          />
          <!-- Thêm "tên khác" ngay trên dòng: gán tên cho CHÍNH mã hàng này. -->
          <Button
            v-if="canEdit && items[index].product_code"
            icon="pi pi-tag"
            severity="secondary"
            text
            rounded
            size="small"
            class="shrink-0"
            data-testid="line-add-alias"
            aria-label="Thêm tên khác"
            v-tooltip="'Thêm tên khác cho mặt hàng này'"
            @click="openAlias(index)"
          />
        </div>
      </template>
    </Column>
    <Column
      v-if="showUnit"
      header="ĐVT"
      :style="compact || preInput ? 'width: 88px' : 'width: 120px'"
    >
      <template #body="{ index }">
        <span class="text-gray-600">{{
          catalog.productByCode(items[index].product_code)?.unit || "—"
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
      <template #body="{ index }">
        <div class="flex w-full flex-col gap-0.5">
          <InputNumber
            v-model="items[index].quantity"
            :min="0"
            :size="compact || preInput ? 'small' : undefined"
            class="w-full"
            :class="{ '!ring-2 !ring-red-500 !border-red-500': shortageAt(index) > 0 }"
            :tooltip="
              showWarehouse && onhandLineAt(index) > 0
                ? `Còn ${fmt(onhandLineAt(index))}`
                : undefined
            "
          />
          <span
            v-if="shortageAt(index) > 0"
            class="text-[11px] font-semibold leading-none text-red-600"
          >
            Thiếu {{ fmt(shortageAt(index)) }}
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
      <template #body="{ index }">
        <InputNumber
          v-model="items[index].unit_price"
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
      <template #body="{ index }">
        <Select
          v-model="items[index].industry_code"
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
      <template #body="{ index }">
        <Select
          v-model="items[index].warehouse_code"
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
      <template #body="{ index }">
        <InputNumber
          v-model="items[index].discount"
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
      <template #body="{ index }">
        <InputNumber
          v-if="amountInput"
          :model-value="items[index].quantity * items[index].unit_price"
          @update:model-value="(v) => onAmountChange(index, v)"
          :min="0"
          mode="currency"
          currency="VND"
          locale="vi-VN"
          :size="compact || preInput ? 'small' : undefined"
          class="w-full"
        />
        <span v-else class="font-medium">{{
          fmtVnd(items[index].quantity * items[index].unit_price)
        }}</span>
      </template>
    </Column>
    <Column header="" :style="compact || preInput ? 'width: 48px' : undefined">
      <template #body="{ index }">
        <Button
          v-if="canEdit"
          icon="pi pi-times"
          text
          rounded
          size="small"
          severity="danger"
          @click="emit('remove', index)"
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
  <!-- Thêm "tên khác" ngay trên dòng hóa đơn. -->
  <AppDialog
    :visible="aliasDialog"
    :header="aliasHeader"
    width="max-w-md"
    action-label="Lưu"
    :saving="aliasSaving"
    @update:visible="aliasDialog = $event"
    @action="saveAlias"
  >
    <div class="flex flex-col gap-3 py-2">
      <p class="text-sm text-gray-600">
        Mặt hàng:
        <b>{{ aliasProduct?.name ?? "" }}</b>
        <span class="text-gray-400">({{ aliasProduct?.code ?? "" }})</span>
      </p>
      <FormField label="Tên khác" required>
        <InputText
          v-model="aliasText"
          size="small"
          placeholder="Tapioca, Bột mì"
          data-testid="alias-input"
          @keyup.enter="saveAlias"
        />
      </FormField>
      <p class="text-xs text-gray-400">
        Gõ 1 tên hoặc nhiều tên cách nhau bằng phẩy. Mỗi tên chỉ gán cho 1 sản phẩm; dòng hóa đơn
        giữ ngay tên vừa thêm.
      </p>
    </div>
  </AppDialog>
</template>
