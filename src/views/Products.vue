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
const tab = ref<"products" | "bom">("products");
const boms = ref<BomItem[]>([]);
/** Thành phẩm đang khai định mức (mã SP). */
const bomProduct = ref("");
/** Dòng đang sửa — bản sao từ `boms` để người dùng thêm / bớt / đổi số lượng. */
const bomRows = ref<{ material_code: string; quantity: number }[]>([]);
const bomSaving = ref(false);

/** Dựng lại dòng đang sửa từ dữ liệu đã nạp theo thành phẩm đang chọn. */
function syncBomRows() {
  bomRows.value = boms.value
    .filter((b) => b.product_code === bomProduct.value)
    .map((b) => ({ material_code: b.material_code, quantity: b.quantity }));
}

watch(bomProduct, syncBomRows);

/** Nạp định mức + danh mục SP đầy đủ (bộ chọn NVL cần mọi sản phẩm, không chỉ trang đang xem). */
async function loadBomData() {
  try {
    const [list] = await Promise.all([api.getProductBoms(), catalog.loadAll()]);
    boms.value = list;
    syncBomRows();
  } catch (e) {
    toast.add({ severity: "error", summary: "Không tải được định mức", detail: String(e) });
  }
}

watch(tab, (t) => {
  if (t === "bom") void loadBomData();
});

async function saveBom() {
  if (!bomProduct.value) return;
  const rows = bomRows.value.filter((r) => r.material_code.trim());
  if (rows.some((r) => !r.quantity || r.quantity <= 0)) {
    toast.add({
      severity: "warn",
      summary: "Số lượng chưa hợp lệ",
      detail: "Số lượng định mức của từng vật tư phải lớn hơn 0",
    });
    return;
  }
  bomSaving.value = true;
  try {
    await api.saveProductBom(
      bomProduct.value,
      rows.map((r) => ({ material_code: r.material_code.trim(), quantity: r.quantity })),
    );
    boms.value = await api.getProductBoms();
    syncBomRows();
    toast.add({
      severity: "success",
      summary: "Đã lưu định mức",
      detail: rows.length
        ? `${rows.length} dòng vật tư cho ${bomProduct.value}`
        : `Đã xóa định mức của ${bomProduct.value}`,
    });
  } catch (e) {
    toast.add({ severity: "error", summary: "Không lưu được định mức", detail: String(e) });
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
              <div class="mb-3 flex flex-wrap items-end justify-between gap-3">
                <div class="flex items-end gap-2">
                  <FormField label="Thành phẩm">
                    <div class="w-72" data-testid="bom-fg">
                      <ProductSelect v-model="bomProduct" size="small" />
                    </div>
                  </FormField>
                </div>
                <div class="flex items-center gap-2">
                  <Button
                    v-if="auth.canStock"
                    label="Thêm dòng"
                    icon="pi pi-plus"
                    severity="secondary"
                    outlined
                    size="small"
                    :disabled="!bomProduct"
                    @click="bomRows.push({ material_code: '', quantity: 1 })"
                  />
                  <Button
                    v-if="auth.canStock"
                    label="Lưu định mức"
                    icon="pi pi-save"
                    size="small"
                    :loading="bomSaving"
                    :disabled="!bomProduct"
                    @click="saveBom"
                  />
                </div>
              </div>

              <p class="mb-3 text-xs text-gray-500">
                Khai <b>1 thành phẩm cần bao nhiêu vật tư</b>. Khi lưu phiếu nhập kho loại
                <b>Tự sản xuất / gia công</b> và bật ô "Tự xuất NVL theo định mức", app tự tính số
                NVL = định mức × sản lượng, tự trừ tồn FIFO và tự sinh phiếu xuất NVL (Nợ 154 / Có
                152) ngay trong lúc lưu phiếu nhập.
              </p>

              <EmptyState
                v-if="!bomProduct"
                icon="pi pi-list-check"
                text="Chọn một thành phẩm để xem hoặc khai định mức vật tư."
              />
              <div v-else data-testid="bom-table">
                <DataTable
                  :value="bomRows"
                  size="small"
                  :rows="50"
                  :row-hover="false"
                  class="p-datatable-sm"
                >
                  <template #empty>
                    <EmptyState
                      icon="pi pi-list-check"
                      text="Chưa khai định mức cho sản phẩm này — bấm “Thêm dòng” để bắt đầu."
                    />
                  </template>
                  <Column header="Vật tư" style="min-width: 260px">
                    <template #body="{ index }">
                      <ProductSelect
                        :model-value="bomRows[index].material_code"
                        size="small"
                        @update:model-value="bomRows[index].material_code = $event"
                      />
                    </template>
                  </Column>
                  <Column header="ĐVT" style="width: 110px">
                    <template #body="{ index }">
                      <span class="text-gray-600">
                        {{ catalog.productByCode(bomRows[index].material_code)?.unit || "—" }}
                      </span>
                    </template>
                  </Column>
                  <Column header="Tồn hiện tại" style="width: 130px">
                    <template #body="{ index }">
                      <span
                        :class="
                          materialOnhand(bomRows[index].material_code) > 0
                            ? 'text-gray-600'
                            : 'text-gray-400'
                        "
                      >
                        {{ fmt(materialOnhand(bomRows[index].material_code)) }}
                      </span>
                    </template>
                  </Column>
                  <Column header="SL / 1 thành phẩm" style="width: 170px">
                    <template #body="{ index }">
                      <InputNumber
                        v-model="bomRows[index].quantity"
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
                        @click="bomRows.splice(index, 1)"
                      />
                    </template>
                  </Column>
                </DataTable>
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
