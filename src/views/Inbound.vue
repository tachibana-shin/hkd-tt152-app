<script setup lang="ts">
import { storeToRefs } from "pinia";
import { useAuthStore } from "@/stores/auth";
import { useCatalogStore } from "@/stores/catalog";
import { useSettingsStore } from "@/stores/settings";
import { useStockStore } from "@/stores/stock";
import PartnerDialog from "@/components/PartnerDialog.vue";
import { api } from "@/db";
import { fmtInt as fmt, fmtVnd } from "@/utils/format";
import type { Account } from "@/types";

const catalog = useCatalogStore();
const stock = useStockStore();
const auth = useAuthStore();
const settings = useSettingsStore();
const { products, suppliers, warehouses } = storeToRefs(catalog);
const { entries, loading } = storeToRefs(stock);
const toast = useToast();
const router = useRouter();

// Danh mục tài khoản (DMTK) — chọn TK Nợ / TK Có theo loại nhập.
const accounts = ref<Account[]>([]);
const accountOptions = computed(() =>
  accounts.value.map((a) => ({
    code: a.code,
    label: `${a.code} — ${a.name}`,
  })),
);

// Loại phiếu nhập kho (TT 88/2021/TT-BTC — mẫu 03-VT): Phiếu nhập kho áp dụng cho
// MỌI trường hợp hàng vào kho — mua ngoài (hóa đơn/bảng kê), tự sản xuất, gia công,
// nhập khác (thừa kiểm kê...) — không chỉ riêng mua hàng.
const typeOptions = [
  { label: "Mua hàng ngoài", value: "purchase" },
  { label: "Tự sản xuất / gia công", value: "production" },
  { label: "Nhập khác", value: "other" },
  { label: "Điều chỉnh hóa đơn mua", value: "adjust" },
];

// "Hướng điều chỉnh" chỉ xuất hiện với loại "adjust": bên bán trừ bớt (down —
// trả lại NCC, giảm tồn + giảm phải trả) hoặc bên bán thêm (up — nhập như thường).
const adjustDirOptions = [
  { label: "Tăng (bên bán thêm)", value: "up" },
  { label: "Giảm (trả lại NCC)", value: "down" },
];

// Gợi ý nội dung ô "Theo chứng từ" (cột "Theo..." của mẫu 03-VT) theo loại nhập.
const referencePlaceholder = computed(() =>
  form.inbound_type === "production"
    ? "Số lệnh nhập kho / lệnh sản xuất (VD: Lệnh SX 015)"
    : form.inbound_type === "other"
      ? "Số chứng từ nhập khác (VD: Biên bản kiểm kê 01)"
      : form.inbound_type === "adjust"
        ? "Số hóa đơn / chứng từ điều chỉnh (VD: HĐĐC 01 ngày 20/01/2026)"
        : "Số hóa đơn / bảng kê (VD: HĐ 170 ngày 16/01/2026)",
);

// Đổi TK mặc định theo loại nhập (vẫn sửa tay được):
//   purchase  → Nợ 152 hàng hóa / Có 331 phải trả người bán
//   production→ Nợ 155 thành phẩm / Có 154 chi phí SXKD dở dang
//   other     → Nợ 152 / Có 154 (người dùng tự chọn theo nghiệp vụ)
//   adjust    → Nợ 152 / Có 331 (với hướng "Giảm" backend sẽ đảo ngược:
//               Nợ TK đối ứng / Có 152 giảm hàng trả lại).
function onTypeChange() {
  if (form.inbound_type === "production") {
    form.debit_account = "155";
    form.credit_account = "154";
  } else if (form.inbound_type === "other") {
    form.debit_account = "152";
    form.credit_account = "154";
  } else {
    form.debit_account = "152";
    form.credit_account = "331";
  }
}

function printVoucher(row: { voucher_no: string }) {
  router.push("/print/" + encodeURIComponent(row.voucher_no));
}

