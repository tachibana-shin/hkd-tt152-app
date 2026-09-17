<script setup lang="ts">
import { storeToRefs } from "pinia";
import { useAuthStore } from "@/stores/auth";
import { useCatalogStore } from "@/stores/catalog";
import type { Product } from "@/types";
import { fmtInt as fmt } from "@/utils/format";

const catalog = useCatalogStore();
const auth = useAuthStore();
const { products, loading } = storeToRefs(catalog);
const toast = useToast();
const confirm = useConfirm();

const dialog = ref(false);
const saving = ref(false);
const form = reactive({
  id: null as number | null,
  code: "",
  name: "",
  unit: "Cái",
  sale_price: 0,
  cost_price: 0,
  min_stock: 0,
  vat_rate: 1,
  vat_reduced: false,
});

function openCreate() {
  Object.assign(form, {
    id: null, code: "", name: "", unit: "Cái",
    sale_price: 0, cost_price: 0, min_stock: 0,
    vat_rate: 1, vat_reduced: false,
  });
  dialog.value = true;
}

function openEdit(row: Product) {
  Object.assign(form, {
    id: row.id, code: row.code, name: row.name, unit: row.unit,
    sale_price: row.sale_price, cost_price: row.cost_price,
    min_stock: row.min_stock,
    vat_rate: Math.round(row.vat_rate * 100),
    vat_reduced: row.vat_reduced,
  });
  dialog.value = true;
}

async function save() {
  if (!form.code || !form.name) {
    toast.add({ severity: "warn", summary: "Thiếu thông tin", detail: "Mã và tên sản phẩm là bắt buộc" });
    return;
  }
  saving.value = true;
  try {
    await catalog.saveProduct({
      id: form.id ?? undefined,
      code: form.code,
      name: form.name,
      unit: form.unit,
      sale_price: form.sale_price,
      cost_price: form.cost_price,
      min_stock: form.min_stock,
      vat_rate: form.vat_rate / 100,
      vat_reduced: form.vat_reduced,
    });
    toast.add({ severity: "success", summary: "Đã lưu", detail: `Sản phẩm ${form.code}` });
    dialog.value = false;
  } catch (e) {
    toast.add({ severity: "error", summary: "Lỗi", detail: String(e) });
  } finally {
    saving.value = false;
  }
}

function remove(row: Product) {
  confirm.require({
    message: `Xóa sản phẩm "${row.name}"?`,
    header: "Xác nhận xóa",
    icon: "pi pi-exclamation-triangle",
    acceptLabel: "Xóa",
    rejectLabel: "Hủy",
    accept: async () => {
      try {
        await catalog.deleteProduct(row.id);
        toast.add({ severity: "success", summary: "Đã xóa", detail: row.code });
      } catch (e) {
        toast.add({ severity: "error", summary: "Không xóa được", detail: String(e) });
      }
    },
  });
}

onMounted(() => catalog.loadAll());

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
        <Button v-if="auth.canStock" label="Thêm sản phẩm" icon="pi pi-plus" @click="openCreate" />
      </template>
    </Toolbar>

    <Card>
      <template #content>
        <DataTable :value="products" :loading="loading" stripedRows paginator :rows="10">
          <Column field="code" header="Mã SP" style="width: 110px" />
          <Column field="name" header="Tên sản phẩm" />
          <Column field="unit" header="ĐVT" style="width: 80px" />
          <Column field="sale_price" header="Giá bán" style="width: 130px">
            <template #body="{ data }">{{ fmt(data.sale_price) }}</template>
          </Column>
          <Column field="cost_price" header="Giá vốn" style="width: 130px">
            <template #body="{ data }">{{ fmt(data.cost_price) }}</template>
          </Column>
          <Column field="min_stock" header="Tồn tối thiểu" style="width: 110px">
            <template #body="{ data }">{{ fmt(data.min_stock) }}</template>
          </Column>
          <Column field="vat_rate" header="Thuế GTGT" style="width: 100px">
            <template #body="{ data }">
              <Tag :value="data.vat_rate * 100 + '%'" :severity="data.vat_reduced ? 'secondary' : 'info'" />
            </template>
          </Column>
          <Column header="" style="width: 140px" alignFrozen="right">
            <template #body="{ data }">
              <Button v-if="auth.canStock" icon="pi pi-pencil" text rounded size="small" @click="openEdit(data)" />
              <Button v-if="auth.canStock" icon="pi pi-trash" text rounded size="small" severity="danger" @click="remove(data)" />
            </template>
          </Column>
          <template #empty><EmptyState text="Chưa có sản phẩm." icon="pi pi-box" /></template>
        </DataTable>
      </template>
    </Card>

    <AppDialog
      v-model:visible="dialog"
      :header="form.id ? 'Sửa sản phẩm' : 'Thêm sản phẩm'"
      width="max-w-xl"
      action-label="Lưu"
      :saving="saving"
      :show-action="auth.canStock"
      @action="save"
    >
      <div class="grid grid-cols-2 gap-4 py-2">
        <FormField label="Mã sản phẩm" required>
          <InputText v-model="form.code" placeholder="SP001" :disabled="form.id !== null" />
        </FormField>
        <FormField label="Đơn vị tính">
          <InputText v-model="form.unit" placeholder="Cái" />
        </FormField>
        <FormField label="Tên sản phẩm" required class="col-span-2">
          <InputText v-model="form.name" placeholder="Tên hàng hóa / dịch vụ" />
        </FormField>
        <FormField label="Giá bán">
          <InputNumber v-model="form.sale_price" :min="0" mode="currency" currency="VND" locale="vi-VN" class="w-full" />
        </FormField>
        <FormField label="Giá vốn">
          <InputNumber v-model="form.cost_price" :min="0" mode="currency" currency="VND" locale="vi-VN" class="w-full" />
        </FormField>
        <FormField label="Tồn tối thiểu">
          <InputNumber v-model="form.min_stock" :min="0" class="w-full" />
        </FormField>
        <FormField label="Thuế suất GTGT (%)">
          <Select v-model="form.vat_rate" :options="[1, 3, 5]" class="w-full" />
        </FormField>
        <div class="col-span-2 flex items-center gap-2 pt-2">
          <Checkbox v-model="form.vat_reduced" :binary="true" inputId="vat_reduced" />
          <label for="vat_reduced" class="text-sm">Giảm thuế GTGT</label>
          <i class="pi pi-info-circle cursor-help text-xs text-gray-400" v-tooltip="'Theo Nghị quyết 174/2025'" aria-hidden="true" />
        </div>
      </div>
    </AppDialog>
  </div>
</template>