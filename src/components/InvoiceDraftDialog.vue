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
import LineItemsEditor from "@/components/LineItemsEditor.vue";
import ProductAliasDialog from "@/components/ProductAliasDialog.vue";
import ProductionLotDialog from "@/components/ProductionLotDialog.vue";
import type { Invoice } from "@/types";
import { fmtInt as fmt, parseIsoDate, toIsoDate } from "@/utils/format";

const visible = defineModel<boolean>("visible", { default: false });
const props = defineProps<{ editInvoice?: Invoice | null }>();
const emit = defineEmits<{ done: [] }>();

const auth = useAuthStore();
const catalog = useCatalogStore();
const invoiceStore = useInvoiceStore();
const confirm = useConfirm();
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
    /** Tên hiển thị dòng (tên khác / alias) — rỗng = tên sản phẩm (F5). */
    line_name?: string;
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
    line_name: "",
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

// ─── Nút ở header bảng "Mặt hàng": thêm tên khác + tạo nhanh lô sản xuất ───
const aliasDialog = ref(false);
const lotDialog = ref(false);
const editor = ref<InstanceType<typeof LineItemsEditor> | null>(null);

/**
 * Thêm tên khác từ header (chưa đứng trên dòng nào) → gắn thẳng tên vừa thêm
 * vào dòng đầu đang dùng đúng hàng đó và CHƯA đặt tên riêng: in ra đúng tên
 * người vừa gõ. Hàng chưa có trên hóa đơn thì chỉ lưu danh mục — báo luôn cho
 * người dùng khỏi tưởng bảng hỏng.
 */
function onAliasSaved(code: string, added: string[]) {
  const row = form.items.find((it) => it.product_code === code && !it.line_name);
  if (row) row.line_name = added[0];
  const detail = `${added.join(", ")} → ${catalog.productByCode(code)?.name ?? code}`;
  toast.add({
    severity: "success",
    summary: "Đã thêm tên khác",
    detail: row
      ? `${detail} · gắn vào dòng hóa đơn`
      : `${detail} · hàng chưa có trên hóa đơn nên chỉ lưu vào danh mục`,
  });
}

/**
 * Tạo lô ngay trong popup: nạp lại danh mục + tồn (để soi thiếu tồn không dùng
 * số cũ) rồi gắn thành phẩm vừa tạo vào dòng trống — không phải quay lại ô chọn.
 */
async function onLotDone(payload: { voucherNo: string; productCode: string }) {
  try {
    await Promise.all([catalog.loadAll(), catalog.loadOnhand(true)]);
  } catch (e) {
    // Nạp lại lỗi vẫn gắn hàng được — popup thiếu tồn sẽ báo khi lưu.
    toast.add({ severity: "warn", summary: "Không làm mới được tồn kho", detail: String(e) });
  }
  let index = form.items.findIndex((it) => !it.product_code);
  if (index < 0) {
    addRow();
    index = form.items.length - 1;
  }
  editor.value?.assignProduct(index, payload.productCode);
}

