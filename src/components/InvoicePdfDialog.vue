<script setup lang="ts">
/**
 * Xem trước hóa đơn điện tử rồi in / tải xuống file PDF.
 *
 * HTML được dựng hoàn toàn offline từ `detail_json` (`buildInvoiceHtml`): font,
 * ảnh và script sinh QR đều nhúng base64 nên mở qua `blob:` URL — không phụ
 * thuộc mạng và iframe có origin riêng, không chạm được dữ liệu của app.
 *
 * Nút "Tải PDF" gọi backend in bằng Chrome headless (`render_invoice_pdf`);
 * nút "In" dùng chính hộp in của trình duyệt (chọn "Lưu thành PDF").
 */
import { api } from "@/db";
import { buildInvoiceHtml } from "@/invoice-pdf/build";
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
const html = ref("");
const blobUrl = ref("");
const loading = ref(false);
const errorMsg = ref("");
const exporting = ref(false);

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

// Dựng lại mỗi khi mở dialog hoặc đổi hóa đơn (không dùng onMounted: mỗi lần mở
// là 1 hóa đơn khác).
watch(
  () => [visible.value, props.detailJson] as const,
  async ([isOpen, raw]) => {
    if (!isOpen || !raw) return;
    loading.value = true;
    errorMsg.value = "";
    html.value = "";
    releaseBlob();
    invoice.value = undefined;
    try {
      const data = JSON.parse(raw) as InvoiceData;
      const built = await buildInvoiceHtml(data);
      invoice.value = data;
      html.value = built;
      blobUrl.value = URL.createObjectURL(new Blob([built], { type: "text/html" }));
    } catch (e) {
      errorMsg.value = String(e);
      toast.add({ severity: "error", summary: "Không dựng được hóa đơn", detail: String(e) });
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

/** In bằng Chrome headless rồi tải file .pdf về máy. */
async function downloadPdf() {
  if (!html.value || exporting.value) return;
  exporting.value = true;
  try {
    const base64 = await api.renderInvoicePdf(html.value);
    const bytes = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
    const url = URL.createObjectURL(new Blob([bytes], { type: "application/pdf" }));
    const link = document.createElement("a");
    link.href = url;
    link.download = `${header.value.replace(/[\\/:*?"<>|]+/g, "-")}.pdf`;
    link.click();
    URL.revokeObjectURL(url);
  } catch (e) {
    toast.add({ severity: "error", summary: "Không in được PDF", detail: String(e) });
  } finally {
    exporting.value = false;
  }
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
        :loading="exporting"
        :disabled="!html || exporting"
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
