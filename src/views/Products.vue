<script setup lang="ts">
import { storeToRefs } from "pinia";
import { FilterMatchMode } from "@primevue/core/api";
import { api } from "@/db";
import { useAuthStore } from "@/stores/auth";
import { useCatalogStore } from "@/stores/catalog";
import { useSettingsStore } from "@/stores/settings";
import ProductDialog from "@/components/ProductDialog.vue";
import type { BomItem, Product } from "@/types";
import { fmtInt as fmt, fmtVnd } from "@/utils/format";
import { useKeepAliveRefresh } from "@/composables/useKeepAliveRefresh";

const catalog = useCatalogStore();
const auth = useAuthStore();
const settings = useSettingsStore();
const {
  pageProducts: products,
  totalProducts,
  productsLoading: loading,
  industryGroups,
} = storeToRefs(catalog);
const toast = useToast();
const confirm = useConfirm();

// ─── LAZY LOAD (server-side page + sort + filter — sản phẩm có thể lên tới vài nghìn) ───
const first = ref(0);
const rowsPerPage = ref(50);
const multiSortMeta = ref<{ field: string; order: 1 | -1 }[]>([]);
const lazyParams = ref<Record<string, unknown>>({});

interface GridFilter {
  value: any;
  matchMode: string;
}

const filters = ref<Record<string, GridFilter>>({
  global: { value: null, matchMode: FilterMatchMode.CONTAINS },
  code: { value: null, matchMode: FilterMatchMode.CONTAINS },
  name: { value: null, matchMode: FilterMatchMode.CONTAINS },
  unit: { value: null, matchMode: FilterMatchMode.CONTAINS },
  sale_price: { value: null, matchMode: FilterMatchMode.EQUALS },
  cost_price: { value: null, matchMode: FilterMatchMode.EQUALS },
  min_stock: { value: null, matchMode: FilterMatchMode.EQUALS },
  vat_rate: { value: null, matchMode: FilterMatchMode.EQUALS },
  import_tax_rate: { value: null, matchMode: FilterMatchMode.EQUALS },
  industry_code: { value: null, matchMode: FilterMatchMode.CONTAINS },
});

const hasActiveFilter = computed(() =>
  Object.values(filters.value).some((f) => f.value != null && f.value !== ""),
);

function loadLazyData() {
  lazyParams.value = {
    ...lazyParams.value,
    first: first.value,
    rows: rowsPerPage.value,
    filters: filters.value,
  };
  catalog.loadProductsPage(lazyParams.value);
}

function onPage(event: { first: number; rows: number }) {
  first.value = event.first;
  rowsPerPage.value = event.rows;
  loadLazyData();
}

function onSort(event: {
  sortField?: string | null;
  sortOrder?: 1 | -1 | null;
  multiSortMeta?: Array<{ field: string; order: 1 | -1 }> | null;
}) {
  multiSortMeta.value = event.multiSortMeta ?? [];
  lazyParams.value = {
    ...lazyParams.value,
    sortField: event.sortField ?? null,
    sortOrder: event.sortOrder ?? null,
    multiSortMeta: multiSortMeta.value,
  };
  loadLazyData();
}

function onFilter() {
  first.value = 0;
  loadLazyData();
}

// Ô tìm kiếm chung do AppDataTable dựng sẵn trong header → đồng bộ vào bộ lọc
// server-side (Products là lazy: DataTable không tự lọc client-side).
function onSearch(v: string) {
  filters.value.global.value = v || null;
  first.value = 0;
  onFilter();
}

function resetFilters() {
  Object.values(filters.value).forEach((f) => (f.value = null));
}

// ─── SELECTION (checkbox, giữ lựa chọn qua các trang bằng id) ───
const selectedIds = ref<Set<number>>(new Set());
const selection = computed<Product[]>({
  get: () => products.value.filter((p) => selectedIds.value.has(p.id)),
  set: (val: Product[]) => {
    selectedIds.value = new Set(val.map((p) => p.id));
  },
});
const selectedCount = computed(() => selectedIds.value.size);

