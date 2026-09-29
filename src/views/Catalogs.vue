<script setup lang="ts">
import { storeToRefs } from "pinia";
import { FilterMatchMode } from "@primevue/core/api";
import { api, type PageResult } from "@/db";
import { useAuthStore } from "@/stores/auth";
import { useCatalogStore } from "@/stores/catalog";
import PartnerDialog from "@/components/PartnerDialog.vue";
import type { Warehouse, Customer, Supplier, IndustryGroup } from "@/types";
import { useKeepAliveRefresh } from "@/composables/useKeepAliveRefresh";
import { useLazyPage } from "@/composables/useLazyPage";

const catalog = useCatalogStore();
const auth = useAuthStore();
const { industryGroups } = storeToRefs(catalog);
const toast = useToast();
const confirm = useConfirm();

// Hàng filter theo cột — mặc định ẩn, bật bằng nút "Bộ lọc" của AppDataTable.
const filters = ref<Record<string, { value: any; matchMode: string }>>({
  code: { value: null, matchMode: FilterMatchMode.CONTAINS },
  name: { value: null, matchMode: FilterMatchMode.CONTAINS },
  tax_code: { value: null, matchMode: FilterMatchMode.CONTAINS },
  address: { value: null, matchMode: FilterMatchMode.CONTAINS },
  phone: { value: null, matchMode: FilterMatchMode.CONTAINS },
});

/**
 * Danh sách đối tác (kho / khách / NCC) dài theo thời gian nên mỗi bảng có phân
 * trang server-side riêng: bảng lazy gửi trang + lọc cột + từ khoá, server trả
 * đúng trang đó kèm tổng số dòng.
 */
function lazyList<T>(
  label: string,
  loader: (lazy: Record<string, unknown>) => Promise<PageResult<T>>,
) {
  const rows = ref<T[]>([]);
  const total = ref(0);
  const busy = ref(false);
  const page = useLazyPage(async (lazy) => {
    busy.value = true;
    try {
      const res = await loader(lazy);
      rows.value = res.rows;
      total.value = res.total;
    } catch (e) {
      toast.add({ severity: "error", summary: `Lỗi tải ${label}`, detail: String(e) });
    } finally {
      busy.value = false;
    }
  });
  return { rows, total, busy, page };
}

const warehousesList = lazyList("kho hàng", (lazy) => api.getWarehousesPage(lazy));
const customersList = lazyList("khách hàng", (lazy) => api.getCustomersPage(lazy));
const suppliersList = lazyList("nhà cung cấp", (lazy) => api.getSuppliersPage(lazy));

type Kind = "warehouse" | "customer" | "supplier";

const tab = ref("warehouses");
const dialog = ref(false);
const dialogKind = ref<Kind>("warehouse");
const editing = ref(false);
const saving = ref(false);
// Đối tác đang sửa (thêm mới thì null) — PartnerDialog tự sinh mã khi thêm mới.
const editingRow = ref<{
  code: string;
  name: string;
  tax_code?: string;
  address?: string;
  phone?: string;
} | null>(null);

// ─── Nhóm ngành (Thêm / Sửa qua dialog; Xóa dòng đang chọn) ───
const igDialog = ref(false);
const igEditing = ref(false);
const igForm = reactive({ code: "", name: "", vat_rate: 0, pit_rate: 0 });
const igSelection = ref<IndustryGroup[]>([]);
// Selection theo tab (kho / khách / NCC) — chọn dòng rồi bấm Sửa ở header.
const whSelection = ref<Warehouse[]>([]);
const cuSelection = ref<Customer[]>([]);
const suSelection = ref<Supplier[]>([]);
const igDialogTitle = computed(() => (igEditing.value ? "Sửa nhóm ngành" : "Thêm nhóm ngành"));

function openIgCreate() {
  igEditing.value = false;
  Object.assign(igForm, { code: "", name: "", vat_rate: 0, pit_rate: 0 });
  igDialog.value = true;
}

