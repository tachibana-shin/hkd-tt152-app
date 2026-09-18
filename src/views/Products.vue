<script setup lang="ts">
import { storeToRefs } from "pinia";
import { FilterMatchMode } from "@primevue/core/api";
import { useAuthStore } from "@/stores/auth";
import { useCatalogStore } from "@/stores/catalog";
import { useSettingsStore } from "@/stores/settings";
import type { Product } from "@/types";
import { fmtInt as fmt, fmtVnd } from "@/utils/format";

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

onMounted(() => {
  loadLazyData();
  catalog.loadIndustryGroups(); // nhóm ngành cho cột + dialog (màn này không loadAll)
});

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
          products.value.length > 0 &&
          products.value.every((p) => ids.includes(p.id));
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

// ─── DIALOG THÊM / SỬA SẢN PHẨM ───
const dialog = ref(false);
const saving = ref(false);
const editing = ref(false);
const dialogTitle = computed(() =>
  editing.value ? "Sửa sản phẩm" : "Thêm sản phẩm",
);
const form = reactive({
  id: null as number | null,
  code: "",
  name: "",
  unit: "Cái",
  sale_price: 0,
  cost_price: 0,
  min_stock: 0,
  vat_rate: 10,
  import_tax_rate: 0,
  is_service: false,
  industry_code: "PPHH",
});

// Thuế nhập khẩu dùng AutoComplete: chọn nhanh từ danh sách cấu hình hoặc gõ tùy ý.
const importTaxText = ref("0");
const importTaxSuggestions = ref<string[]>([]);

function onImportTaxComplete() {
  importTaxSuggestions.value = settings.importTaxOptions
    .map((n) => String(n))
    .filter((o) => o.includes(importTaxText.value.trim()));
}

function syncImportTaxText() {
  importTaxText.value = String(form.import_tax_rate);
}

function readImportTax() {
  const n = parseFloat(importTaxText.value.replace(",", "."));
  form.import_tax_rate = Number.isFinite(n) ? Math.max(0, Math.round(n)) : 0;
}

async function openCreate() {
  editing.value = false;
  try {
    const [code] = await Promise.all([
      settings.nextProductCode(),
      settings.load(),
    ]);
    Object.assign(form, {
      id: null,
      code,
      name: "",
      unit: settings.defaultUnit,
      sale_price: 0,
      cost_price: 0,
      min_stock: settings.defaultMinStock,
      vat_rate: settings.vatRateDefault,
      import_tax_rate: settings.importTaxDefault,
      is_service: false,
      industry_code: "PPHH",
    });
  } catch {
    Object.assign(form, {
      id: null,
      code: "",
      name: "",
      unit: "Cái",
      sale_price: 0,
      cost_price: 0,
      min_stock: 0,
      vat_rate: 10,
      import_tax_rate: 0,
      is_service: false,
      industry_code: "PPHH",
    });
  }
  syncImportTaxText();
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
  const row = selection.value[0];
  editing.value = true;
  Object.assign(form, {
    id: row.id,
    code: row.code,
    name: row.name,
    unit: row.unit,
    sale_price: row.sale_price,
    cost_price: row.cost_price,
    min_stock: row.min_stock,
    vat_rate: row.vat_rate * 100,
    import_tax_rate: row.import_tax_rate * 100,
    is_service: row.is_service ?? false,
    industry_code: row.industry_code ?? "PPHH",
  });
  syncImportTaxText();
  dialog.value = true;
}

