<script setup lang="ts">
import { storeToRefs } from "pinia";
import { useAuthStore } from "@/stores/auth";
import { useCatalogStore } from "@/stores/catalog";
import { useStockStore } from "@/stores/stock";
import { fmtInt as fmt } from "@/utils/format";

const catalog = useCatalogStore();
const stock = useStockStore();
const auth = useAuthStore();
const { products, suppliers, warehouses } = storeToRefs(catalog);
const { entries, loading } = storeToRefs(stock);
const toast = useToast();
const router = useRouter();

function printVoucher(row: { voucher_no: string }) {
  router.push("/print/" + encodeURIComponent(row.voucher_no));
}

const dialog = ref(false);
const saving = ref(false);

const form = reactive({
  posting_date: new Date(),
  voucher_no: "PN001",
  description: "",
  supplier_code: "",
  warehouse_code: "",
  note: "",
  items: [] as { product_code: string; quantity: number; unit_price: number }[],
});

const iso = (d: Date | null) => (d ? d.toISOString().slice(0, 10) : "");

function nextVoucherNo() {
  const nums = entries.value
    .filter((e) => e.entry_type === "PN")
    .map((e) => parseInt(e.voucher_no.replace(/\D/g, ""), 10))
    .filter((n) => !isNaN(n));
  const next = nums.length ? Math.max(...nums) + 1 : 1;
  form.voucher_no = `PN${String(next).padStart(3, "0")}`;
}

function openCreate() {
  Object.assign(form, {
    posting_date: new Date(),
    description: "Phiếu nhập kho",
    supplier_code: "",
    warehouse_code: warehouses.value[0]?.code ?? "",
    note: "",
    items: [],
  });
  nextVoucherNo();
  dialog.value = true;
}

function addRow() {
  form.items.push({ product_code: "", quantity: 1, unit_price: 0 });
}

function removeRow(i: number) {
  form.items.splice(i, 1);
}

async function save() {
  if (!form.posting_date || !form.items.length) {
    toast.add({
      severity: "warn",
      summary: "Thiếu thông tin",
      detail: "Cần ngày và ít nhất 1 mặt hàng",
    });
    return;
  }
  const invalid = form.items.find((it) => !it.product_code);
  if (invalid) {
    toast.add({
      severity: "warn",
      summary: "Thiếu mã sản phẩm",
      detail: "Chọn sản phẩm cho từng dòng",
    });
    return;
  }
  saving.value = true;
  try {
    const res = JSON.parse(
      await stock.inbound({
        posting_date: iso(form.posting_date),
        voucher_no: form.voucher_no,
        description: form.description,
        supplier_code: form.supplier_code,
        warehouse_code: form.warehouse_code,
        unit_code: "HKD",
        note: form.note,
        items: form.items.map((it) => ({ ...it })),
      }),
    );
    toast.add({
      severity: "success",
      summary: `Đã nhập kho ${form.voucher_no}`,
      detail: `Tổng tiền: ${fmt(res.total)} đ`,
    });
    dialog.value = false;
  } catch (e) {
    toast.add({ severity: "error", summary: "Lỗi", detail: String(e) });
  } finally {
    saving.value = false;
  }
}

onMounted(async () => {
  await Promise.all([catalog.loadAll(), stock.loadEntries("PN")]);
});
</script>

<template>
  <div class="space-y-4">
    <Toolbar>
      <template #start>
        <div class="flex items-center gap-2">
          <i-mdi-download class="text-xl text-blue-500" />
          <h3 class="font-semibold">Phiếu nhập kho</h3>
        </div>
      </template>
      <template #end>
        <Button
          v-if="auth.canStock"
          label="Tạo phiếu nhập"
          icon="pi pi-plus"
          @click="openCreate"
        />
      </template>
    </Toolbar>

    <Card>
      <template #content>
        <AppDataTable
          :value="entries"
          :loading="loading"
          stripedRows
          paginator
          :rows="10"
          actions-header="In"
          actions-width="64"
        >
          <Column field="voucher_no" header="Số phiếu" />
          <Column field="posting_date" header="Ngày" />
          <Column field="product_code" header="Mã SP" />
          <Column field="description" header="Diễn giải" />
          <Column field="supplier_name" header="Nhà cung cấp">
            <template #body="{ data }">{{
              data.supplier_name || "—"
            }}</template>
          </Column>
          <Column field="quantity" header="SL" align="right" />
          <Column field="unit_price" header="Đơn giá" align="right">
            <template #body="{ data }">{{ fmt(data.unit_price) }}</template>
          </Column>
          <Column field="amount" header="Thành tiền" align="right">
            <template #body="{ data }">{{ fmt(data.amount) }}</template>
          </Column>
          <template #actions="{ data }">
            <Button
              icon="pi pi-print"
              text
              rounded
              size="small"
              @click="printVoucher(data)"
            />
          </template>
          <template #empty
            ><EmptyState text="Chưa có phiếu nhập kho." icon="pi pi-download"
          /></template>
        </AppDataTable>
      </template>
    </Card>

    <AppDialog
      v-model:visible="dialog"
      header="Tạo phiếu nhập kho"
      width="max-w-3xl"
      action-label="Lưu phiếu"
      :saving="saving"
      :show-action="auth.canStock"
      @action="save"
    >
      <div class="grid grid-cols-3 gap-4 py-2">
        <FormField label="Ngày nhập">
          <DatePicker
            v-model="form.posting_date"
            dateFormat="dd/mm/yy"
            class="w-full"
          />
        </FormField>
        <FormField label="Số phiếu">
          <InputText v-model="form.voucher_no" disabled />
        </FormField>
        <FormField label="Kho nhập">
          <Select
            v-model="form.warehouse_code"
            :options="warehouses"
            optionLabel="name"
            optionValue="code"
            class="w-full"
          />
        </FormField>
        <FormField label="Nhà cung cấp" class="col-span-2">
          <Select
            v-model="form.supplier_code"
            :options="suppliers"
            optionLabel="name"
            optionValue="code"
            :editable="true"
            filter
            class="w-full"
            placeholder="Chọn hoặc nhập tên NCC"
          />
        </FormField>
        <FormField label="Diễn giải">
          <InputText v-model="form.description" />
        </FormField>
      </div>

      <LineItemsEditor
        :items="form.items"
        :products="products"
        :can-edit="auth.canStock"
        price-field="cost_price"
        show-amount
        @add="addRow"
        @remove="removeRow"
      />
    </AppDialog>
  </div>
</template>
