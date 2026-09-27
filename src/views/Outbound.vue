<script setup lang="ts">
import { storeToRefs } from "pinia";
import { useAuthStore } from "@/stores/auth";
import { useCatalogStore } from "@/stores/catalog";
import { useStockStore } from "@/stores/stock";
import PartnerDialog from "@/components/PartnerDialog.vue";
import { api } from "@/db";
import { fmtInt as fmt, fmtVnd, toIsoDate } from "@/utils/format";
import { useKeepAliveRefresh } from "@/composables/useKeepAliveRefresh";

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

// ─── Xem phiếu (chỉ đọc) — dùng chung VoucherViewDialog ───
const viewVoucherNo = ref("");
const viewVoucherVisible = ref(false);

function openVoucher(no: string) {
  viewVoucherNo.value = no;
  viewVoucherVisible.value = true;
}

// Lập phiếu điều chỉnh (khách trả lại hàng) nhanh từ 1 phiếu xuất gốc: tự điền
// khách hàng + từng mặt hàng (SL, đơn giá, nhóm ngành) theo đúng chứng từ.
async function adjustFromVoucher(row: { voucher_no: string }) {
  try {
    const voucher = await api.getVoucher(row.voucher_no);
    const lines = voucher.filter((r) => r.entry_type === "PX" && r.product_code && r.quantity > 0);
    if (!lines.length) {
      toast.add({
        severity: "warn",
        summary: "Không điều chỉnh được",
        detail: `Chứng từ ${row.voucher_no} không có dòng hàng hóa để trả lại`,
      });
      return;
    }
    Object.assign(form, {
      posting_date: new Date(),
      description: `Khách trả lại — theo ${row.voucher_no}`,
      customer_code: voucher[0]?.customer_code ?? "",
      note: "",
      outbound_type: "adjust",
      adjust_dir: "down",
      receive_now: false,
      create_invoice: false, // phiếu trả lại không phát hành hóa đơn mới
      e_invoice_no: "",
      e_invoice_symbol: "",
      e_invoice_date: null,
      items: lines.map((l) => ({
        product_code: l.product_code,
        quantity: l.quantity,
        unit_price: l.unit_price,
        industry_code: l.industry_code || "PPHH",
        warehouse_code: "", // rỗng → kho mặc định của sản phẩm
      })),
    });
    nextVoucherNo();
    dialog.value = true;
  } catch (e) {
    toast.add({ severity: "error", summary: "Lỗi", detail: String(e) });
  }
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
  // Lập kèm hóa đơn bán hàng (tab "Hóa đơn") — số hóa đơn = số phiếu xuất:
  // bật mặc định cho phiếu thực tăng doanh thu (bán thường / điều chỉnh tăng).
  create_invoice: true,
  e_invoice_no: "",
  e_invoice_symbol: "",
  e_invoice_date: null as Date | null,
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
    create_invoice: true,
    e_invoice_no: "",
    e_invoice_symbol: "",
    e_invoice_date: null,
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
        posting_date: toIsoDate(form.posting_date),
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
        // Lập kèm hóa đơn bán hàng (tab "Hóa đơn") — chỉ phiếu thực tăng doanh
        // thu; backend tự bỏ qua với phiếu giảm/trả lại. Số hóa đơn = số phiếu
        // xuất (liên kết 1-1). Khai HĐĐT → hóa đơn chuyển ngay thành "Đã liên kết".
        create_invoice:
          (form.outbound_type === "sale" ||
            (form.outbound_type === "adjust" && form.adjust_dir === "up")) &&
          form.create_invoice,
        invoice: {
          number: form.voucher_no,
          e_invoice_no: form.e_invoice_no,
          e_invoice_symbol: form.e_invoice_symbol,
          e_invoice_date: toIsoDate(form.e_invoice_date),
        },
      }),
    );
    toast.add({
      severity: "success",
      summary: `Đã xuất kho ${form.voucher_no}`,
      detail: `Doanh thu: ${fmt(res.revenue)} đ • Lãi gộp: ${fmt(res.profit)} đ${
        res.pt_no ? ` • Đã tạo phiếu thu ${res.pt_no}` : ""
      }${
        res.invoice_no
          ? ` • Đã lập hóa đơn ${res.invoice_no} (${
              res.invoice_status === "official" ? "Đã liên kết HĐĐT" : "Nháp"
            })`
          : ""
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

async function reload() {
  await Promise.all([catalog.loadAll(), stock.loadEntries("PX")]);
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
          <i-mdi-upload class="text-xl text-orange-500" />
          <h3 class="font-semibold">Phiếu xuất kho / Bán hàng</h3>
        </div>
      </template>
      <template #end>
        <Button v-if="auth.canStock" label="Tạo phiếu xuất" icon="pi pi-plus" @click="openCreate" />
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
          actions-header="Thao tác"
          actions-width="132"
        >
          <Column field="voucher_no" header="Số phiếu">
            <template #body="{ data }">
              <Button
                :label="data.voucher_no"
                link
                class="!p-0 text-sm"
                :aria-label="`Xem phiếu ${data.voucher_no}`"
                v-tooltip="'Xem phiếu'"
                @click="openVoucher(data.voucher_no)"
              />
            </template>
          </Column>
          <Column field="posting_date" header="Ngày" />
          <Column field="product_code" header="Mã SP" />
          <Column field="description" header="Diễn giải" />
          <Column field="customer_name" header="Khách hàng">
            <template #body="{ data }">{{ data.customer_name || "—" }}</template>
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
            <div class="flex justify-center gap-1">
              <Button
                icon="pi pi-eye"
                text
                rounded
                size="small"
                aria-label="Xem phiếu"
                v-tooltip="'Xem phiếu'"
                @click="openVoucher(data.voucher_no)"
              />
              <Button
                v-if="!data.adjust_code"
                icon="pi pi-undo"
                text
                rounded
                size="small"
                aria-label="Lập phiếu điều chỉnh (khách trả lại)"
                v-tooltip="'Lập phiếu điều chỉnh (khách trả lại)'"
                @click="adjustFromVoucher(data)"
              />
              <Button
                icon="pi pi-print"
                text
                rounded
                size="small"
                aria-label="In phiếu"
                v-tooltip="'In phiếu'"
                @click="printVoucher(data)"
              />
            </div>
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
      width="max-w-4xl"
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
        <FormField v-if="form.outbound_type === 'adjust'" label="Hướng điều chỉnh">
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
        <FormField v-if="form.outbound_type === 'sale'" label="Thu tiền ngay">
          <div class="flex h-full items-center gap-1.5">
            <ToggleSwitch v-model="form.receive_now" class="shrink-0" />
            <i
              class="pi pi-info-circle cursor-help text-xs text-gray-400 shrink-0"
              v-tooltip="
                'Bật: khi lưu sẽ tự tạo phiếu thu (PT) thu tiền của khách hàng — cần chọn Khách hàng.'
              "
              aria-hidden="true"
            />
          </div>
        </FormField>
      </div>

      <!-- Hóa đơn bán hàng lập KÈM phiếu xuất (tab "Hóa đơn") -->
      <div
        v-if="
          form.outbound_type === 'sale' ||
          (form.outbound_type === 'adjust' && form.adjust_dir === 'up')
        "
        class="mt-3 flex flex-col gap-3 rounded-lg border border-dashed border-gray-300 p-3"
      >
        <label class="flex items-center gap-2 text-sm">
          <ToggleSwitch v-model="form.create_invoice" class="shrink-0" />
          <span class="font-medium">Lập kèm hóa đơn bán hàng (tab Hóa đơn)</span>
          <i
            class="pi pi-info-circle cursor-help text-xs text-gray-400 shrink-0"
            v-tooltip="
              'Tự tạo 1 hóa đơn trong tab Hóa đơn với số hóa đơn = số phiếu xuất (liên kết 1-1, không lệch sổ). Khai trước Số HĐĐT + Ký hiệu + Ngày thì hóa đơn chuyển ngay thành Đã liên kết HĐĐT; để trống thì chỉ là hóa đơn nháp.'
            "
            aria-hidden="true"
          />
        </label>
        <div v-if="form.create_invoice" class="grid grid-cols-4 gap-3">
          <FormField label="Số hóa đơn">
            <InputText :model-value="form.voucher_no" disabled size="small" />
          </FormField>
          <FormField label="Số HĐĐT">
            <InputText
              v-model="form.e_invoice_no"
              placeholder="trống = hóa đơn nháp"
              size="small"
            />
          </FormField>
          <FormField label="Ký hiệu HĐĐT">
            <InputText v-model="form.e_invoice_symbol" placeholder="VD: 1C24TT152" size="small" />
          </FormField>
          <FormField label="Ngày HĐĐT">
            <DatePicker
              v-model="form.e_invoice_date"
              dateFormat="dd/mm/yy"
              showClear
              size="small"
              class="w-full"
            />
          </FormField>
        </div>
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

    <!-- Xem phiếu (chỉ đọc) — dùng chung cho cả PX/PN -->
    <VoucherViewDialog v-model:visible="viewVoucherVisible" :voucher-no="viewVoucherNo" />

    <!-- Thêm khách hàng nhanh — dùng chung dialog chuẩn (tự sinh mã + tra cứu MST) -->
    <PartnerDialog
      v-model:visible="customerDialog"
      kind="customer"
      :show-action="auth.canStock"
      @saved="onCustomerSaved"
    />
  </div>
</template>