function openIgEdit() {
  if (igSelection.value.length !== 1) {
    toast.add({
      severity: "warn",
      summary: "Chọn 1 dòng",
      detail: "Nhấn chọn đúng 1 nhóm ngành muốn sửa",
    });
    return;
  }
  const row = igSelection.value[0];
  igEditing.value = true;
  Object.assign(igForm, {
    code: row.code,
    name: row.name,
    vat_rate: Math.round(row.vat_rate * 10000) / 100,
    pit_rate: Math.round(row.pit_rate * 10000) / 100,
  });
  igDialog.value = true;
}

async function saveIg() {
  if (!igForm.code.trim() || !igForm.name.trim()) {
    toast.add({
      severity: "warn",
      summary: "Thiếu thông tin",
      detail: "Mã và tên nhóm ngành là bắt buộc",
    });
    return;
  }
  saving.value = true;
  try {
    await catalog.saveIndustryGroup({
      code: igForm.code.trim(),
      name: igForm.name.trim(),
      vat_rate: igForm.vat_rate / 100,
      pit_rate: igForm.pit_rate / 100,
    });
    toast.add({
      severity: "success",
      summary: "Đã lưu",
      detail: `Nhóm ngành ${igForm.code.trim()}`,
    });
    igDialog.value = false;
  } catch (e) {
    toast.add({ severity: "error", summary: "Không lưu được", detail: String(e) });
  } finally {
    saving.value = false;
  }
}