// ─── XÓA HÀNG LOẠT (thay cho nút xóa từng dòng) ───
function removeSelected() {
  const ids = [...selectedIds.value];
  const n = ids.length;
  if (!n) return;
  confirm.require({
    message: `Xóa ${n} sản phẩm đã chọn?`,
    header: "Xác nhận xóa",
    icon: "pi pi-exclamation-triangle",
    acceptLabel: "Xóa",
    rejectLabel: "Hủy",
    accept: async () => {
      try {
        await catalog.deleteProducts(ids);
        selectedIds.value = new Set();
        toast.add({
          severity: "success",
          summary: "Đã xóa",
          detail: `${n} sản phẩm`,
        });
        // Đã chọn hết trang hiện tại → lùi về trang trước nếu còn.
        const allOnPageSelected =
          products.value.length > 0 && products.value.every((p) => ids.includes(p.id));
        if (allOnPageSelected && first.value > 0) {
          first.value = Math.max(0, first.value - rowsPerPage.value);
        }
        loadLazyData();
      } catch (e) {
        toast.add({
          severity: "error",
          summary: "Không xóa được",
          detail: String(e),
        });
      }
    },
  });
}

// ─── GÁN NHÓM NGÀNH HÀNG HÓA (vá dữ liệu cũ tạo trước khi có mặc định) ───
const assigningIndustry = ref(false);

/** Tên nhóm ngành mặc định của hàng hóa (PPHH — Phân phối, cung cấp hàng hóa). */
const goodsGroupName = computed(
  () => industryGroups.value.find((g) => g.code === "PPHH")?.name ?? "Phân phối, cung cấp hàng hóa",
);
const goodsGroupTooltip = computed(
  () => `Gán nhóm "${goodsGroupName.value}" cho hàng hóa chưa có nhóm ngành`,
);

function assignGoodsIndustry() {
  confirm.require({
    header: "Gán nhóm ngành hàng hóa",
    message: `Gán nhóm "${goodsGroupName.value}" cho mọi sản phẩm loại "Hàng hóa" đang chưa có nhóm ngành? Sản phẩm dịch vụ và sản phẩm đã có nhóm khác được giữ nguyên.`,
    icon: "pi pi-tags",
    acceptLabel: "Gán nhóm ngành",
    rejectLabel: "Hủy",
    accept: async () => {
      assigningIndustry.value = true;
      try {
        const r = await catalog.assignGoodsIndustry();
        toast.add({
          severity: r.updated > 0 ? "success" : "info",
          summary:
            r.updated > 0
              ? `Đã gán nhóm ngành cho ${r.updated} sản phẩm`
              : "Không có sản phẩm nào cần gán",
          detail:
            r.kept > 0
              ? `${r.kept} sản phẩm hàng hóa đã có nhóm ngành khác — giữ nguyên.`
              : undefined,
        });
        loadLazyData();
      } catch (e) {
        toast.add({ severity: "error", summary: "Không gán được nhóm ngành", detail: String(e) });
      } finally {
        assigningIndustry.value = false;
      }
    },
  });
}

// ─── TAB ĐỊNH MỨC VẬT TƯ (F4) ───
// Khai "1 thành phẩm cần bao nhiêu NVL" — khi lưu PNK loại "Tự sản xuất / gia
// công" và bật ô tick, app tự trừ tồn NVL + tự sinh phiếu xuất (Nợ 154 / Có 152).
//
// Khai được NHIỀU thành phẩm trong 1 màn: mỗi thành phẩm là 1 bảng riêng xếp
// chồng lên nhau, bấm "Thêm định mức" là có bảng mới — không phải chọn lại
// dropdown rồi nhớ xem bảng trước đã khai gì như kiểu 1 bảng duy nhất trước đây.
const tab = ref<"products" | "bom">("products");
const boms = ref<BomItem[]>([]);
/** 1 bảng định mức = 1 thành phẩm + toàn bộ dòng vật tư của bảng đó. */
interface BomBlock {
  /**
   * Khóa `v-for` gắn theo object bảng — không bao giờ đổi khi lưu/nạp lại.
   * Đổi khóa là DataTable bị thay instance, closure trong ô bảng giữ object cũ.
   */
  key: number;
  /** Mã thành phẩm lúc nạp từ DB — rỗng = bảng mới chưa từng lưu. */
  original_code: string;
  product_code: string;
  rows: { material_code: string; quantity: number }[];
}
const bomBlocks = ref<BomBlock[]>([]);
const bomSaving = ref(false);

