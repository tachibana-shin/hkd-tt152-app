<script setup lang="ts">
import { storeToRefs } from "pinia";
import { useAuthStore } from "@/stores/auth";
import { useCatalogStore } from "@/stores/catalog";
import { useStockStore } from "@/stores/stock";
import PartnerDialog from "@/components/PartnerDialog.vue";
import { fmtInt as fmt, fmtVnd } from "@/utils/format";

const catalog = useCatalogStore();
const stock = useStockStore();
const auth = useAuthStore();
const { products, customers, industryGroups, warehouses } = storeToRefs(catalog);
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
  voucher_no: "PX001",
  description: "Phiếu xuất kho / bán hàng",
  customer_code: "",
  note: "",
  // Loại xuất: "sale" (bán hàng, mặc định) | "adjust" (điều chỉnh hóa đơn bán).
  outbound_type: "sale" as "sale" | "adjust",
  // Hướng điều chỉnh (chỉ dùng khi loại = "adjust"):
  //   down = khách trả lại / giảm doanh thu (nhập lại hàng về kho)
  //   up   = tăng thêm doanh thu
  adjust_dir: "up" as "up" | "down",
  // Thu tiền ngay: bật mặc định — khi lưu tự tạo phiếu thu (PT) thu tiền khách.
  receive_now: true,
  items: [] as {
    product_code: string;
    quantity: number;
    unit_price: number;
    industry_code: string;
    warehouse_code: string;
  }[],
});

const typeOptions = [
  { label: "Bán hàng", value: "sale" },
  { label: "Điều chỉnh hóa đơn bán", value: "adjust" },
];

const adjustDirOptions = [
  { label: "Tăng (tăng doanh thu)", value: "up" },
  { label: "Giảm (khách trả lại)", value: "down" },
];

function onTypeChange() {
  // Phiếu điều chỉnh hóa đơn không thu tiền mới → tắt "Thu tiền ngay".
  if (form.outbound_type === "adjust") {
    form.receive_now = false;
    form.description = "Điều chỉnh hóa đơn bán";
  } else {
    form.receive_now = true;
    form.description = "Phiếu xuất kho / bán hàng";
  }
}

const iso = (d: Date | null) => (d ? d.toISOString().slice(0, 10) : "");

function nextVoucherNo() {
  const nums = entries.value
    .filter((e) => e.entry_type === "PX")
    .map((e) => parseInt(e.voucher_no.replace(/\D/g, ""), 10))
    .filter((n) => !isNaN(n));
  const next = nums.length ? Math.max(...nums) + 1 : 1;
  form.voucher_no = `PX${String(next).padStart(3, "0")}`;
}

function openCreate() {
  Object.assign(form, {
    posting_date: new Date(),
    description: "Phiếu xuất kho / bán hàng",
    customer_code: "",
    note: "",
    outbound_type: "sale",
    adjust_dir: "up",
    receive_now: true,
    items: [],
  });
  nextVoucherNo();
  dialog.value = true;
}

// ─── Thêm khách hàng nhanh: dùng chung PartnerDialog (tự sinh mã + tra cứu MST) ───
const customerDialog = ref(false);

function onCustomerSaved(code: string) {
  form.customer_code = code;
}

function addRow() {
  form.items.push({
    product_code: "",
    quantity: 1,
    unit_price: 0,
    industry_code: industryGroups.value[0]?.code ?? "PPHH",
    warehouse_code: "", // rỗng → kho mặc định của sản phẩm / kho đầu tiên
  });
}

function removeRow(i: number) {
  form.items.splice(i, 1);
}

async function save() {
  // Kiểu nhập liên tục (pre-input) luôn để lại dòng trống cuối → bỏ dòng chưa chọn hàng.
  const rows = form.items.filter((it) => it.product_code);
  if (!form.posting_date || !rows.length) {
    toast.add({
      severity: "warn",
      summary: "Thiếu thông tin",
      detail: "Cần ngày và ít nhất 1 mặt hàng",
    });
    return;
  }
  const invalid = rows.find((it) => !it.industry_code);
  if (invalid) {
    toast.add({
      severity: "warn",
      summary: "Thiếu thông tin",
      detail: "Chọn sản phẩm và nhóm ngành cho từng dòng",
    });
    return;
  }
  saving.value = true;
  try {
    const res = JSON.parse(
      await stock.outbound({
        posting_date: iso(form.posting_date),
        voucher_no: form.voucher_no,
        description: form.description,
        customer_code: form.customer_code,
        unit_code: "HKD",
        note: form.note,
        items: rows.map((it) => ({
          ...it,
          warehouse_code: it.warehouse_code ?? "", // showClear có thể trả null
        })),
        // Thu tiền ngay: backend tự tạo + liên kết phiếu thu (PT) của khách hàng
        // (chỉ với phiếu bán hàng; phiếu điều chỉnh không thu tiền mới).
        receive_now: form.outbound_type === "sale" && form.receive_now,
        outbound_type: form.outbound_type,
        adjust_dir: form.adjust_dir,
      }),
    );
    toast.add({
      severity: "success",
      summary: `Đã xuất kho ${form.voucher_no}`,
      detail: `Doanh thu: ${fmt(res.revenue)} đ • Lãi gộp: ${fmt(res.profit)} đ${
        res.pt_no ? ` • Đã tạo phiếu thu ${res.pt_no}` : ""
      }`,
    });
    dialog.value = false;
  } catch (e) {
    toast.add({
      severity: "error",
      summary: "Không xuất được",
      detail: String(e),
    });
  } finally {
    saving.value = false;
  }
}