// Lập phiếu điều chỉnh (trả lại NCC) nhanh từ 1 phiếu nhập gốc: tự điền nhà cung
// cấp + từng mặt hàng (SL, đơn giá) theo đúng chứng từ đang xem.
async function adjustFromVoucher(row: { voucher_no: string }) {
  try {
    const voucher = await api.getVoucher(row.voucher_no);
    const lines = voucher.filter((r) => r.entry_type === "PN" && r.product_code && r.quantity > 0);
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
      description: `Trả lại NCC — theo ${row.voucher_no}`,
      supplier_code: voucher[0]?.supplier_code ?? "",
      note: "",
      items: lines.map((l) => ({
        product_code: l.product_code,
        quantity: l.quantity,
        unit_price: l.unit_price,
        discount: 0,
      })),
      inbound_type: "adjust",
      adjust_dir: "down",
      reference_no: row.voucher_no,
      vat_rate: 0,
      debit_account: "152",
      credit_account: "331",
      pay_now: false,
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
  voucher_no: "PN001",
  description: "",
  supplier_code: "",
  warehouse_code: "",
  note: "",
  items: [] as {
    product_code: string;
    quantity: number;
    unit_price: number;
    discount: number; // số tiền CK (đ)
  }[],
  inbound_type: "purchase" as "purchase" | "production" | "other" | "adjust",
  reference_no: "",
  vat_rate: 0,
  debit_account: "152",
  credit_account: "331",
  // Hướng điều chỉnh hóa đơn mua (chỉ dùng khi loại = "adjust"):
  //   up   = bên bán thêm → nhập kho như phiếu thường
  //   down = bên bán trừ bớt → trả lại NCC, giảm tồn + giảm phải trả
  adjust_dir: "up" as "up" | "down",
  // Trả tiền ngay: bật mặc định — khi lưu tự tạo phiếu chi (PC) thanh toán cho NCC.
  pay_now: true,
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
    inbound_type: "purchase",
    reference_no: "",
    vat_rate: 0,
    debit_account: "152",
    credit_account: "331",
    adjust_dir: "up",
    pay_now: true,
  });
  nextVoucherNo();
  dialog.value = true;
}

function addRow() {
  form.items.push({ product_code: "", quantity: 1, unit_price: 0, discount: 0 });
}

function removeRow(i: number) {
  form.items.splice(i, 1);
}

// ─── Thêm nhà cung cấp nhanh: dùng chung PartnerDialog (tự sinh mã + tra cứu MST) ───
const supplierDialog = ref(false);

function onSupplierSaved(code: string) {
  form.supplier_code = code;
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
        items: rows.map((it) => ({ ...it })),
        inbound_type: form.inbound_type,
        reference_no: form.reference_no,
        // % → tỷ lệ (0-1); backend tự bỏ qua thuế nếu hộ chưa bật khấu trừ GTGT.
        vat_rate: form.vat_rate / 100,
        debit_account: form.debit_account,
        credit_account: form.credit_account,
        // Trả tiền ngay: backend tự tạo + liên kết phiếu chi (PC) cho nhà cung cấp.
        pay_now: form.pay_now,
        adjust_dir: form.adjust_dir,
      }),
    );
    toast.add({
      severity: "success",
      summary: `Đã nhập kho ${form.voucher_no}`,
      detail: `Giá trị nhập kho: ${fmt(res.total)} đ${
        res.vat ? ` · VAT khấu trừ: ${fmt(res.vat)} đ` : ""
      }${res.pc_no ? ` · Đã tạo phiếu chi ${res.pc_no}` : ""}`,
    });
    dialog.value = false;
  } catch (e) {
    toast.add({ severity: "error", summary: "Lỗi", detail: String(e) });
  } finally {
    saving.value = false;
  }
}

