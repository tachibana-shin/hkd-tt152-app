<script setup lang="ts">
import { storeToRefs } from "pinia";
import { api } from "@/db";
import { useAuthStore } from "@/stores/auth";
import { useCatalogStore } from "@/stores/catalog";
import { useInvoiceStore } from "@/stores/invoice";
import type { Invoice, InvoiceItem } from "@/types";
import { fmtInt as fmt, fmtVnd } from "@/utils/format";

const auth = useAuthStore();
const catalog = useCatalogStore();
const invoiceStore = useInvoiceStore();
const { products, customers, industryGroups, warehouses } = storeToRefs(catalog);
const { invoices, detail, loading } = storeToRefs(invoiceStore);
const toast = useToast();

const draftDialog = ref(false);
const detailDialog = ref(false);
const saving = ref(false);
const confirm = useConfirm();

// ─── Thêm khách hàng nhanh: dùng chung PartnerDialog (tự sinh mã + tra cứu MST) ───
const customerDialog = ref(false);

function onCustomerSaved(code: string) {
  const c = catalog.customers.find((x) => x.code === code);
  if (c) {
    form.customer = c.name;
    if (c.tax_code) form.customer_tax_code = c.tax_code;
  }
}

const linkDialog = ref(false);
const linking = ref(false);
const linkTarget = ref<Invoice | null>(null);
const linkForm = reactive({
  hddtNo: "",
  hddtSymbol: "",
  hddtDate: new Date(),
});

const form = reactive({
  number: "",
  date: new Date(),
  customer: "",
  customer_tax_code: "",
  items: [] as {
    product_code: string;
    quantity: number;
    unit_price: number;
    industry_code?: string;
    discount?: number;
    warehouse_code?: string;
  }[],
});

const iso = (d: Date | null) => (d ? d.toISOString().slice(0, 10) : "");

function nextInvoiceNo() {
  const nums = invoices.value
    .map((i) => parseInt(i.number.replace(/[^\d]/g, ""), 10))
    .filter((n) => !isNaN(n));
  const next = nums.length ? Math.max(...nums) + 1 : 1;
  form.number = `HD${String(next).padStart(4, "0")}`;
}

function openCreate() {
  Object.assign(form, {
    date: new Date(),
    customer: "",
    customer_tax_code: "",
    items: [],
  });
  nextInvoiceNo();
  draftDialog.value = true;
}

function addRow() {
  form.items.push({
    product_code: "",
    quantity: 1,
    unit_price: 0,
    industry_code: "",
    discount: 0,
    warehouse_code: "",
  });
}

function removeRow(i: number) {
  form.items.splice(i, 1);
}

const statusBadge: Record<string, "secondary" | "info" | "success" | "danger" | "warn"> = {
  draft: "secondary",
  pasted: "info",
  exported: "info",
  official: "success",
  cancelled: "danger",
  adjusted: "warn",
};
const statusText: Record<string, string> = {
  draft: "Nháp",
  pasted: "Đã dán",
  exported: "Đã chép — chờ phát hành",
  official: "Đã phát hành",
  cancelled: "Đã hủy",
  adjusted: "Đã bị sửa bên kia",
};

// ─── Chép sang dịch vụ HĐĐT khác (kiểm tra trước + bản chốt) ───
const exportVisible = ref(false);
const exportTarget = ref<Invoice | null>(null);

function openExport(inv: Invoice) {
  exportTarget.value = inv;
  exportVisible.value = true;
}

// ─── Ghi nhận hủy / bị sửa bên kia ───
const statusVisible = ref(false);
const statusMode = ref<"cancelled" | "adjusted">("cancelled");
const statusTarget = ref<Invoice | null>(null);

function openStatus(inv: Invoice, mode: "cancelled" | "adjusted") {
  statusTarget.value = inv;
  statusMode.value = mode;
  statusVisible.value = true;
}

