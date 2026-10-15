<script setup lang="ts">
/**
 * Sửa hóa đơn ĐÃ XỬ LÝ (đã chép sang bên kia / đã phát hành) cho khớp hóa đơn
 * chính thức ở hệ thống ký bên kia — app liên kết hoàn toàn thủ công nên bản
 * ghi local có thể lệch hóa đơn thật.
 *
 * Hai nhóm trường tách bạch với sổ:
 * + NGOÀI SỔ: số hiệu, ngày, khách hàng, MST, số/ký hiệu/ngày HĐĐT — gọi
 *   `update_invoice_header`, không đụng bút toán (ngày chỉ đổi trong cùng kỳ
 *   khi hóa đơn đã có phiếu xuất).
 * + DÒNG HÀNG: SL, đơn giá, chiết khấu, tên dòng… — gọi `update_invoice_lines`:
 *   có phiếu xuất thì app tự ghi phiếu {số}DT (đảo toàn bộ doanh thu cũ + ghi
 *   lại theo HĐ chính thức, bù tồn kho / giá vốn theo chênh lệch số lượng);
 *   thiếu kho ở phần tăng thêm thì backend chặn.
 */
import { storeToRefs } from "pinia";
import LineItemsEditor from "@/components/LineItemsEditor.vue";
import { api } from "@/db";
import { useAuthStore } from "@/stores/auth";
import { useCatalogStore } from "@/stores/catalog";
import type { Invoice } from "@/types";
import { fmtVnd, parseIsoDate, toIsoDate } from "@/utils/format";

const visible = defineModel<boolean>("visible", { default: false });
const props = defineProps<{ invoice: Invoice | null }>();
const emit = defineEmits<{ done: [] }>();

const auth = useAuthStore();
const catalog = useCatalogStore();
const { products, industryGroups, warehouses } = storeToRefs(catalog);
const toast = useToast();
const saving = ref(false);

type Line = {
  product_code: string;
  quantity: number;
  unit_price: number;
  industry_code?: string;
  discount?: number;
  warehouse_code?: string;
  line_name?: string;
};

const form = reactive({
  number: "",
  date: new Date(),
  customer: "",
  customerTaxCode: "",
  eInvoiceNo: "",
  eInvoiceSymbol: "",
  eInvoiceDate: null as Date | null,
  items: [] as Line[],
});

/** Bản gốc nạp từ DB — đối chiếu xem thật sự có đổi gì trước khi gửi. */
let origHeader = "";
let origItems = "[]";

function headerJson() {
  return JSON.stringify([
    form.number,
    toIsoDate(form.date),
    form.customer,
    form.customerTaxCode,
    form.eInvoiceNo,
    form.eInvoiceSymbol,
    form.eInvoiceDate ? toIsoDate(form.eInvoiceDate) : "",
  ]);
}

async function prefill() {
  const inv = props.invoice;
  form.number = inv?.number ?? "";
  form.date = parseIsoDate(inv?.date) ?? new Date();
  form.customer = inv?.customer ?? "";
  form.customerTaxCode = inv?.customer_tax_code ?? "";
  form.eInvoiceNo = inv?.e_invoice_no ?? "";
  form.eInvoiceSymbol = inv?.e_invoice_symbol ?? "";
  form.eInvoiceDate = parseIsoDate(inv?.e_invoice_date);
  form.items = [];
  origHeader = headerJson();
  origItems = "[]";
  if (!inv?.id) return;
  saving.value = true;
  try {
    const d = await api.getInvoiceDetail(inv.id);
    form.items = d.items.map((it) => ({
      product_code: it.product_code ?? "",
      quantity: it.quantity,
      unit_price: it.unit_price,
      industry_code: it.industry_code ?? "",
      discount: it.discount ?? 0,
      warehouse_code: it.warehouse_code ?? "",
      line_name: it.line_name ?? "",
    }));
    origItems = JSON.stringify(form.items);
  } catch (e) {
    toast.add({ severity: "error", summary: "Không nạp được dòng hàng", detail: String(e) });
  } finally {
    saving.value = false;
  }
}

watch(
  [visible, () => props.invoice?.id],
  ([open, id], [wasOpen, wasId]) => {
    if (!open || (wasOpen && id === wasId)) return;
    void catalog.loadAll(); // ô chọn hàng / kho / nhóm ngành phải sẵn sàng
    void prefill();
  },
  { immediate: true },
);

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