let bomBlockSeq = 0;
/** Tạo bảng mới (khóa tăng dần theo object, khác nhau tuyệt đối). */
function makeBlock(originalCode: string, code: string, rows: BomBlock["rows"] = []): BomBlock {
  return { key: ++bomBlockSeq, original_code: originalCode, product_code: code, rows };
}

/** Tên thành phẩm để báo lỗi dễ đọc (fallback về mã khi không có trong danh mục). */
function fgName(code: string): string {
  return catalog.productByCode(code)?.name || code;
}

/**
 * Dựng bảng từ định mức đã nạp — gộp mọi dòng theo thành phẩm, sắp theo mã.
 *
 * Quan trọng: bảng đã có thì **giữ nguyên object**, chỉ thay dòng tại chỗ.
 * Slot của `Column` trong DataTable giữ closure tham chiếu `blk`; nếu thay object
 * mới thì bảng hiển thị vẫn đọc object cũ — người dùng sửa/xóa dòng là rơi vào
 * object mồ côi (hiển thị không đổi, lưu lại ghi dữ liệu cũ). E2E bắt được lỗi này.
 */
function groupBoms() {
  const rowsByCode = new Map<string, BomBlock["rows"]>();
  for (const b of boms.value) {
    const rows = rowsByCode.get(b.product_code) ?? [];
    rows.push({ material_code: b.material_code, quantity: b.quantity });
    rowsByCode.set(b.product_code, rows);
  }

  const pending: BomBlock[] = [];
  const reused: BomBlock[] = [];
  for (const blk of bomBlocks.value) {
    // Bảng thêm dở (chưa chọn thành phẩm) giữ nguyên object qua lần nạp lại.
    if (!blk.product_code) {
      pending.push(blk);
      continue;
    }
    const rows = rowsByCode.get(blk.product_code);
    // Định mức của mã này đã không còn (bị xóa ở nơi khác) → bỏ luôn bảng.
    if (!rows) continue;
    blk.rows.splice(0, blk.rows.length, ...rows);
    // Vừa ghi lại DB → không còn là bảng mới thêm dở nên không hiện nút "Bỏ".
    blk.original_code = blk.product_code;
    rowsByCode.delete(blk.product_code);
    reused.push(blk);
  }

  const created = [...rowsByCode].map(([code, rows]) => makeBlock(code, code, rows));
  bomBlocks.value = [...reused, ...created, ...pending].sort((a, b) => {
    // Đã khai xếp trước bảng mới thêm dở, rồi mới so mã với nhau.
    if (!a.product_code) return 1;
    if (!b.product_code) return -1;
    return a.product_code.localeCompare(b.product_code);
  });
}

/** Nạp định mức + danh mục SP đầy đủ (bộ chọn NVL cần mọi sản phẩm, không chỉ trang đang xem). */
async function loadBomData() {
  try {
    const [list] = await Promise.all([api.getProductBoms(), catalog.loadAll()]);
    boms.value = list;
    groupBoms();
  } catch (e) {
    toast.add({ severity: "error", summary: "Không tải được định mức", detail: String(e) });
  }
}

watch(tab, (t) => {
  if (t === "bom") void loadBomData();
});

/** Thêm 1 bảng trống ở cuối (chưa chọn thành phẩm — bấm "Bỏ" để hủy). */
function addBomBlock() {
  bomBlocks.value.push(makeBlock("", ""));
}

/**
 * Đổi thành phẩm của 1 bảng. Thành phẩm đã khai ở bảng khác → chặn: mỗi SP chỉ
 * có 1 định mức, muốn sửa thì sửa thẳng bảng đang giữ định mức đó.
 */