// ─── Xoá hóa đơn nháp ───
// Hóa đơn bán không trừ kho/không ghi bút toán nên xoá nháp không ảnh hưởng sổ
// sách; các trạng thái sau nháp (đã chép, đã phát hành) đi luồng hủy/điều chỉnh.
function removeInvoice(inv: Invoice) {
  confirm.require({
    header: "Xoá hóa đơn nháp",
    message: `Xoá hóa đơn nháp ${inv.number} (${fmtVnd(inv.total)})? Chỉ xoá được khi hóa đơn còn ở trạng thái nháp.`,
    icon: "pi pi-exclamation-triangle",
    acceptLabel: "Xoá",
    rejectLabel: "Hủy",
    accept: async () => {
      try {
        await invoiceStore.deleteInvoice(inv.id);
        toast.add({
          severity: "success",
          summary: "Đã xoá hóa đơn nháp",
          detail: inv.number,
        });
      } catch (e) {
        toast.add({ severity: "error", summary: "Không xoá được", detail: String(e) });
      }
    },
  });
}

async function afterStatusChange() {
  await invoiceStore.loadInvoices();
  if (statusTarget.value) {
    const fresh = invoices.value.find((i) => i.id === statusTarget.value?.id);
    if (fresh) statusTarget.value = fresh;
  }
}

async function save() {
  // Kiểu nhập liên tục (pre-input) luôn để lại dòng trống cuối → bỏ dòng chưa chọn hàng.
  const rows = form.items.filter((it) => it.product_code);
  if (!rows.length || !form.customer) {
    toast.add({
      severity: "warn",
      summary: "Thiếu thông tin",
      detail: "Cần khách hàng và ít nhất 1 mặt hàng",
    });
    return;
  }
  saving.value = true;
  try {
    const res = JSON.parse(
      await invoiceStore.saveDraft({
        number: form.number,
        date: iso(form.date),
        customer: form.customer,
        customer_tax_code: form.customer_tax_code,
        items: rows.map((it) => ({ ...it })),
      }),
    );
    toast.add({
      severity: "success",
      summary: `Đã lập hóa đơn ${form.number}`,
      detail: `Tổng tiền: ${fmt(res.total)} đ • Thuế phải nộp (tỷ lệ nhóm ngành): ${fmt(res.tax_payable ?? 0)} đ`,
    });
    draftDialog.value = false;
  } catch (e) {
    toast.add({
      severity: "error",
      summary: "Không lập được hóa đơn",
      detail: String(e),
    });
  } finally {
    saving.value = false;
  }
}

async function showDetail(inv: Invoice) {
  await invoiceStore.loadDetail(inv.id);
  detailDialog.value = true;
}

function openLinkHddt(inv: Invoice) {
  linkTarget.value = inv;
  linkForm.hddtNo = inv.e_invoice_no ?? "";
  linkForm.hddtSymbol = inv.e_invoice_symbol ?? "";
  linkForm.hddtDate = inv.e_invoice_date ? new Date(inv.e_invoice_date) : new Date();
  linkDialog.value = true;
}

async function saveLink() {
  if (!linkTarget.value) return;
  const hddtNo = linkForm.hddtNo.trim();
  if (!hddtNo) {
    toast.add({
      severity: "warn",
      summary: "Thiếu Số HĐĐT",
      detail: "Vui lòng nhập Số hóa đơn điện tử trước khi lưu liên kết",
    });
    return;
  }
  linking.value = true;
  try {
    await api.linkHddt({
      invoiceId: linkTarget.value.id,
      hddtNo,
      hddtSymbol: linkForm.hddtSymbol.trim(),
      hddtDate: iso(linkForm.hddtDate),
    });
    toast.add({
      severity: "success",
      summary: "Đã liên kết HĐĐT",
      detail: `Hóa đơn ${linkTarget.value.number} chuyển sang trạng thái Đã liên kết HĐĐT`,
    });
    linkDialog.value = false;
    await invoiceStore.loadInvoices();
  } catch (e) {
    toast.add({
      severity: "error",
      summary: "Liên kết HĐĐT thất bại",
      detail: String(e),
    });
  } finally {
    linking.value = false;
  }
}

void (async () => {
  await Promise.all([catalog.loadAll(), invoiceStore.loadInvoices()]);
})();
</script>

