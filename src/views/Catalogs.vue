<script setup lang="ts">
import { storeToRefs } from "pinia";
import { FilterMatchMode } from "@primevue/core/api";
import { useAuthStore } from "@/stores/auth";
import { useCatalogStore } from "@/stores/catalog";
import { api } from "@/db";
import type {
  Warehouse,
  Customer,
  Supplier,
  IndustryGroup,
  TaxInfo,
  TaxResult,
} from "@/types";

const catalog = useCatalogStore();
const auth = useAuthStore();
const { warehouses, suppliers, customers, industryGroups, loading } =
  storeToRefs(catalog);
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

type Kind = "warehouse" | "customer" | "supplier";

const tab = ref("warehouses");
const dialog = ref(false);
const dialogKind = ref<Kind>("warehouse");
const editing = ref(false);
const saving = ref(false);

const form = reactive({
  code: "",
  name: "",
  tax_code: "",
  address: "",
  phone: "",
});

// ─── Tra cứu MST ───
const lookupLoading = ref(false);
const lookupResults = ref<TaxResult[]>([]);
const lookupSelectedUrl = ref("");

// ─── Nhóm ngành (Thêm / Sửa qua dialog; Xóa dòng đang chọn) ───
const igDialog = ref(false);
const igEditing = ref(false);
const igForm = reactive({ code: "", name: "", vat_rate: 0, pit_rate: 0 });
const igSelection = ref<IndustryGroup[]>([]);
// Selection theo tab (kho / khách / NCC) — chọn dòng rồi bấm Sửa ở header.
const whSelection = ref<Warehouse[]>([]);
const cuSelection = ref<Customer[]>([]);
const suSelection = ref<Supplier[]>([]);
const igDialogTitle = computed(() =>
  igEditing.value ? "Sửa nhóm ngành" : "Thêm nhóm ngành",
);

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
  const label =
    rows.length === 1
      ? `nhóm ngành ${rows[0].code}`
      : `${rows.length} nhóm ngành`;
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

const dialogTitle = computed(() => {
  const base = editing.value ? "Sửa" : "Thêm";
  switch (dialogKind.value) {
    case "warehouse":
      return `${base} kho`;
    case "customer":
      return `${base} khách hàng`;
    default:
      return `${base} nhà cung cấp`;
  }
});

const kindLabel = computed(() => {
  switch (dialogKind.value) {
    case "warehouse":
      return "kho";
    case "customer":
      return "khách hàng";
    case "supplier":
      return "nhà cung cấp";
  }
});

const codePlaceholder = computed(() => {
  switch (dialogKind.value) {
    case "warehouse":
      return "KHO01";
    case "customer":
      return "KH001";
    case "supplier":
      return "NCC001";
  }
});

async function openCreate(kind: Kind) {
  dialogKind.value = kind;
  editing.value = false;
  Object.assign(form, {
    code: "",
    name: "",
    tax_code: "",
    address: "",
    phone: "",
  });
  resetLookup();
  // Tự sinh mã: kho KHO001, khách KH001, nhà cung cấp NCC001... — vẫn sửa tay được.
  const gen =
    kind === "warehouse"
      ? api.nextWarehouseCode()
      : kind === "customer"
        ? api.nextCustomerCode()
        : api.nextSupplierCode();
  try {
    form.code = await gen;
  } catch {
    /* giữ trống — người dùng tự nhập */
  }
  dialog.value = true;
}

function openEdit(kind: Kind, row: Warehouse | Customer | Supplier) {
  dialogKind.value = kind;
  editing.value = true;
  Object.assign(form, {
    code: row.code,
    name: row.name,
    tax_code: "",
    address: "",
    phone: "",
  });
  if (kind !== "warehouse") {
    const c = row as Customer;
    form.tax_code = c.tax_code ?? "";
    form.address = c.address ?? "";
    form.phone = c.phone ?? "";
  }
  resetLookup();
  dialog.value = true;
}

function resetLookup() {
  lookupLoading.value = false;
  lookupResults.value = [];
  lookupSelectedUrl.value = "";
}