void (async () => {
  await Promise.all([catalog.loadAll(), stock.loadEntries("PN"), settings.load()]);
  accounts.value = await api.getAccounts();
})();
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
        <Button v-if="auth.canStock" label="Tạo phiếu nhập" icon="pi pi-plus" @click="openCreate" />
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
          actions-width="96"
        >
          <Column field="voucher_no" header="Số phiếu" />
          <Column field="posting_date" header="Ngày" />
          <Column field="product_code" header="Mã SP" />
          <Column field="description" header="Diễn giải" />
          <Column field="supplier_name" header="Nhà cung cấp">
            <template #body="{ data }">{{ data.supplier_name || "—" }}</template>
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
                v-if="!data.adjust_code"
                icon="pi pi-undo"
                text
                rounded
                size="small"
                aria-label="Lập phiếu điều chỉnh (trả lại NCC)"
                v-tooltip="'Lập phiếu điều chỉnh (trả lại NCC)'"
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
            ><EmptyState text="Chưa có phiếu nhập kho." icon="pi pi-download"
          /></template>
        </AppDataTable>
      </template>
    </Card>

    <AppDialog
      v-model:visible="dialog"
      :header="
        form.inbound_type === 'adjust' ? 'Tạo phiếu điều chỉnh hóa đơn mua' : 'Tạo phiếu nhập kho'
      "
      width="max-w-3xl"
      action-label="Lưu phiếu"
      :saving="saving"
      :show-action="auth.canStock"
      @action="save"
    >
      <div class="grid grid-cols-3 gap-4 py-2">
        <FormField label="Loại nhập">
          <Select
            v-model="form.inbound_type"
            :options="typeOptions"
            optionLabel="label"
            optionValue="value"
            size="small"
            class="w-full"
            @change="onTypeChange"
          />
        </FormField>
        <FormField v-if="form.inbound_type === 'adjust'" label="Hướng điều chỉnh">
          <Select
            v-model="form.adjust_dir"
            :options="adjustDirOptions"
            optionLabel="label"
            optionValue="value"
            size="small"
            class="w-full"
          />
        </FormField>
        <FormField label="Ngày nhập">
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
        <FormField label="Kho nhập">
          <Select
            v-model="form.warehouse_code"
            :options="warehouses"
            optionLabel="name"
            optionValue="code"
            size="small"
            class="w-full"
          />
        </FormField>
        <FormField label="Theo chứng từ" class="col-span-2">
          <InputText v-model="form.reference_no" :placeholder="referencePlaceholder" size="small" />
        </FormField>
        <FormField v-if="form.inbound_type === 'purchase'" label="Nhà cung cấp" class="col-span-2">
          <div class="flex gap-2">
            <Select
              v-model="form.supplier_code"
              :options="suppliers"
              optionLabel="name"
              optionValue="code"
              :editable="true"
              filter
              size="small"
              class="w-full"
              placeholder="Chọn hoặc nhập tên NCC"
            />
            <Button
              v-if="auth.canStock"
              icon="pi pi-plus"
              text
              rounded
              severity="secondary"
              aria-label="Thêm nhà cung cấp nhanh"
              v-tooltip="'Thêm nhà cung cấp nhanh'"
              @click="supplierDialog = true"
            />
          </div>
        </FormField>
        <FormField
          v-if="form.inbound_type === 'purchase' && settings.deductVat"
          label="Thuế GTGT đầu vào (%)"
        >
          <InputNumber
            v-model="form.vat_rate"
            :min="0"
            :max="30"
            suffix=" %"
            size="small"
            class="w-full"
          />
        </FormField>
        <FormField label="TK Nợ (kho)">
          <Select
            v-model="form.debit_account"
            :options="accountOptions"
            option-label="label"
            option-value="code"
            filter
            size="small"
            class="w-full"
          />
        </FormField>
        <FormField label="TK Có (đối ứng)">
          <Select
            v-model="form.credit_account"
            :options="accountOptions"
            option-label="label"
            option-value="code"
            filter
            size="small"
            class="w-full"
          />
        </FormField>
        <FormField label="Diễn giải">
          <InputText v-model="form.description" size="small" />
        </FormField>
        <FormField v-if="form.inbound_type === 'purchase'" label="Trả tiền ngay">
          <div class="flex h-full items-center gap-1.5">
            <ToggleSwitch v-model="form.pay_now" class="shrink-0" />
            <i
              class="pi pi-info-circle cursor-help text-xs text-gray-400 shrink-0"
              v-tooltip="
                'Bật: khi lưu sẽ tự tạo phiếu chi (PC) thanh toán cho nhà cung cấp — cần chọn Nhà cung cấp.'
              "
              aria-hidden="true"
            />
          </div>
        </FormField>
      </div>
      <p class="mt-1 text-xs text-gray-400">
        Mua hàng ngoài: Nợ 152 / Có 331 — nhập số tiền chiết khấu ở cột Tiền CK, giá trị nhập kho =
        Thành tiền − Tiền CK. Tự sản xuất, gia công: nhập kho thành phẩm theo lệnh sản xuất (Nợ 155
        / Có 154). Nhập khác: tùy chọn TK Nợ/Có (thừa kiểm kê, điều chỉnh…). Đơn giá nhập = giá sau
        chiết khấu, chưa thuế khi bật khấu trừ GTGT, ngược lại là giá đã gồm thuế. Điều chỉnh hóa
        đơn mua — Giảm: trả lại NCC, trừ lô FIFO theo ngày nhập, ghi Nợ TK đối ứng / Có TK hàng (kèm
        giảm thuế 133 nếu khấu trừ); Tăng: nhập kho như phiếu thường.
      </p>

      <LineItemsEditor
        :items="form.items"
        :products="products"
        :can-edit="auth.canStock"
        price-field="cost_price"
        show-amount
        compact
        show-add-product
        :show-discount="form.inbound_type === 'purchase'"
        :total-label="form.inbound_type === 'purchase' ? 'Giá trị nhập kho:' : 'Tổng tiền:'"
        @add="addRow"
        @remove="removeRow"
      />
    </AppDialog>

    <!-- Thêm nhà cung cấp nhanh — dùng chung dialog chuẩn (tự sinh mã + tra cứu MST) -->
    <PartnerDialog
      v-model:visible="supplierDialog"
      kind="supplier"
      :show-action="auth.canStock"
      @saved="onSupplierSaved"
    />
  </div>
</template>