<template>
  <div class="space-y-4">
    <Toolbar>
      <template #start>
        <div class="flex items-center gap-2">
          <i-mdi-file-document-outline class="text-xl text-violet-500" />
          <h3 class="font-semibold">Hóa đơn</h3>
        </div>
      </template>
      <template #end>
        <Button
          v-if="auth.canAccounting"
          label="Lập hóa đơn nháp"
          icon="pi pi-plus"
          @click="openCreate"
        />
      </template>
    </Toolbar>

    <Card>
      <template #content>
        <AppDataTable
          :value="invoices"
          :loading="loading"
          stripedRows
          paginator
          :rows="10"
          actions-header="Hành động"
          actions-width="190"
        >
          <Column field="number" header="Số HĐ">
            <template #body="{ data }">
              <Button
                :label="data.number"
                icon="pi pi-eye"
                text
                size="small"
                @click="showDetail(data)"
              />
            </template>
          </Column>
          <Column field="date" header="Ngày" />
          <Column field="customer" header="Khách hàng" />
          <Column field="customer_tax_code" header="MST" />
          <Column field="total" header="Tổng tiền" align="right">
            <template #body="{ data }">{{ fmtVnd(data.total) }}</template>
          </Column>
          <Column header="Trạng thái">
            <template #body="{ data }">
              <div class="flex flex-col items-start gap-1">
                <Tag
                  :value="statusText[data.status] ?? data.status"
                  :severity="statusBadge[data.status] ?? 'secondary'"
                />
                <span
                  v-if="data.exported_at"
                  class="text-xs text-gray-400"
                  :title="`Đã chép sang bên kia lúc ${data.exported_at}`"
                >
                  đã chép {{ data.exported_at.slice(0, 16) }}
                </span>
              </div>
            </template>
          </Column>
          <Column header="Số HĐĐT">
            <template #body="{ data }">{{ data.e_invoice_no || "—" }}</template>
          </Column>
          <Column header="Ký hiệu">
            <template #body="{ data }">{{ data.e_invoice_symbol || "—" }}</template>
          </Column>
          <Column field="voucher_no" header="Phiếu xuất">
            <template #body="{ data }">{{ data.voucher_no || "—" }}</template>
          </Column>
          <template #actions="{ data }">
            <div v-if="auth.canAccounting" class="flex items-center justify-center gap-1">
              <!-- Hủy rồi thì không chép/liên kết được nữa -->
              <template v-if="data.status !== 'cancelled'">
                <Button
                  icon="pi pi-copy"
                  text
                  rounded
                  size="small"
                  aria-label="Chép sang dịch vụ HĐĐT khác"
                  v-tooltip="'Chép sang dịch vụ HĐĐT khác'"
                  @click="openExport(data)"
                />
                <Button
                  v-if="!data.e_invoice_no"
                  icon="pi pi-link"
                  text
                  rounded
                  size="small"
                  aria-label="Nhập số HĐĐT đã phát hành"
                  v-tooltip="'Nhập số HĐĐT đã phát hành'"
                  severity="success"
                  @click="openLinkHddt(data)"
                />
                <Button
                  v-if="data.e_invoice_no || data.status === 'exported'"
                  icon="pi pi-ban"
                  text
                  rounded
                  size="small"
                  aria-label="Ghi nhận HĐ đã hủy bên kia"
                  v-tooltip="'Bên kia đã hủy hóa đơn này'"
                  severity="danger"
                  @click="openStatus(data, 'cancelled')"
                />
                <Button
                  v-if="data.e_invoice_no"
                  icon="pi pi-pencil"
                  text
                  rounded
                  size="small"
                  aria-label="Ghi nhận HĐ bị sửa bên kia"
                  v-tooltip="'Bên kia đã sửa hóa đơn này'"
                  severity="warn"
                  @click="openStatus(data, 'adjusted')"
                />
                <!-- Nháp thì xoá được; các trạng thái sau nháp giữ lại dấu vết -->
                <Button
                  v-if="data.status === 'draft'"
                  icon="pi pi-trash"
                  text
                  rounded
                  size="small"
                  aria-label="Xoá hóa đơn nháp"
                  v-tooltip="'Xoá hóa đơn nháp'"
                  severity="danger"
                  @click="removeInvoice(data)"
                />
              </template>
              <span v-else class="text-xs text-gray-500" :title="data.cancel_reason">
                {{ data.cancel_reason || "Đã hủy" }}
              </span>
            </div>
          </template>
          <template #empty
            ><EmptyState text="Chưa có hóa đơn nào." icon="pi pi-receipt"
          /></template>
        </AppDataTable>
      </template>
    </Card>

    <!-- Dialog lập hóa đơn nháp -->
    <AppDialog
      v-model:visible="draftDialog"
      header="Lập hóa đơn bán hàng (nháp)"
      width="max-w-6xl"
      action-label="Lập hóa đơn"
      :saving="saving"
      :show-action="auth.canAccounting"
      @action="save"
    >
      <div class="grid grid-cols-3 gap-4 py-2">
        <FormField label="Số hóa đơn">
          <InputText v-model="form.number" disabled size="small" />
        </FormField>
        <FormField label="Ngày lập">
          <DatePicker v-model="form.date" dateFormat="dd/mm/yy" size="small" class="w-full" />
        </FormField>
        <FormField label="MST khách hàng">
          <InputText v-model="form.customer_tax_code" placeholder="Mã số thuế" size="small" />
        </FormField>
        <FormField label="Khách hàng" required class="col-span-3">
          <div class="flex gap-2">
            <Select
              v-model="form.customer"
              :options="customers"
              optionLabel="name"
              optionValue="name"
              :editable="true"
              filter
              size="small"
              class="w-full"
              placeholder="Chọn hoặc nhập tên khách hàng"
              @change="
                (e: any) => {
                  const c = customers.find((x) => x.name === e.value) as any;
                  if (c?.tax_code) form.customer_tax_code = (c as any).tax_code;
                }
              "
            />
            <Button
              v-if="auth.canAccounting"
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
      </div>

      <LineItemsEditor
        :items="form.items"
        :products="products"
        :industry-groups="industryGroups"
        :warehouses="warehouses"
        :can-edit="auth.canAccounting"
        price-field="sale_price"
        show-industry
        show-amount
        amount-input
        show-discount
        show-unit
        show-warehouse
        compact
        show-add-product
        info="Nhập thẳng Thành tiền từng dòng (Đơn giá tự tính = Thành tiền ÷ SL, đổi SL được). Nhóm ngành tự lấy theo sản phẩm (đổi được trên dòng). Tiền CK trừ vào giá trị dòng (Thành tiền − CK). Hàng hóa phải có đủ tồn kho theo đúng kho xuất trên dòng; sản phẩm dịch vụ (nhân công...) không cần tồn kho."
        total-label="Tổng tiền:"
        @add="addRow"
        @remove="removeRow"
      />
    </AppDialog>

    <!-- Dialog chi tiết hóa đơn -->
    <AppDialog
      v-model:visible="detailDialog"
      header="Chi tiết hóa đơn"
      width="max-w-3xl"
      cancel-label="Đóng"
      :show-action="false"
    >
      <template v-if="detail">
        <div class="grid grid-cols-2 gap-3 text-sm mb-4">
          <div>
            <span class="text-gray-500">Số HĐ:</span>
            <b>{{ detail.invoice.number }}</b>
          </div>
          <div><span class="text-gray-500">Ngày:</span> {{ detail.invoice.date }}</div>
          <div>
            <span class="text-gray-500">Khách hàng:</span>
            {{ detail.invoice.customer }}
          </div>
          <div>
            <span class="text-gray-500">MST:</span>
            {{ detail.invoice.customer_tax_code || "—" }}
          </div>
        </div>
        <AppDataTable :value="detail.items as InvoiceItem[]" stripedRows>
          <Column field="product_code" header="Mã SP" />
          <Column field="product_name" header="Tên sản phẩm" />
          <Column field="industry_code" header="Nhóm ngành">
            <template #body="{ data }">{{
              industryGroups.find((g) => g.code === data.industry_code)?.name ??
              (data.industry_code || "—")
            }}</template>
          </Column>
          <Column field="unit" header="ĐVT" />
          <Column field="quantity" header="SL" align="right" />
          <Column field="unit_price" header="Đơn giá" align="right">
            <template #body="{ data }">{{ fmtVnd(data.unit_price) }}</template>
          </Column>
          <Column header="Thành tiền" align="right">
            <template #body="{ data }">{{
              fmtVnd((data as InvoiceItem).quantity * (data as InvoiceItem).unit_price)
            }}</template>
          </Column>
          <Column header="Tiền CK" align="right">
            <template #body="{ data }">{{
              (data as InvoiceItem).discount ? fmtVnd((data as InvoiceItem).discount ?? 0) : "—"
            }}</template>
          </Column>
          <Column header="Giá trị dòng" align="right">
            <template #body="{ data }">{{ fmtVnd((data as InvoiceItem).subtotal) }}</template>
          </Column>
        </AppDataTable>
        <div class="mt-4 flex flex-col items-end gap-1 text-sm">
          <div class="flex gap-6">
            <span class="text-gray-500">Cộng tiền hàng:</span
            ><b>{{ fmt(detail.invoice.total) }} đ</b>
          </div>
          <div class="flex gap-6">
            <span class="text-gray-500">Thuế GTGT phải nộp (theo tỷ lệ nhóm ngành):</span
            ><b
              >{{
                fmt(
                  (detail.items as InvoiceItem[]).reduce(
                    (s, it) => s + it.subtotal * (it.vat_rate ?? 0),
                    0,
                  ),
                )
              }}
              đ</b
            >
          </div>
          <div class="flex gap-6 text-base font-bold text-primary-600">
            <span>Tổng tiền thanh toán:</span><span>{{ fmt(detail.invoice.total) }} đ</span>
          </div>
          <p class="text-xs text-gray-400">
            Hóa đơn bán hàng của HKD không tách thuế GTGT — thuế nộp theo tỷ lệ nhóm ngành trên
            doanh thu (báo cáo thuế).
          </p>
        </div>
      </template>
    </AppDialog>

    <!-- Dialog liên kết HĐĐT -->
    <AppDialog
      v-model:visible="linkDialog"
      header="Liên kết hóa đơn điện tử (HĐĐT)"
      width="max-w-md"
      action-label="Lưu liên kết"
      action-icon="pi pi-link"
      :saving="linking"
      :show-action="auth.canAccounting"
      @action="saveLink"
    >
      <div class="flex flex-col gap-4 py-2">
        <p v-if="linkTarget" class="text-sm text-gray-600">
          Hóa đơn <b>{{ linkTarget.number }}</b> — {{ linkTarget.customer || "—" }}. Sau khi lưu,
          chuyển sang trạng thái <b>Đã liên kết HĐĐT</b>.
        </p>
        <FormField label="Số HĐĐT" required input-id="link-hddt-no">
          <InputText
            :id="'link-hddt-no'"
            v-model="linkForm.hddtNo"
            placeholder="Nhập số hóa đơn điện tử"
            class="w-full"
          />
        </FormField>
        <FormField label="Ký hiệu HĐĐT" input-id="link-hddt-symbol">
          <InputText
            :id="'link-hddt-symbol'"
            v-model="linkForm.hddtSymbol"
            placeholder="Nhập ký hiệu (VD: 1C24TT152)"
            class="w-full"
          />
        </FormField>
        <FormField label="Ngày HĐĐT">
          <DatePicker v-model="linkForm.hddtDate" dateFormat="dd/mm/yy" class="w-full" />
        </FormField>
      </div>
    </AppDialog>

    <!-- Thêm khách hàng nhanh — dùng chung dialog chuẩn (tự sinh mã + tra cứu MST) -->
    <PartnerDialog
      v-model:visible="customerDialog"
      kind="customer"
      :show-action="auth.canAccounting"
      @saved="onCustomerSaved"
    />

    <!-- Chép hóa đơn sang dịch vụ HĐĐT khác: kiểm tra trước + lưu bản chốt -->
    <InvoiceExportDialog
      v-model:visible="exportVisible"
      :invoice="exportTarget"
      @done="invoiceStore.loadInvoices()"
    />

    <!-- Ghi nhận hủy / bị sửa bên kia -->
    <InvoiceStatusDialog
      v-model:visible="statusVisible"
      :invoice="statusTarget"
      :mode="statusMode"
      @done="afterStatusChange"
    />
  </div>
</template>