/** Mỗi lần mở dialog: lập mới (tự sinh số) hoặc nạp hóa đơn cần sửa. */
watch(
  [visible, () => props.editInvoice?.id],
  async ([open, id], [wasOpen, wasId]) => {
    if (!open || (wasOpen && id === wasId)) return;
    // Soi thiếu tồn phải theo số mới nhất — map chỉ nạp 1 lần ở nơi khác nên
    // ép tải lại mỗi lần mở (nhanh, không chặn render form).
    void catalog.loadOnhand(true);
    if (!id) {
      editingId.value = null;
      Object.assign(form, {
        date: new Date(),
        customer: "",
        customer_tax_code: "",
        items: [],
        // Số để trống tới khi gợi ý xong — tránh hiện tạm một số đã tồn tại
        // (mở app mới thì store chưa nạp danh sách hóa đơn).
        number: "",
      });
      saving.value = true;
      try {
        await invoiceStore.loadInvoices();
        nextInvoiceNo();
      } catch {
        // Không tải được danh sách → vẫn gợi ý từ số đang có trong store.
        nextInvoiceNo();
      } finally {
        saving.value = false;
      }
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
          // Giữ nguyên TÊN người dùng đã chọn (có thể là tên khác / alias) —
          // `product_name` trả về đã là tên hiển thị, nhưng cần bản gốc để lưu.
          line_name: it.line_name ?? "",
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

  // Soi tồn NGAY LÚC CHỐT (có thể vừa tạo lô / nhập kho trong lúc mở popup).
  await catalog.loadOnhand(true);
  const missing = rows
    .map((it) => {
      const p = catalog.productByCode(it.product_code);
      // Hàng dịch vụ / nhân công không theo dõi tồn kho.
      if (!p || p.is_service || !(it.quantity > 0)) return "";
      const avail = catalog.onhandAt(it.product_code, it.warehouse_code ?? "");
      const short = it.quantity - avail;
      return short > 1e-9 ? `${p.name}: thiếu ${fmt(short)}` : "";
    })
    .filter(Boolean);

  if (missing.length) {
    // Nháp THÌ vẫn lưu được (HĐĐT đã xuất cũng cần thời gian mới lên cổng thông
    // tin điện tử) — nhưng phải hỏi trước; ghi số HĐĐT sau này vẫn bị chặn.
    confirm.require({
      header: "Tồn kho đang thiếu",
      message: `${missing.join("; ")}. Vẫn lưu nháp? Hóa đơn nháp lưu được, nhưng đến lúc ghi số HĐĐT sẽ bị chặn nếu vẫn thiếu kho.`,
      icon: "pi pi-exclamation-triangle",
      acceptLabel: "Vẫn lưu nháp",
      rejectLabel: "Quay lại",
      accept: async () => {
        await persist(rows);
      },
    });
    return;
  }
  await persist(rows);
}

async function persist(rows: typeof form.items) {
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
    // Backend luôn báo các dòng thiếu tồn dù đã lưu — gộp vào toast duy nhất.
    const shortage: string[] = res.shortage ?? [];
    toast.add({
      severity: shortage.length ? "warn" : "success",
      summary: editingId.value ? `Đã sửa hóa đơn ${form.number}` : `Đã lập hóa đơn ${form.number}`,
      detail: [
        `Tổng tiền: ${fmt(res.total)} đ • Thuế phải nộp (tỷ lệ nhóm ngành): ${fmt(res.tax_payable ?? 0)} đ`,
        ...(shortage.length ? [`Thiếu tồn kho: ${shortage.join("; ")}`] : []),
      ].join(" • "),
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
    <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4 py-2">
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
      ref="editor"
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
    >
      <template #actions>
        <Button
          label="Thêm tên khác"
          icon="pi pi-tag"
          size="small"
          text
          :disabled="!auth.canAccounting"
          data-testid="header-add-alias"
          v-tooltip="'Thêm tên khác cho một mặt hàng'"
          @click="aliasDialog = true"
        />
        <Button
          label="Tạo lô sản xuất"
          icon="pi pi-factory"
          size="small"
          text
          :disabled="!auth.canStock"
          data-testid="header-create-lot"
          v-tooltip="'Tạo nhanh lô sản xuất — thành phẩm được gắn vào dòng trống'"
          @click="lotDialog = true"
        />
      </template>
    </LineItemsEditor>

    <PartnerDialog
      v-model:visible="customerDialog"
      kind="customer"
      :show-action="auth.canAccounting"
      @saved="onCustomerSaved"
    />

    <!-- Thêm tên khác từ header bảng — cùng dialog với icon trên từng dòng. -->
    <ProductAliasDialog v-model:visible="aliasDialog" @saved="onAliasSaved" />
    <!-- Tạo nhanh lô sản xuất trong lúc đang lập hóa đơn. -->
    <ProductionLotDialog v-model:visible="lotDialog" @done="onLotDone" />
  </AppDialog>
</template>
