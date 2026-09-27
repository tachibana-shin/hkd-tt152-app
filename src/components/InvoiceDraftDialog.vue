<script setup lang="ts">
/**
 * Lập mới / sửa hóa đơn nháp — dùng chung cho màn Hóa đơn và màn Chờ xuất HĐĐT.
 *
 * Truyền `editInvoice` để sửa (nạp sẵn hóa đơn + dòng hàng), bỏ trống để lập mới
 * (tự sinh số hóa đơn kế tiếp). Lưu xong bắn `done` để màn cha nạp lại danh sách.
 */
import { storeToRefs } from "pinia";
import { useAuthStore } from "@/stores/auth";
import { useCatalogStore } from "@/stores/catalog";
import { useInvoiceStore } from "@/stores/invoice";
import type { Invoice } from "@/types";
import { fmtInt as fmt, parseIsoDate, toIsoDate } from "@/utils/format";

const visible = defineModel<boolean>("visible", { default: false });
const props = defineProps<{ editInvoice?: Invoice | null }>();
const emit = defineEmits<{ done: [] }>();

const auth = useAuthStore();
const catalog = useCatalogStore();
const invoiceStore = useInvoiceStore();
const { products, customers, industryGroups, warehouses } = storeToRefs(catalog);
const { invoices } = storeToRefs(invoiceStore);
const toast = useToast();

const saving = ref(false);
const editingId = ref<number | null>(null);
const form = reactive({
  number: "",
  date: new Date() as Date,
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

const isEdit = computed(() => editingId.value !== null);
const header = computed(() =>
  isEdit.value ? `Sửa hóa đơn nháp ${form.number}` : "Lập hóa đơn bán hàng (nháp)",
);
const actionLabel = computed(() => (isEdit.value ? "Lưu thay đổi" : "Lập hóa đơn"));

function nextInvoiceNo() {
  const nums = invoices.value
    .map((i) => parseInt(i.number.replace(/[^\d]/g, ""), 10))
    .filter((n) => !isNaN(n));
  const next = nums.length ? Math.max(...nums) + 1 : 1;
  form.number = `HD${String(next).padStart(4, "0")}`;
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

// ─── Thêm khách hàng nhanh: dùng chung PartnerDialog (tự sinh mã + tra cứu MST) ───
const customerDialog = ref(false);

function onCustomerSaved(code: string) {
  const c = catalog.customers.find((x) => x.code === code);
  if (c) {
    form.customer = c.name;
    if (c.tax_code) form.customer_tax_code = c.tax_code;
  }
}

/** Mỗi lần mở dialog: lập mới (tự sinh số) hoặc nạp hóa đơn cần sửa. */
watch(
  [visible, () => props.editInvoice?.id],
  async ([open, id], [wasOpen, wasId]) => {
    if (!open || (wasOpen && id === wasId)) return;
    if (!id) {
      editingId.value = null;
      Object.assign(form, {
        date: new Date(),
        customer: "",
        customer_tax_code: "",
        items: [],
      });
      nextInvoiceNo();
      return;
    }
    saving.value = true;
    try {
      const d = await invoiceStore.loadDetail(id);
      if (!d) return;
      editingId.value = id;
      Object.assign(form, {
        number: d.invoice.number,
        date: parseIsoDate(d.invoice.date) ?? new Date(),
        customer: d.invoice.customer ?? "",
        customer_tax_code: d.invoice.customer_tax_code ?? "",
        items: d.items.map((it) => ({
          product_code: it.product_code ?? "",
          quantity: it.quantity,
          unit_price: it.unit_price,
          industry_code: it.industry_code ?? "",
          discount: it.discount ?? 0,
          warehouse_code: it.warehouse_code ?? "",
        })),
      });
    } catch (e) {
      toast.add({ severity: "error", summary: "Không mở được hóa đơn", detail: String(e) });
      visible.value = false;
    } finally {
      saving.value = false;
    }
  },
  { immediate: true },
);

async function save() {
  // Chốt bấm nhiều lần: lần gọi sau bỏ qua, nếu không sẽ lập ra nhiều hóa đơn
  // trùng số (cùng đọc một số tự sinh ở form).
  if (saving.value) return;
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
    const payload = {
      number: form.number,
      date: toIsoDate(form.date),
      customer: form.customer,
      customer_tax_code: form.customer_tax_code,
      items: rows.map((it) => ({ ...it })),
    };
    const res = JSON.parse(
      editingId.value
        ? await invoiceStore.updateInvoice({ id: editingId.value, ...payload })
        : await invoiceStore.saveDraft(payload),
    );
    toast.add({
      severity: "success",
      summary: editingId.value ? `Đã sửa hóa đơn ${form.number}` : `Đã lập hóa đơn ${form.number}`,
      detail: `Tổng tiền: ${fmt(res.total)} đ • Thuế phải nộp (tỷ lệ nhóm ngành): ${fmt(res.tax_payable ?? 0)} đ`,
    });
    visible.value = false;
    emit("done");
  } catch (e) {
    toast.add({ severity: "error", summary: "Không lưu được hóa đơn", detail: String(e) });
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <AppDialog
    v-model:visible="visible"
    :header="header"
    width="max-w-6xl"
    :action-label="actionLabel"
    :saving="saving"
    :show-action="auth.canAccounting"
    @action="save"
  >
    <div class="grid grid-cols-3 gap-4 py-2">
      <FormField label="Số hóa đơn">
        <!-- Lập mới tự sinh số; sửa thì người dùng được sửa lại. -->
        <InputText v-model="form.number" :disabled="!isEdit" size="small" />
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

    <PartnerDialog
      v-model:visible="customerDialog"
      kind="customer"
      :show-action="auth.canAccounting"
      @saved="onCustomerSaved"
    />
  </AppDialog>
</template>
