<script setup lang="ts">
import { storeToRefs } from "pinia";
import { api } from "@/db";
import { useAuthStore } from "@/stores/auth";
import { useCatalogStore } from "@/stores/catalog";
import { useInvoiceStore } from "@/stores/invoice";
import type { Invoice, InvoiceItem } from "@/types";
import { fmtInt as fmt } from "@/utils/format";

const auth = useAuthStore();
const catalog = useCatalogStore();
const invoiceStore = useInvoiceStore();
const { products, customers } = storeToRefs(catalog);
const { invoices, detail, loading } = storeToRefs(invoiceStore);
const toast = useToast();

const draftDialog = ref(false);
const detailDialog = ref(false);
const saving = ref(false);

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
  items: [] as { product_code: string; quantity: number; unit_price: number }[],
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
  form.items.push({ product_code: "", quantity: 1, unit_price: 0 });
}

function removeRow(i: number) {
  form.items.splice(i, 1);
}

const statusBadge = {
  draft: "secondary" as const,
  pasted: "info" as const,
  official: "success" as const,
};

async function save() {
  if (!form.items.length || !form.customer) {
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
        items: form.items.map((it) => ({ ...it })),
      }),
    );
    toast.add({
      severity: "success",
      summary: `Đã lập hóa đơn ${form.number}`,
      detail: `Tổng: ${fmt(res.total)} đ • Thuế GTGT: ${fmt(res.vat_amount)} đ`,
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
  linkForm.hddtDate = inv.e_invoice_date
    ? new Date(inv.e_invoice_date)
    : new Date();
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

onMounted(async () => {
  await Promise.all([catalog.loadAll(), invoiceStore.loadInvoices()]);
});
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
          actions-width="170"
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
            <template #body="{ data }">{{ fmt(data.total) }} đ</template>
          </Column>
          <Column field="vat_amount" header="Thuế GTGT" align="right">
            <template #body="{ data }">{{ fmt(data.vat_amount) }} đ</template>
          </Column>
          <Column header="Trạng thái">
            <template #body="{ data }">
              <Tag
                :value="
                  data.status === 'draft'
                    ? 'Nháp'
                    : data.status === 'pasted'
                      ? 'Đã dán'
                      : 'Đã liên kết HĐĐT'
                "
                :severity="
                  statusBadge[data.status as keyof typeof statusBadge] ??
                  'secondary'
                "
              />
            </template>
          </Column>
          <Column header="Số HĐĐT">
            <template #body="{ data }">{{ data.e_invoice_no || "—" }}</template>
          </Column>
          <Column header="Ký hiệu">
            <template #body="{ data }">{{
              data.e_invoice_symbol || "—"
            }}</template>
          </Column>
          <template #actions="{ data }">
            <Button
              v-if="auth.canAccounting && data.status === 'draft'"
              icon="pi pi-link"
              label="Liên kết HĐĐT"
              text
              size="small"
              severity="success"
              @click="openLinkHddt(data)"
            />
            <Tag
              v-else-if="data.status === 'official'"
              value="Đã liên kết"
              severity="success"
              icon="pi pi-check"
            />
            <span v-else class="text-sm text-gray-400">—</span>
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
      width="max-w-3xl"
      action-label="Lập hóa đơn"
      :saving="saving"
      :show-action="auth.canAccounting"
      @action="save"
    >
      <div class="grid grid-cols-3 gap-4 py-2">
        <FormField label="Số hóa đơn">
          <InputText v-model="form.number" disabled />
        </FormField>
        <FormField label="Ngày lập">
          <DatePicker
            v-model="form.date"
            dateFormat="dd/mm/yy"
            class="w-full"
          />
        </FormField>
        <FormField label="MST khách hàng">
          <InputText
            v-model="form.customer_tax_code"
            placeholder="Mã số thuế"
          />
        </FormField>
        <FormField label="Khách hàng" required class="col-span-3">
          <Select
            v-model="form.customer"
            :options="customers"
            optionLabel="name"
            optionValue="name"
            :editable="true"
            filter
            class="w-full"
            placeholder="Chọn hoặc nhập tên khách hàng"
            @change="
              (e: any) => {
                const c = customers.find((x) => x.name === e.value) as any;
                if (c?.tax_code) form.customer_tax_code = (c as any).tax_code;
              }
            "
          />
        </FormField>
      </div>

      <LineItemsEditor
        :items="form.items"
        :products="products"
        :can-edit="auth.canAccounting"
        price-field="sale_price"
        info="Kiểm tra tồn kho trước khi lưu."
        total-label="Tổng tiền (chưa thuế):"
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
          <div>
            <span class="text-gray-500">Ngày:</span> {{ detail.invoice.date }}
          </div>
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
          <Column field="unit" header="ĐVT" />
          <Column field="quantity" header="SL" align="right" />
          <Column field="unit_price" header="Đơn giá" align="right">
            <template #body="{ data }">{{ fmt(data.unit_price) }}</template>
          </Column>
          <Column field="subtotal" header="Thành tiền" align="right">
            <template #body="{ data }">{{ fmt(data.subtotal) }}</template>
          </Column>
        </AppDataTable>
        <div class="mt-4 flex flex-col items-end gap-1 text-sm">
          <div class="flex gap-6">
            <span class="text-gray-500">Cộng tiền hàng:</span
            ><b>{{ fmt(detail.invoice.total) }} đ</b>
          </div>
          <div class="flex gap-6">
            <span class="text-gray-500">Thuế GTGT:</span
            ><b>{{ fmt(detail.invoice.vat_amount) }} đ</b>
          </div>
          <div class="flex gap-6 text-base font-bold text-primary-600">
            <span>Tổng cộng:</span
            ><span
              >{{
                fmt(detail.invoice.total + detail.invoice.vat_amount)
              }}
              đ</span
            >
          </div>
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
          Hóa đơn <b>{{ linkTarget.number }}</b> —
          {{ linkTarget.customer || "—" }}. Sau khi lưu, chuyển sang trạng thái
          <b>Đã liên kết HĐĐT</b>.
        </p>
        <FormField label="Số HĐĐT" required>
          <InputText
            v-model="linkForm.hddtNo"
            placeholder="Nhập số hóa đơn điện tử"
            class="w-full"
          />
        </FormField>
        <FormField label="Ký hiệu HĐĐT">
          <InputText
            v-model="linkForm.hddtSymbol"
            placeholder="Nhập ký hiệu (VD: 1C24TT152)"
            class="w-full"
          />
        </FormField>
        <FormField label="Ngày HĐĐT">
          <DatePicker
            v-model="linkForm.hddtDate"
            dateFormat="dd/mm/yy"
            class="w-full"
          />
        </FormField>
      </div>
    </AppDialog>
  </div>
</template>