async function save() {
  if (!form.code || !form.name) {
    toast.add({
      severity: "warn",
      summary: "Thiếu thông tin",
      detail: "Mã và tên sản phẩm là bắt buộc",
    });
    return;
  }
  saving.value = true;
  try {
    readImportTax();
    await catalog.saveProduct({
      id: form.id ?? undefined,
      code: form.code,
      name: form.name,
      unit: form.unit,
      sale_price: form.sale_price,
      cost_price: form.cost_price,
      min_stock: form.min_stock,
      vat_rate: form.vat_rate / 100,
      import_tax_rate: form.import_tax_rate / 100,
      is_service: form.is_service,
      industry_code: form.industry_code,
    });
    toast.add({
      severity: "success",
      summary: "Đã lưu",
      detail: `Sản phẩm ${form.code}`,
    });
    dialog.value = false;
    if (form.id != null) {
      // Sửa sản phẩm: giữ nguyên filter/trang đang xem.
      loadLazyData();
    } else {
      // Sản phẩm mới: bỏ filter để sản phẩm mới nằm trong kết quả hiển thị.
      resetFilters();
      first.value = 0;
      loadLazyData();
    }
  } catch (e) {
    toast.add({ severity: "error", summary: "Lỗi", detail: String(e) });
  } finally {
    saving.value = false;
  }
}
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
            v-if="auth.canStock"
            label="Sửa"
            icon="pi pi-pencil"
            severity="secondary"
            outlined
            :disabled="selectedCount !== 1"
            @click="openEdit"
          />
          <Button
            v-if="auth.canStock && selectedCount > 0"
            severity="danger"
            outlined
            icon="pi pi-trash"
            :label="`Xóa đã chọn (${selectedCount})`"
            @click="removeSelected"
          />
          <Button
            v-if="auth.canStock"
            label="Thêm sản phẩm"
            icon="pi pi-plus"
            @click="openCreate"
          />
        </div>
      </template>
    </Toolbar>

    <Card>
      <template #content>
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
          :global-filter-fields="['code', 'name', 'unit']"
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
                  industryGroups.find((g) => g.code === data.industry_code)
                    ?.name ?? data.industry_code
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
              <Tag
                :value="data.import_tax_rate * 100 + '%'"
                severity="secondary"
              />
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
                hasActiveFilter
                  ? 'Không tìm thấy sản phẩm phù hợp.'
                  : 'Chưa có sản phẩm.'
              "
              icon="pi pi-box"
            />
          </template>
        </AppDataTable>
      </template>
    </Card>

    <AppDialog
      v-model:visible="dialog"
      :header="dialogTitle"
      width="max-w-xl"
      action-label="Lưu"
      :saving="saving"
      :show-action="auth.canStock"
      @action="save"
    >
      <div class="grid grid-cols-2 gap-4 py-2">
        <FormField label="Mã sản phẩm" required>
          <InputText
            size="small"
            v-model="form.code"
            placeholder="SP001"
            :disabled="editing"
          />
        </FormField>
        <FormField label="Đơn vị tính">
          <InputText size="small" v-model="form.unit" placeholder="Cái" />
        </FormField>
        <FormField label="Tên sản phẩm" required class="col-span-2">
          <InputText
            size="small"
            v-model="form.name"
            placeholder="Tên hàng hóa / dịch vụ"
          />
        </FormField>
        <FormField label="Loại">
          <Select
            v-model="form.is_service"
            :options="[
              { label: 'Hàng hóa (theo dõi tồn kho)', value: false },
              {
                label: 'Dịch vụ (không nhập/xuất kho — vd nhân công)',
                value: true,
              },
            ]"
            option-label="label"
            option-value="value"
            size="small"
            class="w-full"
          />
          <p class="text-xs text-gray-400 mt-1">
            Thuế suất GTGT là thuế trên hóa đơn <b>mua vào</b> (đầu vào). HKD
            không xuất VAT khi bán ra — thuế bán ra tính theo nhóm ngành khi ghi
            phiếu xuất / hóa đơn.
          </p>
        </FormField>
        <FormField label="Nhóm ngành (cơ sở tính thuế bán ra)">
          <Select
            v-model="form.industry_code"
            :options="industryGroups"
            option-label="name"
            option-value="code"
            show-clear
            size="small"
            class="w-full"
          />
          <p class="text-xs text-gray-400 mt-1">
            Tự điền vào dòng phiếu xuất / hóa đơn.
          </p>
        </FormField>
        <FormField label="Giá bán">
          <InputNumber
            v-model="form.sale_price"
            :min="0"
            mode="currency"
            currency="VND"
            locale="vi-VN"
            size="small"
            class="w-full"
          />
        </FormField>
        <FormField label="Giá vốn">
          <InputNumber
            v-model="form.cost_price"
            :min="0"
            mode="currency"
            currency="VND"
            locale="vi-VN"
            size="small"
            class="w-full"
          />
        </FormField>
        <FormField label="Tồn tối thiểu">
          <InputNumber v-model="form.min_stock" :min="0" size="small" class="w-full" />
        </FormField>
        <FormField label="Thuế suất GTGT đầu vào (%)">
          <Select
            v-model="form.vat_rate"
            :options="settings.vatRateOptions"
            size="small"
            class="w-full"
          />
        </FormField>
        <FormField label="Thuế nhập khẩu (%)">
          <AutoComplete
            v-model="importTaxText"
            :suggestions="importTaxSuggestions"
            @complete="onImportTaxComplete"
            placeholder="Gõ hoặc chọn tỷ lệ"
            size="small"
            class="w-full"
          />
        </FormField>
        </div>
    </AppDialog>
  </div>
</template>