onMounted(async () => {
  await Promise.all([catalog.loadAll(), stock.loadEntries("PX")]);
});
</script>

<template>
  <div class="space-y-4">
    <Toolbar>
      <template #start>
        <div class="flex items-center gap-2">
          <i-mdi-upload class="text-xl text-orange-500" />
          <h3 class="font-semibold">Phiếu xuất kho / Bán hàng</h3>
        </div>
      </template>
      <template #end>
        <Button
          v-if="auth.canStock"
          label="Tạo phiếu xuất"
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
          <Column field="customer_name" header="Khách hàng">
            <template #body="{ data }">{{
              data.customer_name || "—"
            }}</template>
          </Column>
          <Column field="industry_code" header="Nhóm ngành">
            <template #body="{ data }">
              <Tag :value="data.industry_code" severity="info" />
            </template>
          </Column>
          <Column field="quantity" header="SL" align="right" />
          <Column field="unit_price" header="Đơn giá" align="right">
            <template #body="{ data }">{{ fmtVnd(data.unit_price) }}</template>
          </Column>
          <Column field="amount" header="Thành tiền" align="right">
            <template #body="{ data }">{{ fmtVnd(data.amount) }}</template>
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
            ><EmptyState text="Chưa có phiếu xuất kho." icon="pi pi-upload"
          /></template>
        </AppDataTable>
      </template>
    </Card>

    <AppDialog
      v-model:visible="dialog"
      :header="
        form.outbound_type === 'adjust'
          ? 'Tạo phiếu điều chỉnh hóa đơn bán'
          : 'Tạo phiếu xuất kho / Bán hàng'
      "
      width="max-w-3xl"
      action-label="Lưu phiếu"
      :saving="saving"
      :show-action="auth.canStock"
      @action="save"
    >
      <div class="grid grid-cols-3 gap-4 py-2">
        <FormField label="Loại xuất">
          <Select
            v-model="form.outbound_type"
            :options="typeOptions"
            optionLabel="label"
            optionValue="value"
            size="small"
            class="w-full"
            @change="onTypeChange"
          />
        </FormField>
        <FormField
          v-if="form.outbound_type === 'adjust'"
          label="Hướng điều chỉnh"
        >
          <Select
            v-model="form.adjust_dir"
            :options="adjustDirOptions"
            optionLabel="label"
            optionValue="value"
            size="small"
            class="w-full"
          />
        </FormField>
        <FormField label="Ngày xuất">
          <DatePicker
            v-model="form.posting_date"
            dateFormat="dd/mm/yy"
            size="small"
            class="w-full"
          />
        </FormField>
        <FormField label="Số phiếu">
          <InputText v-model="form.voucher_no" disabled size="small" />
        </FormField>
        <FormField label="Diễn giải">
          <InputText v-model="form.description" size="small" />
        </FormField>
        <FormField label="Khách hàng" class="col-span-2">
          <div class="flex gap-2">
            <Select
              v-model="form.customer_code"
              :options="customers"
              optionLabel="name"
              optionValue="code"
              :editable="true"
              filter
              size="small"
              class="w-full"
              placeholder="Chọn hoặc nhập tên khách hàng"
            />
            <Button
              v-if="auth.canStock"
              icon="pi pi-plus"
              text
              rounded
              severity="secondary"
              aria-label="Thêm khách hàng nhanh"
              v-tooltip="'Thêm khách hàng nhanh'"
              @click="customerDialog = true"
            />
          </div>
        </FormField>
        <FormField label="Ghi chú">
          <InputText v-model="form.note" size="small" />
        </FormField>
        <FormField
          v-if="form.outbound_type === 'sale'"
          label="Thu tiền ngay"
        >
          <div class="flex h-full items-center gap-1.5">
            <ToggleSwitch v-model="form.receive_now" class="shrink-0" />
            <i
              class="pi pi-info-circle cursor-help text-xs text-gray-400 shrink-0"
              v-tooltip="'Bật: khi lưu sẽ tự tạo phiếu thu (PT) thu tiền của khách hàng — cần chọn Khách hàng.'"
              aria-hidden="true"
            />
          </div>
        </FormField>
      </div>

      <LineItemsEditor
        :items="form.items"
        :products="products"
        :industry-groups="industryGroups"
        :warehouses="warehouses"
        :can-edit="auth.canStock"
        price-field="sale_price"
        show-industry
        show-warehouse
        pre-input
        show-add-product
        info="Xuất FIFO theo kho đã chọn ở từng dòng; dòng bỏ trống kho sẽ lấy kho mặc định của sản phẩm."
        @add="addRow"
        @remove="removeRow"
      />
    </AppDialog>

    <!-- Thêm khách hàng nhanh — dùng chung dialog chuẩn (tự sinh mã + tra cứu MST) -->
    <PartnerDialog
      v-model:visible="customerDialog"
      kind="customer"
      :show-action="auth.canStock"
      @saved="onCustomerSaved"
    />
  </div>
</template>