function setBlockFg(blk: BomBlock, code: string) {
  if (code && bomBlocks.value.some((b) => b !== blk && b.product_code === code)) {
    toast.add({
      severity: "warn",
      summary: "Thành phẩm đã có định mức",
      detail: `“${fgName(code)}” đang khai ở bảng khác — sửa trực tiếp bảng đó.`,
    });
    return;
  }
  blk.product_code = code;
}

/**
 * Nhập liên tục (pre-input) — không cần nút "Thêm dòng".
 *
 * Chọn thành phẩm là bảng tự có 1 dòng trống; điền xong dòng cuối (đã chọn vật
 * tư) là tự mở dòng kế để nhập tiếp; xóa hết dòng cũng tự mở lại dòng trống.
 * Cùng cơ chế với `compact`/`preInput` của LineItemsEditor ở phiếu nhập/xuất.
 */
watch(
  () =>
    bomBlocks.value
      .map(
        (b) =>
          `${b.key}:${b.product_code}:${b.rows.length}:${b.rows[b.rows.length - 1]?.material_code ?? ""}`,
      )
      .join("|"),
  () => {
    for (const blk of bomBlocks.value) {
      // Bảng chưa chọn thành phẩm thì chưa khai vật tư (tránh nhập trước rồi lưu bị chặn).
      if (!blk.product_code) continue;
      const last = blk.rows[blk.rows.length - 1];
      if (!last || last.material_code) blk.rows.push({ material_code: "", quantity: 1 });
    }
  },
  { immediate: true },
);

/**
 * Lưu MỌI bảng trong 1 lần bấm.
 *
 * - bảng có dòng vật tư → ghi (thay toàn bộ dòng cũ của thành phẩm đó);
 * - bảng đã từng lưu mà người dùng xóa hết dòng → XÓA định mức của SP đó;
 * - bảng mới thêm nhưng chưa khai gì → bỏ qua (chưa có gì để ghi);
 * - bảng đã lưu mà người dùng đổi thành phẩm → ghi cho mã mới + gỡ mã cũ.
 *
 * Lỗi ở bảng nào báo ngay tên thành phẩm đó rồi dừng (phần đã lưu vẫn giữ).
 */
async function saveAllBoms() {
  const blocks = bomBlocks.value;
  /** Dòng còn mã vật tư (dòng người dùng thêm dở trống được bỏ qua). */
  const rowsOf = (b: BomBlock) => b.rows.filter((r) => r.material_code.trim());

  const noFg = blocks.find((b) => !b.product_code && rowsOf(b).length > 0);
  if (noFg) {
    toast.add({
      severity: "warn",
      summary: "Thiếu thành phẩm",
      detail: "Còn bảng có vật tư mà chưa chọn thành phẩm — chọn thành phẩm hoặc bấm “Bỏ”.",
    });
    return;
  }

  const codes = blocks.map((b) => b.product_code).filter(Boolean);
  const dup = codes.find((c, i) => codes.indexOf(c) !== i);
  if (dup) {
    toast.add({
      severity: "warn",
      summary: "Thành phẩm bị lặp",
      detail: `“${fgName(dup)}” đang khai ở 2 bảng — mỗi thành phẩm chỉ có 1 định mức.`,
    });
    return;
  }

  const bad = blocks.find((b) => rowsOf(b).some((r) => !r.quantity || r.quantity <= 0));
  if (bad) {
    toast.add({
      severity: "warn",
      summary: "Số lượng chưa hợp lệ",
      detail: `Số lượng định mức của “${fgName(bad.product_code)}” phải lớn hơn 0`,
    });
    return;
  }

  const targets = blocks.filter((b) => b.product_code && (rowsOf(b).length > 0 || b.original_code));
  if (!targets.length) {
    toast.add({
      severity: "warn",
      summary: "Chưa có định mức nào để lưu",
      detail: "Chọn thành phẩm rồi nhập vật tư ở dòng trống.",
    });
    return;
  }

  bomSaving.value = true;
  let failed = false;
  const saved: string[] = [];
  const removed: string[] = [];
  try {
    for (const b of targets) {
      const rows = rowsOf(b).map((r) => ({
        material_code: r.material_code.trim(),
        quantity: r.quantity,
      }));
      try {
        // Ghi định mức của bảng này (dòng rỗng = xóa định mức của thành phẩm).
        await api.saveProductBom(b.product_code, rows);
        (rows.length ? saved : removed).push(b.product_code);
        // Đổi thành phẩm sau khi đã lưu lần đầu → gỡ định mức của mã cũ.
        if (b.original_code && b.original_code !== b.product_code) {
          await api.saveProductBom(b.original_code, []);
          if (rows.length) removed.push(b.original_code);
        }
      } catch (e) {
        failed = true;
        toast.add({
          severity: "error",
          summary: "Không lưu được định mức",
          detail: `${fgName(b.product_code)}: ${e}`,
        });
        break;
      }
    }
    boms.value = await api.getProductBoms();
    groupBoms();
    if (!failed) {
      const lineCount = targets.reduce((n, b) => n + rowsOf(b).length, 0);
      toast.add({
        severity: "success",
        summary: "Đã lưu định mức",
        detail:
          removed.length && !saved.length
            ? `Đã xóa định mức của ${removed.map(fgName).join(", ")}`
            : `${saved.length} thành phẩm — ${lineCount} dòng vật tư`,
      });
    }
  } finally {
    bomSaving.value = false;
  }
}