/** Điền thông tin tra cứu được vào form (chỉ ghi đè khi có giá trị). */
function applyTaxInfo(info: TaxInfo) {
  const t = (v?: string) => (v ?? "").trim();
  const name = t(info.name);
  const addr = t(info.address);
  const phone = t(info.phone);
  if (name) form.name = name;
  if (addr) form.address = addr;
  if (phone) form.phone = phone;
}

async function runLookup() {
  const mst = form.tax_code.trim();
  if (!mst) {
    toast.add({
      severity: "warn",
      summary: "Thiếu MST",
      detail: "Nhập mã số thuế trước khi tra cứu",
    });
    return;
  }
  lookupLoading.value = true;
  lookupResults.value = [];
  lookupSelectedUrl.value = "";
  try {
    const outcome = await api.lookupTaxCode(mst);
    if (outcome.kind === "single") {
      applyTaxInfo(outcome.info);
      toast.add({
        severity: "success",
        summary: "Đã tra cứu",
        detail: `Tìm thấy: ${outcome.info.name ?? outcome.info.tax_code ?? mst}`,
      });
    } else if (outcome.kind === "multiple") {
      lookupResults.value = outcome.results;
      toast.add({
        severity: "info",
        summary: "Nhiều kết quả",
        detail: `Có ${outcome.results.length} kết quả — chọn đúng bên dưới`,
      });
    } else {
      toast.add({
        severity: "warn",
        summary: "Không tìm thấy",
        detail: outcome.message,
      });
    }
  } catch (e) {
    toast.add({ severity: "error", summary: "Lỗi tra cứu", detail: String(e) });
  } finally {
    lookupLoading.value = false;
  }
}

async function applyLookupResult(url: string) {
  if (!url) return;
  try {
    const info = await api.lookupTaxDetail(url);
    applyTaxInfo(info);
    lookupSelectedUrl.value = url;
    toast.add({
      severity: "success",
      summary: "Đã điền thông tin",
      detail: info.name ?? "Xong",
    });
  } catch (e) {
    toast.add({ severity: "error", summary: "Lỗi lấy chi tiết", detail: String(e) });
  }
}

async function save() {
  if (!form.code || !form.name) {
    toast.add({
      severity: "warn",
      summary: "Thiếu thông tin",
      detail: "Mã và tên là bắt buộc",
    });
    return;
  }
  saving.value = true;
  try {
    if (dialogKind.value === "warehouse") {
      await catalog.saveWarehouse(form.code, form.name);
    } else if (dialogKind.value === "customer") {
      await catalog.saveCustomer({
        code: form.code,
        name: form.name,
        tax_code: form.tax_code,
        address: form.address,
        phone: form.phone,
      });
    } else {
      await catalog.saveSupplier({
        code: form.code,
        name: form.name,
        tax_code: form.tax_code,
        address: form.address,
        phone: form.phone,
      });
    }
    toast.add({
      severity: "success",
      summary: "Đã lưu",
      detail: `Đã lưu ${kindLabel.value} ${form.code}`,
    });
    dialog.value = false;
  } catch (e) {
    toast.add({ severity: "error", summary: "Lỗi lưu", detail: String(e) });
  } finally {
    saving.value = false;
  }
}

async function reload() {
  try {
    await catalog.loadAll();
  } catch (e) {
    toast.add({ severity: "error", summary: "Lỗi tải lại", detail: String(e) });
  }
}

// Sửa qua dialog: chọn 1 dòng (tap) rồi bấm Sửa trên header tab.

