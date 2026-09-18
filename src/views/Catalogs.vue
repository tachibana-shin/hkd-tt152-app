<script setup lang="ts">
import { storeToRefs } from "pinia";
import { FilterMatchMode } from "@primevue/core/api";
import { useAuthStore } from "@/stores/auth";
import { useCatalogStore } from "@/stores/catalog";
import { api } from "@/db";
import type { Warehouse, Customer, Supplier, TaxInfo, TaxResult } from "@/types";

const catalog = useCatalogStore();
const auth = useAuthStore();
const { warehouses, suppliers, customers, industryGroups, loading } =
  storeToRefs(catalog);
const toast = useToast();

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

const dialogTitle = computed(() => {
  const base = editing.value ? "Sửa" : "Thêm";
  switch (dialogKind.value) {
    case "warehouse":
      return `${base} kho`;
    case "customer":
      return `${base} khách hàng`;
    case "supplier":
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
  // Kho: tự sinh mã như sản phẩm (KHO001, KHO002...) — vẫn sửa tay được.
  if (kind === "warehouse") {
    try {
      form.code = await api.nextWarehouseCode();
    } catch {
      /* giữ trống — người dùng tự nhập */
    }
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

// Sửa ô trực tiếp (double-tap) — AppDataTable phát cell-save khi commit;
// lưu DB bằng đúng action đang dùng trong dialog, rồi tải lại bảng.
function onCellSave(
  kind: Kind,
  { field, data }: { field: string; data: any },
) {
  const str = (s: unknown) => String(s ?? "").trim();
  if (field === "name" && !str(data.name)) {
    toast.add({
      severity: "warn",
      summary: "Không hợp lệ",
      detail: "Tên không được để trống",
    });
    return;
  }
  const label =
    kind === "warehouse"
      ? "kho"
      : kind === "customer"
        ? "khách hàng"
        : "nhà cung cấp";
  const saving =
    kind === "warehouse"
      ? catalog.saveWarehouse(data.code, data.name)
      : kind === "customer"
        ? catalog.saveCustomer({
            code: data.code,
            name: data.name,
            tax_code: str(data.tax_code),
            address: str(data.address),
            phone: str(data.phone),
          })
        : catalog.saveSupplier({
            code: data.code,
            name: data.name,
            tax_code: str(data.tax_code),
            address: str(data.address),
            phone: str(data.phone),
          });
  saving
    .then(() => {
      toast.add({
        severity: "success",
        summary: "Đã lưu",
        detail: `Đã lưu ${label} ${data.code}`,
      });
    })
    .catch((e: unknown) => {
      toast.add({ severity: "error", summary: "Lỗi lưu", detail: String(e) });
    })
    .finally(() => reload());
}

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
                v-tooltip="'Tỷ lệ theo TT 152/2025, chỉnh qua dữ liệu mẫu.'"
            /></Tab>
          </TabList>
          <TabPanels>
            <!-- Kho hàng -->
            <TabPanel value="warehouses">
              <div class="mb-3 flex items-center justify-end">
                <Button
                  v-if="auth.canStock"
                  label="Thêm kho"
                  icon="pi pi-plus"
                  @click="openCreate('warehouse')"
                />
              </div>
              <AppDataTable
                v-model:filters="filters"
                :value="warehouses"
                :loading="loading"
                stripedRows
                data-key="id"
                @cell-save="onCellSave('warehouse', $event)"
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
                <Column field="name" header="Tên kho" :editable="auth.canStock">
                  <template #editor="{ data }">
                    <InputText size="small" class="w-full" v-model="data.name" />
                  </template>
                  <template #filter="{ filterModel, filterCallback }">
                    <InputText
                      size="small"
                      v-model="filterModel.value"
                      placeholder="Tên"
                      @input="filterCallback()"
                    />
                  </template>
                </Column>
                <Column header="">
                  <template #body="{ data }">
                    <Button
                      v-if="auth.canStock"
                      icon="pi pi-pencil"
                      text
                      rounded
                      size="small"
                      @click="openEdit('warehouse', data)"
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
              <div class="mb-3 flex items-center justify-end">
                <Button
                  v-if="auth.canStock"
                  label="Thêm khách hàng"
                  icon="pi pi-plus"
                  @click="openCreate('customer')"
                />
              </div>
              <AppDataTable
                v-model:filters="filters"
                :value="customers"
                :loading="loading"
                stripedRows
                data-key="id"
                @cell-save="onCellSave('customer', $event)"
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
                <Column field="name" header="Tên" :editable="auth.canStock">
                  <template #editor="{ data }">
                    <InputText size="small" class="w-full" v-model="data.name" />
                  </template>
                  <template #filter="{ filterModel, filterCallback }">
                    <InputText
                      size="small"
                      v-model="filterModel.value"
                      placeholder="Tên"
                      @input="filterCallback()"
                    />
                  </template>
                </Column>
                <Column
                  field="tax_code"
                  header="MST"
                  :editable="auth.canStock"
                >
                  <template #editor="{ data }">
                    <InputText
                      size="small"
                      class="w-full"
                      v-model="data.tax_code"
                    />
                  </template>
                  <template #filter="{ filterModel, filterCallback }">
                    <InputText
                      size="small"
                      v-model="filterModel.value"
                      placeholder="MST"
                      @input="filterCallback()"
                    />
                  </template>
                </Column>
                <Column field="address" header="Địa chỉ" :editable="auth.canStock">
                  <template #editor="{ data }">
                    <InputText
                      size="small"
                      class="w-full"
                      v-model="data.address"
                    />
                  </template>
                  <template #filter="{ filterModel, filterCallback }">
                    <InputText
                      size="small"
                      v-model="filterModel.value"
                      placeholder="Địa chỉ"
                      @input="filterCallback()"
                    />
                  </template>
                </Column>
                <Column field="phone" header="Điện thoại" :editable="auth.canStock">
                  <template #editor="{ data }">
                    <InputText size="small" class="w-full" v-model="data.phone" />
                  </template>
                  <template #filter="{ filterModel, filterCallback }">
                    <InputText
                      size="small"
                      v-model="filterModel.value"
                      placeholder="ĐT"
                      @input="filterCallback()"
                    />
                  </template>
                </Column>
                <Column header="">
                  <template #body="{ data }">
                    <Button
                      v-if="auth.canStock"
                      icon="pi pi-pencil"
                      text
                      rounded
                      size="small"
                      @click="openEdit('customer', data)"
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
              <div class="mb-3 flex items-center justify-end">
                <Button
                  v-if="auth.canStock"
                  label="Thêm nhà cung cấp"
                  icon="pi pi-plus"
                  @click="openCreate('supplier')"
                />
              </div>
              <AppDataTable
                v-model:filters="filters"
                :value="suppliers"
                :loading="loading"
                stripedRows
                data-key="id"
                @cell-save="onCellSave('supplier', $event)"
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
                <Column field="name" header="Tên" :editable="auth.canStock">
                  <template #editor="{ data }">
                    <InputText size="small" class="w-full" v-model="data.name" />
                  </template>
                  <template #filter="{ filterModel, filterCallback }">
                    <InputText
                      size="small"
                      v-model="filterModel.value"
                      placeholder="Tên"
                      @input="filterCallback()"
                    />
                  </template>
                </Column>
                <Column
                  field="tax_code"
                  header="MST"
                  :editable="auth.canStock"
                >
                  <template #editor="{ data }">
                    <InputText
                      size="small"
                      class="w-full"
                      v-model="data.tax_code"
                    />
                  </template>
                  <template #filter="{ filterModel, filterCallback }">
                    <InputText
                      size="small"
                      v-model="filterModel.value"
                      placeholder="MST"
                      @input="filterCallback()"
                    />
                  </template>
                </Column>
                <Column field="address" header="Địa chỉ" :editable="auth.canStock">
                  <template #editor="{ data }">
                    <InputText
                      size="small"
                      class="w-full"
                      v-model="data.address"
                    />
                  </template>
                  <template #filter="{ filterModel, filterCallback }">
                    <InputText
                      size="small"
                      v-model="filterModel.value"
                      placeholder="Địa chỉ"
                      @input="filterCallback()"
                    />
                  </template>
                </Column>
                <Column field="phone" header="Điện thoại" :editable="auth.canStock">
                  <template #editor="{ data }">
                    <InputText size="small" class="w-full" v-model="data.phone" />
                  </template>
                  <template #filter="{ filterModel, filterCallback }">
                    <InputText
                      size="small"
                      v-model="filterModel.value"
                      placeholder="ĐT"
                      @input="filterCallback()"
                    />
                  </template>
                </Column>
                <Column header="">
                  <template #body="{ data }">
                    <Button
                      v-if="auth.canStock"
                      icon="pi pi-pencil"
                      text
                      rounded
                      size="small"
                      @click="openEdit('supplier', data)"
                    />
                  </template>
                </Column>
                <template #empty
                  ><EmptyState text="Chưa có nhà cung cấp." icon="pi pi-truck"
                /></template>
              </AppDataTable>
            </TabPanel>

            <!-- Nhóm ngành (chỉ xem) -->
            <TabPanel value="industryGroups">
              <AppDataTable
                :value="industryGroups"
                :loading="loading"
                stripedRows
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
  </div>
</template>
