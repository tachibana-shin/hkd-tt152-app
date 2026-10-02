<script setup lang="ts">
/**
 * Xem trước hóa đơn điện tử rồi in / tải xuống file PDF.
 *
 * Backend dựng HTML từ `detail_json` và render PDF ngay trong Rust
 * (`render_invoice_pdf`): frontend chỉ nhận bytes PDF rồi hiện qua `blob:` URL.
 * iframe có origin riêng nên không chạm được dữ liệu của app, và máy user
 * không cần cài Chrome — HTML/CSS/QR hoàn toàn nằm ở backend.
 */
import { api } from "@/db";
import type { InvoiceData } from "@/invoice-pdf/types";

const visible = defineModel<boolean>("visible", { default: false });
const props = defineProps<{
  /** Chuỗi `detail_json` của hóa đơn (lấy từ DB hoặc từ cổng HĐĐT). */
  detailJson: string;
  /** Tiêu đề dialog, mặc định ghép ký hiệu + số hóa đơn. */
  title?: string;
}>();

const toast = useToast();
const iframeRef = ref<HTMLIFrameElement>();
const blobUrl = ref("");
const loading = ref(false);
const errorMsg = ref("");

const invoice = ref<InvoiceData>();
const header = computed(
  () =>
    props.title ||
    (invoice.value ? `Hóa đơn ${invoice.value.khhdon} ${invoice.value.shdon}` : "Hóa đơn điện tử"),
);

function releaseBlob() {
  if (blobUrl.value) URL.revokeObjectURL(blobUrl.value);
  blobUrl.value = "";
}

/** base64 (lỗi IPC/HTTP) → bytes PDF → `blob:` URL. */
function toPdfBlobUrl(base64: string): string {
  const bytes = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
  return URL.createObjectURL(new Blob([bytes], { type: "application/pdf" }));
}

// Render lại mỗi khi mở dialog hoặc đổi hóa đơn (không dùng onMounted: mỗi lần
// mở là 1 hóa đơn khác).
watch(
  () => [visible.value, props.detailJson] as const,
  async ([isOpen, raw]) => {
    if (!isOpen || !raw) return;
    loading.value = true;
    errorMsg.value = "";
    releaseBlob();
    invoice.value = undefined;
    try {
      invoice.value = JSON.parse(raw) as InvoiceData;
      blobUrl.value = toPdfBlobUrl(await api.renderInvoicePdf(raw));
    } catch (e) {
      errorMsg.value = String(e);
      toast.add({ severity: "error", summary: "Không render được PDF", detail: String(e) });
    } finally {
      loading.value = false;
    }
  },
  { immediate: true },
);

onBeforeUnmount(releaseBlob);

function printInvoice() {
  iframeRef.value?.contentWindow?.focus();
  iframeRef.value?.contentWindow?.print();
}

/** Tải file .pdf về máy (dùng chính blob đang xem trước). */
function downloadPdf() {
  if (!blobUrl.value) return;
  const link = document.createElement("a");
  link.href = blobUrl.value;
  link.download = `${header.value.replace(/[\\/:*?"<>|]+/g, "-")}.pdf`;
  link.click();
}
</script>

<template>
  <AppDialog
    v-model:visible="visible"
    :header="header"
    width="max-w-5xl"
    :show-action="false"
    cancel-label="Đóng"
  >
    <div v-if="loading" class="flex justify-center py-10">
      <ProgressSpinner />
    </div>

    <p v-else-if="errorMsg" class="py-4 text-sm text-red-600">{{ errorMsg }}</p>

    <iframe
      v-else-if="blobUrl"
      ref="iframeRef"
      :src="blobUrl"
      title="Hóa đơn điện tử"
      class="h-[72vh] w-full border-0 bg-white"
    />

    <template #footer>
      <Button
        label="Đóng"
        icon="pi pi-times"
        severity="secondary"
        outlined
        @click="visible = false"
      />
      <Button
        label="Tải PDF"
        icon="pi pi-file-pdf"
        severity="secondary"
        :disabled="!blobUrl"
        @click="downloadPdf"
      />
      <Button
        label="In / Lưu thành PDF"
        icon="pi pi-print"
        :disabled="!blobUrl"
        @click="printInvoice"
      />
    </template>
  </AppDialog>
</template>
