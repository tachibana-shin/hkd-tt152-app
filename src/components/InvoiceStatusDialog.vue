<script setup lang="ts">
/**
 * Xử lý sai sót của hóa đơn ĐÃ phát hành bằng tay qua dịch vụ HĐĐT khác.
 * TT 91/2026/TT-BTC Điều 10: từ 01/07/2026 không được tự hủy hóa đơn điện tử,
 * chỉ được thông báo / lập hóa đơn điều chỉnh / lập hóa đơn thay thế.
 *   * `replace`  — thay thế: app đảo doanh thu (Nợ 511 / Có 131) và tạo hóa đơn
 *     nháp mới sao chép dòng hàng để người dùng sửa rồi chép xuất lại.
 *   * `adjusted` — bên kia đã tự sửa: ghi nhận số HĐĐT điều chỉnh để đối chiếu.
 */
import { api } from "@/db";
import { useAuthStore } from "@/stores/auth";
import Textarea from "primevue/textarea";
import type { Invoice, InvoiceEvent, InvoiceReplaceResult } from "@/types";
import { fmtLocalDateTime } from "@/utils/format";

const visible = defineModel<boolean>("visible", { default: false });
const props = defineProps<{ invoice: Invoice | null; mode: "replace" | "adjusted" }>();
const emit = defineEmits<{ done: []; replaced: [result: InvoiceReplaceResult] }>();

const auth = useAuthStore();
const toast = useToast();
const reason = ref("");
const refInvoice = ref("");
const events = ref<InvoiceEvent[]>([]);
const saving = ref(false);

const isReplace = computed(() => props.mode === "replace");
const title = computed(() =>
  isReplace.value ? "Lập hóa đơn thay thế" : "Ghi nhận HĐ bị sửa bên kia",
);

const statusLabel: Record<string, string> = {
  draft: "Nháp",
  exported: "Đã chép — chờ phát hành",
  official: "Đã phát hành",
  cancelled: "Đã hủy",
  adjusted: "Đã bị sửa bên kia",
  replaced: "Đã bị thay thế",
};

async function loadEvents() {
  if (!props.invoice?.id) return;
  try {
    events.value = await api.invoiceEvents(props.invoice.id);
  } catch {
    events.value = [];
  }
}

watch(
  [visible, () => props.invoice?.id, () => props.mode],
  ([open], [wasOpen]) => {
    // Chỉ khởi tạo lúc MỞ dialog; đang mở mà props đổi thì giữ nguyên dữ liệu đang gõ.
    if (!open || wasOpen) return;
    reason.value = "";
    refInvoice.value = props.invoice?.ref_invoice ?? "";
    void loadEvents();
  },
  { immediate: true },
);

async function save() {
  if (!props.invoice?.id) return;
  if (!reason.value.trim()) {
    toast.add({ severity: "warn", summary: "Thiếu lý do", detail: "Ghi lý do để đối chiếu sau" });
    return;
  }
  if (!isReplace.value && !refInvoice.value.trim()) {
    toast.add({
      severity: "warn",
      summary: "Thiếu số HĐĐT điều chỉnh",
      detail: "Nhập ký hiệu/số HĐ bên kia để đối chiếu",
    });
    return;
  }
  saving.value = true;
  try {
    if (isReplace.value) {
      const res = await api.invoiceReplace(props.invoice.id, reason.value.trim());
      toast.add({
        severity: "success",
        summary: `Đã lập hóa đơn thay thế ${res.replacement_number}`,
        detail: `Doanh thu đã đảo bằng phiếu ${res.adjust_voucher_no} (Nợ 511 / Có 131). Sửa dòng hàng sai rồi chép xuất lại.`,
      });
      visible.value = false;
      emit("replaced", res);
    } else {
      await api.invoiceSetStatus({
        invoiceId: props.invoice.id,
        toStatus: "adjusted",
        reason: reason.value.trim(),
        refInvoice: refInvoice.value.trim(),
      });
      toast.add({ severity: "success", summary: title.value, detail: "Đã ghi nhận lịch sử" });
      visible.value = false;
    }
    emit("done");
  } catch (e) {
    toast.add({ severity: "error", summary: "Không cập nhật được", detail: String(e) });
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <AppDialog
    v-model:visible="visible"
    :header="`${title} — ${invoice?.number ?? ''}`"
    width="max-w-2xl"
    cancel-label="Đóng"
    :action-label="isReplace ? 'Tạo HĐ thay thế' : 'Ghi nhận'"
    :saving="saving"
    :show-action="auth.canAccounting"
    @action="save"
  >
    <div class="space-y-3">
      <Message v-if="isReplace" severity="warn" :closable="false">
        <div class="space-y-1 text-sm">
          <div>
            Không hủy được hóa đơn đã lập — app sẽ giữ hóa đơn này và đánh dấu
            <b>đã bị thay thế</b>, đồng thời đảo doanh thu bằng bút toán <b>Nợ 511 / Có 131</b> đúng
            bằng từng dòng và tạo hóa đơn nháp mới (sao chép khách + dòng hàng) để bạn sửa rồi chép
            xuất lại.
          </div>
          <div>
            Hàng hoá <b>không đụng tồn kho</b> — hàng vẫn nằm trong kho, phiếu xuất của hóa đơn thay
            thế sẽ trừ kho như bình thường. Hàng thật sự trả lại thì lập phiếu nhập kho riêng.
          </div>
        </div>
      </Message>

      <div class="grid grid-cols-2 gap-3 text-sm">
        <FormField
          :label="isReplace ? 'Lý do phải thay thế' : 'Lý do bị sửa'"
          required
          input-id="inv-status-reason"
          class="col-span-2"
        >
          <Textarea
            :id="'inv-status-reason'"
            v-model="reason"
            rows="2"
            class="w-full"
            :placeholder="
              isReplace
                ? 'Ví dụ: sai số lượng bán, ghi nhầm đơn giá…'
                : 'Ví dụ: bên kia sửa dòng hàng / đổi MST người mua…'
            "
          />
        </FormField>
        <FormField
          v-if="!isReplace"
          label="Số HĐĐT điều chỉnh bên kia"
          required
          input-id="inv-status-ref"
        >
          <InputText
            :id="'inv-status-ref'"
            v-model="refInvoice"
            size="small"
            class="w-full"
            placeholder="1C26TT152/00000124"
          />
        </FormField>
      </div>

      <div v-if="events.length" class="rounded border border-gray-200 p-2">
        <div class="mb-1 text-xs font-medium text-gray-500">Lịch sử trạng thái</div>
        <ul class="space-y-0.5 text-xs text-gray-600">
          <li v-for="e in events" :key="e.id">
            <span class="text-gray-400">{{ fmtLocalDateTime(e.at) }}</span>
            {{ statusLabel[e.from_status] ?? (e.from_status || "(mới)") }} →
            <b>{{ statusLabel[e.to_status] ?? e.to_status }}</b>
            <span v-if="e.reason"> · {{ e.reason }}</span>
            <span v-if="e.ref"> · {{ e.ref }}</span>
          </li>
        </ul>
      </div>
    </div>
  </AppDialog>
</template>