/** Tồn của vật tư (mọi kho) — để người dùng thấy trước khi lưu PNK sản xuất. */
function materialOnhand(code: string): number {
  if (!code) return 0;
  return catalog.onhandAt(code, "");
}

// ─── DIALOG THÊM / SỬA SẢN PHẨM (dùng chung ProductDialog) ───
const dialog = ref(false);
const editingProduct = ref<Product | null>(null);
const editing = ref(false);

async function openCreate() {
  editing.value = false;
  editingProduct.value = null;
  dialog.value = true;
}

// Sửa sản phẩm từ dòng đang chọn — chọn đúng 1 dòng rồi bấm Sửa trên header.
function openEdit() {
  if (selectedCount.value !== 1) {
    toast.add({
      severity: "warn",
      summary: "Chọn 1 dòng",
      detail: "Nhấn chọn đúng 1 sản phẩm muốn sửa",
    });
    return;
  }
  editing.value = true;
  editingProduct.value = selection.value[0];
  dialog.value = true;
}

// Sau khi lưu: sửa thì giữ nguyên filter/trang; sản phẩm mới thì bỏ filter
// để dòng vừa thêm nằm trong kết quả hiển thị.
function onProductSaved() {
  if (editing.value) {
    loadLazyData();
  } else {
    resetFilters();
    first.value = 0;
    loadLazyData();
  }
}

function reload() {
  // Giữ nguyên con trang + bộ lọc đang xem: chỉ nạp lại đúng trang dữ liệu đó.
  loadLazyData();
  catalog.loadIndustryGroups(); // nhóm ngành cho cột + dialog (màn này không loadAll)
}

void reload();

// Quay lại màn (KeepAlive giữ state) → nạp lại dữ liệu cho khỏi cũ.
useKeepAliveRefresh(reload);
</script>