async function save() {
  if (saving.value || !props.invoice?.id) return;
  const rows = form.items.filter((it) => it.product_code);
  if (!rows.length) {
    toast.add({
      severity: "warn",
      summary: "Thiếu thông tin",
      detail: "Cần ít nhất 1 mặt hàng trên hóa đơn.",
    });
    return;
  }
  if (!form.number.trim() || !form.customer.trim()) {
    toast.add({
      severity: "warn",
      summary: "Thiếu thông tin",
      detail: "Cần nhập Số hóa đơn và Khách hàng trước khi lưu.",
    });
    return;
  }

  const headerChanged = headerJson() !== origHeader;
  const linesChanged = JSON.stringify(rows) !== origItems;
  if (!headerChanged && !linesChanged) {
    toast.add({ severity: "info", summary: "Không có gì để đổi", life: 3000 });
    visible.value = false;
    emit("done");
    return;
  }

  saving.value = true;
  try {
    if (headerChanged) {
      const res = await api.updateInvoiceHeader({
        id: props.invoice.id,
        number: form.number.trim(),
        date: toIsoDate(form.date),
        customer: form.customer.trim(),
        customer_tax_code: form.customerTaxCode.trim(),
        e_invoice_no: form.eInvoiceNo.trim(),
        e_invoice_symbol: form.eInvoiceSymbol.trim(),
        e_invoice_date: form.eInvoiceDate ? toIsoDate(form.eInvoiceDate) : "",
      });
      toast.add({
        severity: res.warning ? "warn" : "success",
        summary: "Đã sửa thông tin theo HĐ chính thức",
        detail: res.changes.join(" · "),
        life: res.warning ? 6000 : 4000,
      });
      if (res.warning) {
        toast.add({ severity: "warn", summary: "Lưu ý về ngày", detail: res.warning, life: 9000 });
      }
    }
    if (linesChanged) {
      const r = await api.updateInvoiceLines({ id: props.invoice.id, items: rows });
      toast.add({
        severity: "success",
        summary: "Đã sửa dòng hàng theo HĐ chính thức",
        detail: r.voucher_dt
          ? `Doanh thu ${fmtVnd(r.old_total)} → ${fmtVnd(r.new_total)} · lập phiếu ${r.voucher_dt}`
          : `Tổng tiền ${fmtVnd(r.old_total)} → ${fmtVnd(r.new_total)} (chưa có phiếu xuất nên không sinh bút toán)`,
        life: 7000,
      });
      if (r.warning) {
        toast.add({
          severity: "warn",
          summary: "Cảnh báo",
          detail: r.warning,
          life: 9000,
        });
      }
    }
    visible.value = false;
    emit("done");
  } catch (e) {
    toast.add({
      severity: "error",
      summary: "Lưu thay đổi thất bại",
      detail: String(e),
      life: 9000,
    });
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <AppDialog
    v-model:visible="visible"
    :header="`Sửa theo hóa đơn chính thức — ${invoice?.number ?? ''}`"
    width="max-w-4xl"
    cancel-label="Đóng"
    action-label="Lưu"
    :saving="saving"
    :show-action="true"
    @action="save"
  >
    <p class="text-sm text-gray-600">
      Dùng khi bản ghi trong app lệch hóa đơn đã ký ở hệ thống bên kia. Thông tin hồ sơ (số, ngày,
      khách hàng, MST, số/ký hiệu HĐĐT) không ảnh hưởng bút toán. Sửa dòng hàng khi hóa đơn đã có
      phiếu xuất thì app tự ghi phiếu điều chỉnh
      <b>{số}DT</b> — đảo và ghi lại doanh thu, bù tồn kho / giá vốn theo chênh lệch; thiếu kho ở
      phần tăng thêm sẽ bị chặn.
    </p>

    <div class="grid grid-cols-2 gap-3 py-2">
      <FormField label="Số hóa đơn" required input-id="hdr-number">
        <InputText :id="'hdr-number'" v-model="form.number" class="w-full" />
      </FormField>
      <FormField label="Ngày hóa đơn" required input-id="hdr-date">
        <DatePicker
          :input-id="'hdr-date'"
          v-model="form.date"
          dateFormat="dd/mm/yy"
          class="w-full"
        />
      </FormField>
      <FormField label="Khách hàng" required input-id="hdr-customer">
        <InputText :id="'hdr-customer'" v-model="form.customer" class="w-full" />
      </FormField>
      <FormField label="MST khách hàng" input-id="hdr-taxcode">
        <InputText :id="'hdr-taxcode'" v-model="form.customerTaxCode" class="w-full" />
      </FormField>
      <FormField label="Số HĐĐT (bên kia)" input-id="hdr-e-no">
        <InputText
          :id="'hdr-e-no'"
          v-model="form.eInvoiceNo"
          placeholder="Số hóa đơn trên hệ thống ký"
          class="w-full"
        />
      </FormField>
      <FormField label="Ký hiệu HĐĐT" input-id="hdr-e-symbol">
        <InputText
          :id="'hdr-e-symbol'"
          v-model="form.eInvoiceSymbol"
          placeholder="VD: 1C26TT152"
          class="w-full"
        />
      </FormField>
      <FormField label="Ngày HĐĐT" input-id="hdr-e-date">
        <DatePicker
          :input-id="'hdr-e-date'"
          v-model="form.eInvoiceDate"
          dateFormat="dd/mm/yy"
          show-clear
          class="w-full"
        />
      </FormField>
    </div>

    <div class="border-t pt-2">
      <div class="mb-1 text-sm font-medium">Dòng hàng (theo hóa đơn chính thức)</div>
      <LineItemsEditor
        :items="form.items"
        :products="products"
        :industry-groups="industryGroups"
        :warehouses="warehouses"
        :can-edit="auth.canAccounting"
        :warn-shortage="false"
        price-field="sale_price"
        show-industry
        show-amount
        amount-input
        show-discount
        show-unit
        show-warehouse
        compact
        info="SL / Thành tiền / Tiền CK theo hóa đơn chính thức. Hàng hóa được bù tồn theo CHÊNH LỆCH so với trước khi sửa — phần tăng thêm thiếu kho thì chặn lúc Lưu (nhập thêm hàng hoặc dùng phiếu điều chỉnh)."
        total-label="Tổng tiền:"
        @add="addRow"
        @remove="removeRow"
      />
    </div>
  </AppDialog>
</template>
