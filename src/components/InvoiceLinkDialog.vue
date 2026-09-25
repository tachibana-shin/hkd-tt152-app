<script setup lang="ts">
/**
 * Ghi nhận số HĐĐT sau khi phát hành ở máy tính tiền bên kia: nhập ký hiệu + số
 * HĐ → hóa đơn chuyển sang "Đã phát hành". Dùng chung cho màn Hóa đơn và màn
 * Chờ xuất HĐĐT.
 */
import { api } from "@/db";
import { useAuthStore } from "@/stores/auth";
import type { Invoice } from "@/types";

const visible = defineModel<boolean>("visible", { default: false });
const props = defineProps<{ invoice: Invoice | null }>();
const emit = defineEmits<{ done: [] }>();

const auth = useAuthStore();
const toast = useToast();
const linking = ref(false);
const form = reactive({ hddtNo: "", hddtSymbol: "", hddtDate: new Date() });

const iso = (d: Date | null) => (d ? d.toISOString().slice(0, 10) : "");

watch(
  [visible, () => props.invoice?.id],
  ([open, id], [wasOpen, wasId]) => {
    if (!open || (wasOpen && id === wasId)) return;
    form.hddtNo = props.invoice?.e_invoice_no ?? "";
    form.hddtSymbol = props.invoice?.e_invoice_symbol ?? "";
    form.hddtDate = props.invoice?.e_invoice_date
      ? new Date(props.invoice.e_invoice_date)
      : new Date();
  },
  { immediate: true },
);

async function save() {
  if (!props.invoice?.id) return;
  const hddtNo = form.hddtNo.trim();
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
      invoiceId: props.invoice.id,
      hddtNo,
      hddtSymbol: form.hddtSymbol.trim(),
      hddtDate: iso(form.hddtDate),
    });
    toast.add({
      severity: "success",
      summary: "Đã liên kết HĐĐT",
      detail: `Hóa đơn ${props.invoice.number} chuyển sang trạng thái Đã liên kết HĐĐT`,
    });
    visible.value = false;
    emit("done");
  } catch (e) {
    toast.add({ severity: "error", summary: "Liên kết HĐĐT thất bại", detail: String(e) });
  } finally {
    linking.value = false;
  }
}
</script>

<template>
  <AppDialog
    v-model:visible="visible"
    :header="`Liên kết hóa đơn điện tử (HĐĐT) — ${invoice?.number ?? ''}`"
    width="max-w-lg"
    action-label="Lưu"
    :saving="linking"
    :show-action="auth.canAccounting"
    @action="save"
  >
    <p class="text-sm text-gray-600">
      Hóa đơn {{ invoice?.number }} — {{ invoice?.customer }}. Sau khi lưu, chuyển sang trạng thái
      Đã liên kết HĐĐT.
    </p>
    <div class="grid grid-cols-1 gap-3 py-2">
      <FormField label="Số HĐĐT" required input-id="link-hddt-no">
        <InputText
          :id="'link-hddt-no'"
          v-model="form.hddtNo"
          placeholder="Nhập số hóa đơn điện tử"
          class="w-full"
        />
      </FormField>
      <FormField label="Ký hiệu HĐĐT" input-id="link-hddt-symbol">
        <InputText
          :id="'link-hddt-symbol'"
          v-model="form.hddtSymbol"
          placeholder="Nhập ký hiệu (VD: 1C24TT152)"
          class="w-full"
        />
      </FormField>
      <FormField label="Ngày HĐĐT">
        <DatePicker v-model="form.hddtDate" dateFormat="dd/mm/yy" class="w-full" />
      </FormField>
    </div>
  </AppDialog>
</template>