<template>
  <div class="space-y-4">
    <Toolbar>
      <template #start>
        <div class="flex items-center gap-2">
          <i-mdi-package-variant class="text-xl text-primary-500" />
          <h3 class="font-semibold">Danh mục sản phẩm</h3>
        </div>
      </template>
      <template #end>
        <div class="flex items-center gap-2">
          <Button
            v-if="auth.canStock && tab === 'products'"
            label="Sửa"
            icon="pi pi-pencil"
            severity="secondary"
            outlined
            :disabled="selectedCount !== 1"
            @click="openEdit"
          />
          <Button
            v-if="auth.canStock && tab === 'products' && selectedCount > 0"
            severity="danger"
            outlined
            icon="pi pi-trash"
            :label="`Xóa đã chọn (${selectedCount})`"
            @click="removeSelected"
          />
          <Button
            v-if="auth.canStock && tab === 'products'"
            label="Gán nhóm ngành hàng hóa"
            icon="pi pi-tags"
            severity="secondary"
            outlined
            :loading="assigningIndustry"
            v-tooltip="goodsGroupTooltip"
            @click="assignGoodsIndustry"
          />
          <Button
            v-if="auth.canStock && tab === 'products'"
            label="Thêm sản phẩm"
            icon="pi pi-plus"
            @click="openCreate"
          />
        </div>
      </template>
    </Toolbar>

    <Card>
      <template #content>
        <Tabs v-model:value="tab">
          <TabList>
            <Tab value="products"><i class="pi pi-box mr-2" />Danh mục sản phẩm</Tab>
            <Tab value="bom"><i class="pi pi-list-check mr-2" />Định mức vật tư</Tab>
          </TabList>
          <TabPanels>
            <TabPanel value="products">
              <AppDataTable
                v-model:selection="selection"
                v-model:filters="filters"
                v-model:multiSortMeta="multiSortMeta"
                :meta-key-selection="false"
                :value="products"
                :loading="loading"
                :totalRecords="totalProducts"
                :first="first"
                :rows="rowsPerPage"
                :rows-per-page-options="[20, 50, 100, 200]"
                data-key="id"
                sort-mode="multiple"
                removable-sort
                :global-filter-fields="['code', 'name', 'unit', 'aliases']"
                filter-toggle
                @page="onPage"
                @sort="onSort"
                @filter="onFilter"
                @search="onSearch"
              >
                <template #header>
                  <span class="text-sm text-gray-500">
                    <template v-if="selectedCount > 0">
                      Đã chọn {{ selectedCount }} sản phẩm
                    </template>
                    <template v-else>Tổng {{ totalProducts }} sản phẩm</template>
                  </span>
                </template>

                <Column field="code" header="Mã SP" sortable />
                <Column field="name" header="Tên sản phẩm" sortable>
                  <template #filter="{ filterModel, filterCallback }">
                    <InputText
                      size="small"
                      v-model="filterModel.value"
                      placeholder="Tên"
                      @input="filterCallback()"
                    />
                  </template>
                </Column>
                <Column field="aliases" header="Tên khác">
                  <template #body="{ data }">
                    <span v-if="data.aliases" class="text-gray-600">{{ data.aliases }}</span>
                    <span v-else class="text-gray-400">—</span>
                  </template>
                </Column>
                <Column field="unit" header="ĐVT" sortable>
                  <template #filter="{ filterModel, filterCallback }">
                    <InputText
                      size="small"
                      v-model="filterModel.value"
                      placeholder="ĐVT"
                      @input="filterCallback()"
                    />
                  </template>
                </Column>
                <Column field="sale_price" header="Giá bán" sortable>
                  <template #body="{ data }">{{ fmtVnd(data.sale_price) }}</template>
                  <template #filter="{ filterModel, filterCallback }">
                    <InputNumber
                      v-model="filterModel.value"
                      :min="0"
                      mode="currency"
                      currency="VND"
                      locale="vi-VN"
                      placeholder="Bằng"
                      @input="filterCallback()"
                    />
                  </template>
                </Column>
                <Column field="cost_price" header="Giá vốn" sortable>
                  <template #body="{ data }">{{ fmtVnd(data.cost_price) }}</template>
                  <template #filter="{ filterModel, filterCallback }">
                    <InputNumber
                      v-model="filterModel.value"
                      :min="0"
                      mode="currency"
                      currency="VND"
                      locale="vi-VN"
                      placeholder="Bằng"
                      @input="filterCallback()"
                    />
                  </template>
                </Column>
                <Column field="min_stock" header="Tồn tối thiểu" sortable>
                  <template #body="{ data }">{{ fmt(data.min_stock) }}</template>
                  <template #filter="{ filterModel, filterCallback }">
                    <InputNumber
                      v-model="filterModel.value"
                      :min="0"
                      placeholder="Bằng"
                      @input="filterCallback()"
                    />
                  </template>
                </Column>
                <Column field="is_service" header="Loại" sortable>
                  <template #body="{ data }">
                    <Tag
                      :value="data.is_service ? 'Dịch vụ' : 'Hàng hóa'"
                      :severity="data.is_service ? 'warning' : 'info'"
                    />
                  </template>
                </Column>
                <Column field="industry_code" header="Nhóm ngành" sortable>
                  <template #body="{ data }">
                    <Tag
                      v-if="data.industry_code"
                      :value="
                        industryGroups.find((g) => g.code === data.industry_code)?.name ??
                        data.industry_code
                      "
                      severity="secondary"
                    />
                    <span v-else class="text-gray-400">—</span>
                  </template>
                  <template #filter="{ filterModel, filterCallback }">
                    <Select
                      v-model="filterModel.value"
                      :options="industryGroups"
                      option-label="name"
                      option-value="code"
                      placeholder="Tất cả"
                      show-clear
                      @change="filterCallback()"
                    />
                  </template>
                </Column>
                <Column field="vat_rate" header="Thuế GTGT đầu vào" sortable>
                  <template #body="{ data }">
                    <Tag :value="data.vat_rate * 100 + '%'" severity="info" />
                  </template>
                  <template #filter="{ filterModel, filterCallback }">
                    <Select
                      v-model="filterModel.value"
                      :options="settings.vatRateOptions"
                      placeholder="Tất cả"
                      show-clear
                      @change="filterCallback()"
                    />
                  </template>
                </Column>
                <Column field="import_tax_rate" header="Thuế nhập" sortable>
                  <template #body="{ data }">
                    <Tag :value="data.import_tax_rate * 100 + '%'" severity="secondary" />
                  </template>
                  <template #filter="{ filterModel, filterCallback }">
                    <Select
                      v-model="filterModel.value"
                      :options="settings.importTaxOptions"
                      placeholder="Tất cả"
                      show-clear
                      @change="filterCallback()"
                    />
                  </template>
                </Column>

                <template #empty>
                  <EmptyState
                    :text="
                      hasActiveFilter ? 'Không tìm thấy sản phẩm phù hợp.' : 'Chưa có sản phẩm.'
                    "
                    icon="pi pi-box"
                  />
                </template>
              </AppDataTable>
            </TabPanel>

            <!-- ĐỊNH MỨC VẬT TƯ (F4) -->
            <TabPanel value="bom">
              <p class="mb-3 text-xs text-gray-500">
                Khai <b>1 thành phẩm cần bao nhiêu vật tư</b> — mỗi thành phẩm 1 bảng, muốn khai
                thêm SP khác thì bấm <b>Thêm định mức</b>. Khi lưu phiếu nhập kho loại
                <b>Tự sản xuất / gia công</b> và bật ô "Tự xuất NVL theo định mức", app tự tính số
                NVL = định mức × sản lượng, tự trừ tồn FIFO và tự sinh phiếu xuất NVL (Nợ 154 / Có
                152) ngay trong lúc lưu phiếu nhập. Xóa hết dòng của một bảng rồi bấm
                <b>Lưu tất cả định mức</b> là xóa định mức của SP đó.
              </p>

              <div class="mb-3 flex flex-wrap justify-end gap-2">
                <Button
                  data-testid="bom-add"
                  label="Thêm định mức"
                  icon="pi pi-plus"
                  severity="secondary"
                  outlined
                  size="small"
                  :disabled="!auth.canStock"
                  @click="addBomBlock"
                />
                <Button
                  data-testid="bom-save-all"
                  label="Lưu tất cả định mức"
                  icon="pi pi-save"
                  size="small"
                  :loading="bomSaving"
                  :disabled="!auth.canStock || !bomBlocks.length"
                  @click="saveAllBoms"
                />
              </div>

              <EmptyState
                v-if="!bomBlocks.length"
                icon="pi pi-list-check"
                text="Chưa có định mức nào — bấm “Thêm định mức” rồi chọn thành phẩm để bắt đầu khai."
              />

              <div v-else class="space-y-4">
                <!-- 1 bảng = 1 thành phẩm -->
                <div
                  v-for="(blk, bi) in bomBlocks"
                  :key="blk.original_code || `new-${bi}`"
                  data-testid="bom-block"
                  class="rounded-md border border-gray-200 bg-white p-3"
                >
                  <div class="mb-2 flex flex-wrap items-end justify-between gap-3">
                    <div class="flex items-end gap-2">
                      <FormField label="Thành phẩm">
                        <div class="w-72" data-testid="bom-fg">
                          <ProductSelect
                            :model-value="blk.product_code"
                            size="small"
                            @update:model-value="setBlockFg(blk, $event)"
                          />
                        </div>
                      </FormField>
                      <!-- Bảng thêm dở (chưa lưu) thì hủy được ngay, không đụng dữ liệu server. -->
                      <Button
                        v-if="!blk.original_code"
                        data-testid="bom-drop"
                        label="Bỏ"
                        icon="pi pi-times"
                        text
                        severity="danger"
                        size="small"
                        @click="bomBlocks.splice(bi, 1)"
                      />
                    </div>
                    <!-- Không có nút "Thêm dòng": chọn thành phẩm là tự mở dòng
                         trống, điền xong dòng cuối lại tự mở dòng kế (pre-input). -->
                  </div>

                  <div data-testid="bom-table">
                    <DataTable
                      :value="blk.rows"
                      size="small"
                      :rows="50"
                      :row-hover="false"
                      class="p-datatable-sm"
                    >
                      <template #empty>
                        <EmptyState
                          icon="pi pi-list-check"
                          :text="
                            blk.product_code
                              ? 'Bảng này chưa có dòng vật tư nào — dòng trống sẽ tự mở lại khi cần.'
                              : 'Chọn thành phẩm cho bảng này — dòng vật tư trống sẽ tự mở ra.'
                          "
                        />
                      </template>
                      <Column header="Vật tư" style="min-width: 260px">
                        <template #body="{ index }">
                          <ProductSelect
                            :model-value="blk.rows[index].material_code"
                            size="small"
                            @update:model-value="blk.rows[index].material_code = $event"
                          />
                        </template>
                      </Column>
                      <Column header="ĐVT" style="width: 110px">
                        <template #body="{ index }">
                          <span class="text-gray-600">
                            {{ catalog.productByCode(blk.rows[index].material_code)?.unit || "—" }}
                          </span>
                        </template>
                      </Column>
                      <Column header="Tồn hiện tại" style="width: 130px">
                        <template #body="{ index }">
                          <span
                            :class="
                              materialOnhand(blk.rows[index].material_code) > 0
                                ? 'text-gray-600'
                                : 'text-gray-400'
                            "
                          >
                            {{ fmt(materialOnhand(blk.rows[index].material_code)) }}
                          </span>
                        </template>
                      </Column>
                      <Column header="SL / 1 thành phẩm" style="width: 170px">
                        <template #body="{ index }">
                          <InputNumber
                            v-model="blk.rows[index].quantity"
                            :min="0"
                            :use-grouping="false"
                            :max-fraction-digits="3"
                            size="small"
                            class="w-full"
                            :disabled="!auth.canStock"
                          />
                        </template>
                      </Column>
                      <Column v-if="auth.canStock" header="" style="width: 60px">
                        <template #body="{ index }">
                          <Button
                            icon="pi pi-trash"
                            severity="danger"
                            text
                            size="small"
                            aria-label="Xóa dòng vật tư"
                            @click="blk.rows.splice(index, 1)"
                          />
                        </template>
                      </Column>
                    </DataTable>
                  </div>
                </div>
              </div>
            </TabPanel>
          </TabPanels>
        </Tabs>
      </template>
    </Card>

    <!-- Dialog dùng chung: thêm / sửa sản phẩm -->
    <ProductDialog
      v-model:visible="dialog"
      :product="editingProduct"
      :show-action="auth.canStock"
      @saved="onProductSaved"
    />
  </div>
</template>
