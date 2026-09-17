<script setup lang="ts">
import { storeToRefs } from "pinia";
import { useAuthStore } from "@/stores/auth";
import { useCatalogStore } from "@/stores/catalog";
import type { Warehouse, Customer, Supplier } from "@/types";

const catalog = useCatalogStore();
const auth = useAuthStore();
const { warehouses, suppliers, customers, industryGroups, loading } = storeToRefs(catalog);
const toast = useToast();

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

function openCreate(kind: Kind) {
  dialogKind.value = kind;
  editing.value = false;
  Object.assign(form, { code: "", name: "", tax_code: "", address: "", phone: "" });
  dialog.value = true;
}

function openEdit(kind: Kind, row: Warehouse | Customer | Supplier) {
  dialogKind.value = kind;
  editing.value = true;
  Object.assign(form, { code: row.code, name: row.name, tax_code: "", address: "", phone: "" });
  if (kind !== "warehouse") {
    const c = row as Customer;
    form.tax_code = c.tax_code ?? "";
    form.address = c.address ?? "";
    form.phone = c.phone ?? "";
  }
  dialog.value = true;
}

async function save() {
  if (!form.code || !form.name) {
    toast.add({ severity: "warn", summary: "Thiếu thông tin", detail: "Mã và tên là bắt buộc" });
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
    toast.add({ severity: "success", summary: "Đã lưu", detail: `Đã lưu ${kindLabel.value} ${form.code}` });
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
            <Tab value="industryGroups"><i class="pi pi-sitemap mr-2" />Nhóm ngành <i class="pi pi-info-circle text-xs text-gray-400 cursor-help" v-tooltip="'Tỷ lệ theo TT 152/2025, chỉnh qua dữ liệu mẫu.'" /></Tab>
          </TabList>
          <TabPanels>
            <!-- Kho hàng -->
            <TabPanel value="warehouses">
              <div class="mb-3 flex items-center justify-end">
                <Button v-if="auth.canStock" label="Thêm kho" icon="pi pi-plus" @click="openCreate('warehouse')" />
              </div>
              <DataTable :value="warehouses" :loading="loading" stripedRows>
                <Column field="code" header="Mã kho" style="width: 160px" />
                <Column field="name" header="Tên kho" />
                <Column header="" style="width: 90px">
                  <template #body="{ data }">
                    <Button v-if="auth.canStock" icon="pi pi-pencil" text rounded size="small" @click="openEdit('warehouse', data)" />
                  </template>
                </Column>
                <template #empty><EmptyState text="Chưa có kho hàng." icon="pi pi-warehouse" /></template>
              </DataTable>
            </TabPanel>

            <!-- Khách hàng -->
            <TabPanel value="customers">
              <div class="mb-3 flex items-center justify-end">
                <Button v-if="auth.canStock" label="Thêm khách hàng" icon="pi pi-plus" @click="openCreate('customer')" />
              </div>
              <DataTable :value="customers" :loading="loading" stripedRows>
                <Column field="code" header="Mã" style="width: 110px" />
                <Column field="name" header="Tên" />
                <Column field="tax_code" header="MST" style="width: 140px" />
                <Column field="address" header="Địa chỉ" />
                <Column field="phone" header="Điện thoại" style="width: 130px" />
                <Column header="" style="width: 90px">
                  <template #body="{ data }">
                    <Button v-if="auth.canStock" icon="pi pi-pencil" text rounded size="small" @click="openEdit('customer', data)" />
                  </template>
                </Column>
                <template #empty><EmptyState text="Chưa có khách hàng." icon="pi pi-users" /></template>
              </DataTable>
            </TabPanel>

            <!-- Nhà cung cấp -->
            <TabPanel value="suppliers">
              <div class="mb-3 flex items-center justify-end">
                <Button v-if="auth.canStock" label="Thêm nhà cung cấp" icon="pi pi-plus" @click="openCreate('supplier')" />
              </div>
              <DataTable :value="suppliers" :loading="loading" stripedRows>
                <Column field="code" header="Mã" style="width: 110px" />
                <Column field="name" header="Tên" />
                <Column field="tax_code" header="MST" style="width: 140px" />
                <Column field="address" header="Địa chỉ" />
                <Column field="phone" header="Điện thoại" style="width: 130px" />
                <Column header="" style="width: 90px">
                  <template #body="{ data }">
                    <Button v-if="auth.canStock" icon="pi pi-pencil" text rounded size="small" @click="openEdit('supplier', data)" />
                  </template>
                </Column>
                <template #empty><EmptyState text="Chưa có nhà cung cấp." icon="pi pi-truck" /></template>
              </DataTable>
            </TabPanel>

            <!-- Nhóm ngành (chỉ xem) -->
            <TabPanel value="industryGroups">
              <DataTable :value="industryGroups" :loading="loading" stripedRows>
                <Column field="code" header="Mã" style="width: 120px" />
                <Column field="name" header="Tên nhóm" />
                <Column field="vat_rate" header="Thuế GTGT (%)" style="width: 140px">
                  <template #body="{ data }">{{ data.vat_rate * 100 + "%" }}</template>
                </Column>
                <Column field="pit_rate" header="Thuế TNCN (%)" style="width: 140px">
                  <template #body="{ data }">{{ data.pit_rate * 100 + "%" }}</template>
                </Column>
                <template #empty><EmptyState text="Chưa có nhóm ngành nào." icon="pi pi-sitemap" /></template>
              </DataTable>
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
        <FormField :label="dialogKind === 'warehouse' ? 'Mã kho' : 'Mã'" required>
          <InputText v-model="form.code" :placeholder="codePlaceholder" :disabled="editing" />
        </FormField>
        <FormField :label="dialogKind === 'warehouse' ? 'Tên kho' : 'Tên'" required>
          <InputText v-model="form.name" placeholder="Tên hiển thị" />
        </FormField>

        <template v-if="dialogKind !== 'warehouse'">
          <FormField label="MST">
            <InputText v-model="form.tax_code" placeholder="Mã số thuế" />
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