function deleteSelectedIg() {
  const rows = [...igSelection.value];
  if (!rows.length) return;
  const label = rows.length === 1 ? `nhóm ngành ${rows[0].code}` : `${rows.length} nhóm ngành`;
  confirm.require({
    message: `Xóa ${label} đã chọn?`,
    header: "Xác nhận xóa",
    icon: "pi pi-exclamation-triangle",
    acceptLabel: "Xóa",
    rejectLabel: "Hủy",
    accept: async () => {
      try {
        for (const g of rows) {
          await catalog.deleteIndustryGroup(g.code);
        }
        igSelection.value = [];
        toast.add({
          severity: "success",
          summary: "Đã xóa",
          detail: label,
        });
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

function openCreate(kind: Kind) {
  dialogKind.value = kind;
  editing.value = false;
  editingRow.value = null;
  dialog.value = true;
}

function openEdit(kind: Kind, row: Warehouse | Customer | Supplier) {
  dialogKind.value = kind;
  editing.value = true;
  editingRow.value = {
    code: row.code,
    name: row.name,
    tax_code: kind !== "warehouse" ? ((row as Customer).tax_code ?? "") : "",
    address: kind !== "warehouse" ? ((row as Customer).address ?? "") : "",
    phone: kind !== "warehouse" ? ((row as Customer).phone ?? "") : "",
  };
  dialog.value = true;
}

async function reload() {
  try {
    // Nhóm ngành là danh mục cố định theo hệ thống thuế (vài trăm dòng, không
    // phình theo thời gian) nên vẫn nạp một lần; 3 danh sách đối tác phân trang.
    await Promise.all([
      catalog.loadIndustryGroups(),
      warehousesList.page.reload(),
      customersList.page.reload(),
      suppliersList.page.reload(),
    ]);
  } catch (e) {
    toast.add({ severity: "error", summary: "Lỗi tải lại", detail: String(e) });
  }
}

// Sửa qua dialog: chọn 1 dòng (tap) rồi bấm Sửa trên header tab.

void reload();

// Quay lại màn (KeepAlive giữ state) → nạp lại dữ liệu cho khỏi cũ.
useKeepAliveRefresh(reload);
</script>

<template>
  <div class="space-y-4">
    <Toolbar>
      <template #start>
        <div class="flex items-center gap-2">
          <i-mdi-warehouse class="text-xl text-primary-500" />
          <h3 class="font-semibold">Danh mục đối tác &amp; kho</h3>
        </div>
      </template>
      <template #end>
        <Button label="Tải lại" icon="pi pi-refresh" severity="secondary" @click="reload" />
      </template>
    </Toolbar>

    <Card>
      <template #content>
        <Tabs v-model:value="tab">
          <TabList>
            <Tab value="warehouses"><i class="pi pi-warehouse mr-2" />Kho hàng</Tab>
            <Tab value="customers"><i class="pi pi-users mr-2" />Khách hàng</Tab>
            <Tab value="suppliers"><i class="pi pi-truck mr-2" />Nhà cung cấp</Tab>
            <Tab value="industryGroups"
              ><i class="pi pi-sitemap mr-2" />Nhóm ngành
              <i
                class="pi pi-info-circle text-xs text-gray-400 cursor-help"
                v-tooltip="
                  'Tỷ lệ GTGT/TNCN áp dụng cho doanh thu bán ra của nhóm ngành (TT 152/2025).'
                "
            /></Tab>
          </TabList>
          <TabPanels>
            <!-- Kho hàng -->
            <TabPanel value="warehouses">
              <div class="mb-3 flex items-center justify-end gap-2">
                <Button
                  v-if="auth.canStock"
                  label="Sửa"
                  icon="pi pi-pencil"
                  severity="secondary"
                  outlined
                  :disabled="whSelection.length !== 1"
                  @click="openEdit('warehouse', whSelection[0])"
                />
                <Button
                  v-if="auth.canStock"
                  label="Thêm kho"
                  icon="pi pi-plus"
                  @click="openCreate('warehouse')"
                />
              </div>
              <AppDataTable
                v-model:filters="filters"
                v-model:selection="whSelection"
                :value="warehousesList.rows.value"
                :loading="warehousesList.busy.value"
                :totalRecords="warehousesList.total.value"
                :first="warehousesList.page.first.value"
                :rows="warehousesList.page.rowsPerPage.value"
                :rows-per-page-options="warehousesList.page.pageSizes"
                stripedRows
                data-key="id"
                sort-mode="single"
                removable-sort
                filter-toggle
                @page="warehousesList.page.onPage"
                @sort="warehousesList.page.onSort"
                @search="warehousesList.page.onSearch"
                @filter="warehousesList.page.setColumnFilters(filters)"
              >
                <Column field="code" header="Mã kho">
                  <template #filter="{ filterModel, filterCallback }">
                    <InputText
                      size="small"
                      v-model="filterModel.value"
                      placeholder="Mã"
                      @input="filterCallback()"
                    />
                  </template>
                </Column>
                <Column field="name" header="Tên kho">
                  <template #filter="{ filterModel, filterCallback }">
                    <InputText
                      size="small"
                      v-model="filterModel.value"
                      placeholder="Tên"
                      @input="filterCallback()"
                    />
                  </template>
                </Column>
                <template #empty
                  ><EmptyState text="Chưa có kho hàng." icon="pi pi-warehouse"
                /></template>
              </AppDataTable>
            </TabPanel>

            <!-- Khách hàng -->
            <TabPanel value="customers">
              <div class="mb-3 flex items-center justify-end gap-2">
                <Button
                  v-if="auth.canStock"
                  label="Sửa"
                  icon="pi pi-pencil"
                  severity="secondary"
                  outlined
                  :disabled="cuSelection.length !== 1"
                  @click="openEdit('customer', cuSelection[0])"
                />
                <Button
                  v-if="auth.canStock"
                  label="Thêm khách hàng"
                  icon="pi pi-plus"
                  @click="openCreate('customer')"
                />
              </div>
              <AppDataTable
                v-model:filters="filters"
                v-model:selection="cuSelection"
                :value="customersList.rows.value"
                :loading="customersList.busy.value"
                :totalRecords="customersList.total.value"
                :first="customersList.page.first.value"
                :rows="customersList.page.rowsPerPage.value"
                :rows-per-page-options="customersList.page.pageSizes"
                stripedRows
                data-key="id"
                sort-mode="single"
                removable-sort
                filter-toggle
                @page="customersList.page.onPage"
                @sort="customersList.page.onSort"
                @search="customersList.page.onSearch"
                @filter="customersList.page.setColumnFilters(filters)"
              >
                <Column field="code" header="Mã">
                  <template #filter="{ filterModel, filterCallback }">
                    <InputText
                      size="small"
                      v-model="filterModel.value"
                      placeholder="Mã"
                      @input="filterCallback()"
                    />
                  </template>
                </Column>
                <Column field="name" header="Tên">
                  <template #filter="{ filterModel, filterCallback }">
                    <InputText
                      size="small"
                      v-model="filterModel.value"
                      placeholder="Tên"
                      @input="filterCallback()"
                    />
                  </template>
                </Column>
                <Column field="tax_code" header="MST">
                  <template #filter="{ filterModel, filterCallback }">
                    <InputText
                      size="small"
                      v-model="filterModel.value"
                      placeholder="MST"
                      @input="filterCallback()"
                    />
                  </template>
                </Column>
                <Column field="address" header="Địa chỉ">
                  <template #filter="{ filterModel, filterCallback }">
                    <InputText
                      size="small"
                      v-model="filterModel.value"
                      placeholder="Địa chỉ"
                      @input="filterCallback()"
                    />
                  </template>
                </Column>
                <Column field="phone" header="Điện thoại">
                  <template #filter="{ filterModel, filterCallback }">
                    <InputText
                      size="small"
                      v-model="filterModel.value"
                      placeholder="ĐT"
                      @input="filterCallback()"
                    />
                  </template>
                </Column>
                <template #empty
                  ><EmptyState text="Chưa có khách hàng." icon="pi pi-users"
                /></template>
              </AppDataTable>
            </TabPanel>

            <!-- Nhà cung cấp -->
            <TabPanel value="suppliers">
              <div class="mb-3 flex items-center justify-end gap-2">
                <Button
                  v-if="auth.canStock"
                  label="Sửa"
                  icon="pi pi-pencil"
                  severity="secondary"
                  outlined
                  :disabled="suSelection.length !== 1"
                  @click="openEdit('supplier', suSelection[0])"
                />
                <Button
                  v-if="auth.canStock"
                  label="Thêm nhà cung cấp"
                  icon="pi pi-plus"
                  @click="openCreate('supplier')"
                />
              </div>
              <AppDataTable
                v-model:filters="filters"
                v-model:selection="suSelection"
                :value="suppliersList.rows.value"
                :loading="suppliersList.busy.value"
                :totalRecords="suppliersList.total.value"
                :first="suppliersList.page.first.value"
                :rows="suppliersList.page.rowsPerPage.value"
                :rows-per-page-options="suppliersList.page.pageSizes"
                stripedRows
                data-key="id"
                sort-mode="single"
                removable-sort
                filter-toggle
                @page="suppliersList.page.onPage"
                @sort="suppliersList.page.onSort"
                @search="suppliersList.page.onSearch"
                @filter="suppliersList.page.setColumnFilters(filters)"
              >
                <Column field="code" header="Mã">
                  <template #filter="{ filterModel, filterCallback }">
                    <InputText
                      size="small"
                      v-model="filterModel.value"
                      placeholder="Mã"
                      @input="filterCallback()"
                    />
                  </template>
                </Column>
                <Column field="name" header="Tên">
                  <template #filter="{ filterModel, filterCallback }">
                    <InputText
                      size="small"
                      v-model="filterModel.value"
                      placeholder="Tên"
                      @input="filterCallback()"
                    />
                  </template>
                </Column>
                <Column field="tax_code" header="MST">
                  <template #filter="{ filterModel, filterCallback }">
                    <InputText
                      size="small"
                      v-model="filterModel.value"
                      placeholder="MST"
                      @input="filterCallback()"
                    />
                  </template>
                </Column>
                <Column field="address" header="Địa chỉ">
                  <template #filter="{ filterModel, filterCallback }">
                    <InputText
                      size="small"
                      v-model="filterModel.value"
                      placeholder="Địa chỉ"
                      @input="filterCallback()"
                    />
                  </template>
                </Column>
                <Column field="phone" header="Điện thoại">
                  <template #filter="{ filterModel, filterCallback }">
                    <InputText
                      size="small"
                      v-model="filterModel.value"
                      placeholder="ĐT"
                      @input="filterCallback()"
                    />
                  </template>
                </Column>
                <template #empty
                  ><EmptyState text="Chưa có nhà cung cấp." icon="pi pi-truck"
                /></template>
              </AppDataTable>
            </TabPanel>

            <!-- Nhóm ngành -->
            <TabPanel value="industryGroups">
              <div class="mb-3 flex items-center justify-end gap-2">
                <Button
                  v-if="auth.canAccounting"
                  label="Sửa"
                  icon="pi pi-pencil"
                  severity="secondary"
                  outlined
                  :disabled="igSelection.length !== 1"
                  @click="openIgEdit"
                />
                <Button
                  v-if="auth.canAccounting"
                  label="Xóa"
                  icon="pi pi-trash"
                  severity="danger"
                  outlined
                  :disabled="!igSelection.length"
                  @click="deleteSelectedIg"
                />
                <Button
                  v-if="auth.canAccounting"
                  label="Thêm nhóm ngành"
                  icon="pi pi-plus"
                  @click="openIgCreate"
                />
              </div>
              <!-- Nhóm ngành: danh mục cố định theo hệ thống thuế, nạp một lần. -->
              <AppDataTable
                v-model:selection="igSelection"
                :value="industryGroups"
                stripedRows
                data-key="code"
              >
                <Column field="code" header="Mã" />
                <Column field="name" header="Tên nhóm" />
                <Column field="vat_rate" header="Thuế GTGT (%)">
                  <template #body="{ data }">{{ data.vat_rate * 100 + "%" }}</template>
                </Column>
                <Column field="pit_rate" header="Thuế TNCN (%)">
                  <template #body="{ data }">{{ data.pit_rate * 100 + "%" }}</template>
                </Column>
                <template #empty
                  ><EmptyState text="Chưa có nhóm ngành nào." icon="pi pi-sitemap"
                /></template>
              </AppDataTable>
            </TabPanel>
          </TabPanels>
        </Tabs>
      </template>
    </Card>

    <!-- Dialog dùng chung: thêm / sửa kho, khách hàng, nhà cung cấp -->
    <PartnerDialog
      v-model:visible="dialog"
      :kind="dialogKind"
      :editing="editing"
      :initial="editingRow ?? undefined"
      :show-action="auth.canStock"
    />

    <!-- Dialog thêm nhóm ngành -->
    <AppDialog
      v-model:visible="igDialog"
      :header="igDialogTitle"
      width="max-w-md"
      action-label="Lưu"
      :saving="saving"
      :show-action="auth.canAccounting"
      @action="saveIg"
    >
      <div class="grid grid-cols-1 sm:grid-cols-2 gap-4 py-2">
        <FormField label="Mã" required>
          <InputText v-model="igForm.code" :disabled="igEditing" placeholder="PPHH" />
        </FormField>
        <FormField label="Tên nhóm" required>
          <InputText v-model="igForm.name" placeholder="Phân phối, cung cấp hàng hóa" />
        </FormField>
        <FormField label="Thuế GTGT (%)">
          <InputNumber v-model="igForm.vat_rate" :min="0" :max="100" mode="decimal" suffix="%" />
        </FormField>
        <FormField label="Thuế TNCN (%)">
          <InputNumber v-model="igForm.pit_rate" :min="0" :max="100" mode="decimal" suffix="%" />
        </FormField>
        <p class="col-span-2 text-xs text-gray-400">
          Tỷ lệ áp dụng cho doanh thu bán ra của nhóm ngành này (TT 152/2025).
        </p>
      </div>
    </AppDialog>
  </div>
</template>