onMounted(() => catalog.loadAll());
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
        <Button
          label="Tải lại"
          icon="pi pi-refresh"
          severity="secondary"
          @click="reload"
        />
      </template>
    </Toolbar>

    <Card>
      <template #content>
        <Tabs v-model:value="tab">
          <TabList>
            <Tab value="warehouses"
              ><i class="pi pi-warehouse mr-2" />Kho hàng</Tab
            >
            <Tab value="customers"
              ><i class="pi pi-users mr-2" />Khách hàng</Tab
            >
            <Tab value="suppliers"
              ><i class="pi pi-truck mr-2" />Nhà cung cấp</Tab
            >
            <Tab value="industryGroups"
              ><i class="pi pi-sitemap mr-2" />Nhóm ngành
              <i
                class="pi pi-info-circle text-xs text-gray-400 cursor-help"
                v-tooltip="'Tỷ lệ GTGT/TNCN áp dụng cho doanh thu bán ra của nhóm ngành (TT 152/2025).'"
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
                :value="warehouses"
                :loading="loading"
                stripedRows
                data-key="id"
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
                :value="customers"
                :loading="loading"
                stripedRows
                data-key="id"
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
                :value="suppliers"
                :loading="loading"
                stripedRows
                data-key="id"
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
              <AppDataTable
                v-model:selection="igSelection"
                :value="industryGroups"
                :loading="loading"
                stripedRows
                data-key="code"
              >
                <Column field="code" header="Mã" />
                <Column field="name" header="Tên nhóm" />
                <Column field="vat_rate" header="Thuế GTGT (%)">
                  <template #body="{ data }">{{
                    data.vat_rate * 100 + "%"
                  }}</template>
                </Column>
                <Column field="pit_rate" header="Thuế TNCN (%)">
                  <template #body="{ data }">{{
                    data.pit_rate * 100 + "%"
                  }}</template>
                </Column>
                <template #empty
                  ><EmptyState
                    text="Chưa có nhóm ngành nào."
                    icon="pi pi-sitemap"
                /></template>
              </AppDataTable>
            </TabPanel>
          </TabPanels>
        </Tabs>
      </template>
    </Card>

    <!-- Dialog dùng chung: thêm / sửa kho, khách hàng, nhà cung cấp -->
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
        <FormField
          :label="dialogKind === 'warehouse' ? 'Mã kho' : 'Mã'"
          required
        >
          <InputText
            v-model="form.code"
            :placeholder="codePlaceholder"
            :disabled="editing"
          />
        </FormField>
        <FormField
          :label="dialogKind === 'warehouse' ? 'Tên kho' : 'Tên'"
          required
        >
          <InputText v-model="form.name" placeholder="Tên hiển thị" />
        </FormField>

        <template v-if="dialogKind !== 'warehouse'">
          <FormField label="MST">
            <div class="flex gap-2">
              <InputText
                v-model="form.tax_code"
                placeholder="Mã số thuế"
                class="min-w-0 flex-1"
              />
              <Button
                label="Tra cứu"
                icon="pi pi-search"
                severity="secondary"
                class="shrink-0 whitespace-nowrap"
                :loading="lookupLoading"
                :disabled="!form.tax_code.trim()"
                @click="runLookup"
              />
            </div>
          </FormField>
          <FormField
            v-if="lookupResults.length"
            label="Kết quả tra cứu"
            class="col-span-2"
          >
            <Select
              v-model="lookupSelectedUrl"
              :options="lookupResults"
              option-label="name"
              option-value="url"
              placeholder="Có nhiều kết quả — chọn đúng hộ / doanh nghiệp…"
              class="w-full"
              @change="applyLookupResult($event.value)"
            />
          </FormField>
          <FormField label="Điện thoại">
            <InputText v-model="form.phone" placeholder="Số điện thoại" />
          </FormField>
          <FormField label="Địa chỉ" class="col-span-2">
            <InputText v-model="form.address" placeholder="Địa chỉ" />
          </FormField>
        </template>
      </div>
    </AppDialog>

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
      <div class="grid grid-cols-2 gap-4 py-2">
        <FormField label="Mã" required>
          <InputText
            v-model="igForm.code"
            :disabled="igEditing"
            placeholder="PPHH"
          />
        </FormField>
        <FormField label="Tên nhóm" required>
          <InputText
            v-model="igForm.name"
            placeholder="Phân phối, cung cấp hàng hóa"
          />
        </FormField>
        <FormField label="Thuế GTGT (%)">
          <InputNumber
            v-model="igForm.vat_rate"
            :min="0"
            :max="100"
            mode="decimal"
            suffix="%"
          />
        </FormField>
        <FormField label="Thuế TNCN (%)">
          <InputNumber
            v-model="igForm.pit_rate"
            :min="0"
            :max="100"
            mode="decimal"
            suffix="%"
          />
        </FormField>
        <p class="col-span-2 text-xs text-gray-400">
          Tỷ lệ áp dụng cho doanh thu bán ra của nhóm ngành này (TT 152/2025).
        </p>
      </div>
    </AppDialog>
  </div>
</template>
