<script setup lang="ts">
import { storeToRefs } from "pinia";
import { useAuthStore } from "@/stores/auth";
import { useBusinessStore } from "@/stores/business";
import { useCatalogStore } from "@/stores/catalog";
import { useInvoiceStore } from "@/stores/invoice";
import type {
  HddtSavedXml,
  Invoice,
  InvoiceDuplicateNumber,
  InvoiceItem,
  InvoiceReplaceResult,
} from "@/types";
import { api } from "@/db";
import { fmtInt as fmt, fmtLocalDateTime, fmtVnd } from "@/utils/format";
import { useKeepAliveRefresh } from "@/composables/useKeepAliveRefresh";
import { useLazyPage } from "@/composables/useLazyPage";
import { usePortalSession } from "@/composables/usePortalSession";

const auth = useAuthStore();
const business = useBusinessStore();
const catalog = useCatalogStore();
const invoiceStore = useInvoiceStore();
const portal = usePortalSession();
const { industryGroups } = storeToRefs(catalog);
const { detail } = storeToRefs(invoiceStore);
const toast = useToast();

// Danh sách hóa đơn không lọc theo ngày nên phình theo thời gian: phân trang
// server-side, mỗi lần đổi trang/tìm kiếm chỉ lấy đúng trang đó.
// `invoices` giờ là TRANG HIỆN TẠI (dùng cho các hộp thoại/mở phiếu), nên sau khi
// đổi trạng thái hóa đơn phải nạp lại trang đang xem thay vì tìm trong store.
const invoices = ref<Invoice[]>([]);
const loading = ref(false);
const total = ref(0);
const page = useLazyPage(async (lazy) => {
  loading.value = true;
  try {
    const res = await api.getInvoicesPage(lazy, onlyDuplicates.value);
    invoices.value = res.rows;
    total.value = res.total;
  } catch (e) {
    toast.add({ severity: "error", summary: "Lỗi tải hóa đơn", detail: String(e) });
  } finally {
    loading.value = false;
  }
});

const draftDialog = ref(false);
const detailDialog = ref(false);
const confirm = useConfirm();

// Hóa đơn đang sửa (null = lập mới) — hộp thoại lập/sửa dùng chung 1 component.
const editingInvoice = ref<Invoice | null>(null);

function openCreate() {
  editingInvoice.value = null;
  draftDialog.value = true;
}

function openEdit(inv: Invoice) {
  editingInvoice.value = inv;
  draftDialog.value = true;
}

// ─── Xem trước PDF ngay trên tab Hóa đơn (nháp lẫn đã phát hành) ───
const pdfVisible = ref(false);
const pdfDetail = ref("");
const pdfTitle = ref("");

/**
 * Backend dựng `detail_json` từ hóa đơn nội bộ rồi render PDF ngay trong Rust
 * — bản nháp in nhãn "HÓA ĐƠN NHÁP", bản đã phát hành giữ khung in như cũ.
 */
async function openPdf(inv: Invoice) {
  try {
    pdfDetail.value = await api.invoiceDraftDetail(inv.id);
    pdfTitle.value = `Hóa đơn ${inv.number}`;
    pdfVisible.value = true;
  } catch (e) {
    toast.add({ severity: "error", summary: "Không mở được PDF", detail: String(e), life: 4000 });
  }
}

const linkDialog = ref(false);
const linkTarget = ref<Invoice | null>(null);

function openLinkHddt(inv: Invoice) {
  linkTarget.value = inv;
  linkDialog.value = true;
}

/** Sau khi ghi số HĐĐT: nạp lại danh sách + cập nhật hóa đơn đang mở. */
async function afterLink() {
  await reload();
  const fresh = linkTarget.value ? invoices.value.find((i) => i.id === linkTarget.value?.id) : null;
  if (fresh) linkTarget.value = fresh;
}

const statusBadge: Record<string, "secondary" | "info" | "success" | "danger" | "warn"> = {
  draft: "secondary",
  pasted: "info",
  exported: "info",
  official: "success",
  cancelled: "danger",
  adjusted: "warn",
  replaced: "secondary",
};
const statusText: Record<string, string> = {
  draft: "Nháp",
  pasted: "Đã dán",
  exported: "Đã chép — chờ phát hành",
  official: "Đã phát hành",
  cancelled: "Đã hủy",
  adjusted: "Đã bị sửa bên kia",
  replaced: "Đã bị thay thế",
};

// ─── Chép sang dịch vụ HĐĐT khác (kiểm tra trước + bản chốt) ───
const exportVisible = ref(false);
const exportTarget = ref<Invoice | null>(null);

function openExport(inv: Invoice) {
  exportTarget.value = inv;
  exportVisible.value = true;
}

// ─── Thay thế / ghi nhận bị sửa bên kia ───
const statusVisible = ref(false);
const statusMode = ref<"replace" | "adjusted">("replace");
const statusTarget = ref<Invoice | null>(null);

function openStatus(inv: Invoice, mode: "replace" | "adjusted") {
  statusTarget.value = inv;
  statusMode.value = mode;
  statusVisible.value = true;
}

/** Sau khi thay thế: mở ngay hóa đơn nháp mới để sửa dòng hàng sai rồi xuất lại. */
async function onReplaced(res: InvoiceReplaceResult) {
  await reload();
  const fresh = invoices.value.find((i) => i.id === res.replacement_invoice_id);
  if (fresh) {
    editingInvoice.value = fresh;
    draftDialog.value = true;
  }
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
        // Xoá xong phải nạp lại cả báo cáo trùng số (dọn 1 bản trùng thì hết cảnh báo).
        await reload();
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

/** Lập phiếu xuất từ hóa đơn (1-1) — hóa đơn lập ở màn Hóa đơn thì chưa có PX. */
function makeOutbound(inv: Invoice) {
  confirm.require({
    header: "Lập phiếu xuất",
    message: `Lập phiếu xuất cho hóa đơn ${inv.number} (${fmtVnd(inv.total)})? Sẽ trừ tồn kho theo kho xuất trên từng dòng và ghi doanh thu Nợ 131 / Có 511.`,
    icon: "pi pi-receipt",
    acceptLabel: "Lập phiếu xuất",
    rejectLabel: "Hủy",
    accept: async () => {
      try {
        const no = await invoiceStore.createOutbound(inv.id);
        toast.add({
          severity: "success",
          summary: "Đã lập phiếu xuất",
          detail: `${no} · doanh thu đã vào sổ, tồn kho đã trừ`,
        });
      } catch (e) {
        toast.add({ severity: "error", summary: "Không lập được phiếu xuất", detail: String(e) });
      }
    },
  });
}

async function afterStatusChange() {
  await reload();
  if (statusTarget.value) {
    const fresh = invoices.value.find((i) => i.id === statusTarget.value?.id);
    if (fresh) statusTarget.value = fresh;
  }
}

async function showDetail(inv: Invoice) {
  await invoiceStore.loadDetail(inv.id);
  detailDialog.value = true;
  await loadDetailXml(inv);
}

// ─── File XML của hóa đơn này ───
// Quy định bắt buộc lưu file XML HĐĐT. app không tự tải lúc gán số HĐĐT (sau
// khi ký, cổng còn cache nên chưa có hồ sơ gốc) → để người dùng bấm tải ở đây
// và lúc xuất báo cáo kỳ thuế tự tìm bù.
const xmlSaving = ref(false);
/** File XML đã lưu của hóa đơn đang xem (null = chưa tải). */
const xmlFile = ref<HddtSavedXml | null>(null);

/** Số hóa đơn + ký hiệu đã gán HĐĐT — thiếu thì không tra XML được. */
const xmlNo = computed(() => ({
  khhdon: (detail.value?.invoice?.e_invoice_symbol ?? "").trim(),
  shdon: (detail.value?.invoice?.e_invoice_no ?? "").trim(),
}));
const canSaveXml = computed(() => !!xmlNo.value.khhdon && !!xmlNo.value.shdon);

async function loadDetailXml(inv: Invoice) {
  xmlFile.value = null;
  const khhdon = (inv.e_invoice_symbol ?? "").trim();
  const shdon = (inv.e_invoice_no ?? "").trim();
  if (!khhdon || !shdon) return;
  try {
    const rows = await api.hddtListXml();
    xmlFile.value =
      rows.find((r) => r.direction === "sold" && r.khhdon === khhdon && r.shdon === shdon) ?? null;
  } catch {
    // Không đọc được danh sách thì coi như chưa có — nút vẫn hiện, tải lại vẫn được.
    xmlFile.value = null;
  }
}

async function downloadXml() {
  const { khhdon, shdon } = xmlNo.value;
  if (!khhdon || !shdon || xmlSaving.value) return;
  const nbmst = (business.config?.tax_code ?? "").trim();
  if (!nbmst) {
    toast.add({
      severity: "warn",
      summary: "Chưa khai mã số thuế",
      detail: "Mở hồ sơ hộ kinh doanh để nhập mã số thuế — thiếu thì cổng không tra XML được.",
      life: 6000,
    });
    return;
  }
  xmlSaving.value = true;
  try {
    const result = await portal.ensureSession();
    if (result === "need_manual") {
      await portal.openManual();
      toast.add({
        severity: "warn",
        summary: "Cần đăng nhập cổng HĐĐT",
        detail: "Nhập mã captcha trong hộp thoại vừa mở rồi bấm Tải XML lần nữa.",
        life: 6000,
      });
      return;
    }
    if (result !== "logged_in") {
      toast.add({
        severity: "warn",
        summary: "Chưa đăng nhập cổng HĐĐT",
        detail:
          result === "not_configured"
            ? "Chưa khai tài khoản cổng — vào mục Đồng bộ HĐĐT để cấu hình."
            : "Đăng nhập cổng thất bại — thử lại sau.",
        life: 6000,
      });
      return;
    }
    // Không truyền `kind` — backend suy từ ký hiệu (chứ `CG` = máy tính tiền).
    const saved = await api.hddtSaveXml({
      direction: "sold",
      nbmst,
      khmshdon: 0,
      khhdon,
      shdon,
    });
    if (detail.value?.invoice) await loadDetailXml(detail.value.invoice);
    toast.add({
      severity: "success",
      summary: saved.already ? "File XML đã có từ trước" : "Đã tải file XML",
      detail: `${khhdon} — ${shdon} → ${saved.file_name}`,
      life: 4000,
    });
  } catch (e) {
    toast.add({
      severity: "error",
      summary: "Không tải được file XML",
      detail: String(e),
      life: 8000,
    });
  } finally {
    xmlSaving.value = false;
  }
}

// Số hóa đơn trùng: app chỉ BÁO, không tự sửa dữ liệu — người dùng tự xoá bản
// nháp thừa, bản đã phát hành thì xử lý bằng thay thế. Lọc "chỉ hiện trùng" chạy
// ở server (xem get_invoices_page) nên không bỏ sót bản trùng ở trang khác.
const duplicates = ref<InvoiceDuplicateNumber[]>([]);
const onlyDuplicates = ref(false);
const dupTotal = computed(() => duplicates.value.reduce((s, d) => s + d.count, 0));

/** Bật/tắt lọc "chỉ hiện hóa đơn trùng số" — lọc ở server nên phải nạp lại. */
function toggleDuplicates() {
  onlyDuplicates.value = !onlyDuplicates.value;
  void page.setExtra({});
}

async function reload() {
  // Thứ tự quan trọng: phải biết còn hóa đơn trùng không, rồi mới tắt chế độ
  // "chỉ hiện trùng" — nếu lấy trang chạy song song, nó dùng cờ cũ và bảng
  // hiện sai (vừa dọn xong trùng thì vẫn ra danh sách đã lọc).
  try {
    const dups = await api.invoiceDuplicateNumbers();
    duplicates.value = dups;
    if (!dups.length) onlyDuplicates.value = false;
  } catch (e) {
    toast.add({
      severity: "error",
      summary: "Không tải được cảnh báo trùng số",
      detail: String(e),
    });
  }
  await Promise.all([catalog.loadAll(), page.reload()]);
}

void reload();
// Quay lại màn (KeepAlive giữ state) → nạp lại dữ liệu cho khỏi cũ.
useKeepAliveRefresh(reload);
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

    <!-- Số hóa đơn trùng: báo để tự dọn, không tự sửa -->
    <Message v-if="duplicates.length" severity="warn" :closable="false" class="w-full">
      <div class="flex flex-wrap items-center gap-2 text-sm">
        <span>
          <b>{{ duplicates.length }}</b> số hóa đơn bị dùng trùng ({{ dupTotal }} bản ghi):
          <b v-for="(d, i) in duplicates.slice(0, 6)" :key="d.number" class="ml-1">
            {{ d.number }} ({{ d.count }}){{ i < Math.min(duplicates.length, 6) - 1 ? "," : "" }}
          </b>
          <span v-if="duplicates.length > 6">…</span>
          — thường do bấm "Lập hóa đơn" nhiều lần. Xoá bản nháp thừa; bản đã phát hành thì lập hóa
          đơn thay thế.
        </span>
        <Button
          :label="onlyDuplicates ? 'Xem tất cả' : 'Chỉ hiện hóa đơn trùng'"
          size="small"
          severity="warn"
          outlined
          @click="toggleDuplicates()"
        />
      </div>
    </Message>

    <Card>
      <template #content>
        <AppDataTable
          :value="invoices"
          :loading="loading"
          :totalRecords="total"
          :first="page.first.value"
          :rows="page.rowsPerPage.value"
          :rows-per-page-options="page.pageSizes"
          data-key="id"
          sort-mode="single"
          removable-sort
          filter-toggle
          :global-filter-fields="['number', 'customer', 'voucher_no', 'e_invoice_no']"
          stripedRows
          actions-header="Hành động"
          actions-width="220"
          @page="page.onPage"
          @sort="page.onSort"
          @search="page.onSearch"
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
                  :title="`Đã chép sang bên kia lúc ${fmtLocalDateTime(data.exported_at)}`"
                >
                  đã chép {{ fmtLocalDateTime(data.exported_at) }}
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
          <Column header="Ngày HĐĐT">
            <template #body="{ data }">
              <span>{{ data.e_invoice_date || "—" }}</span>
              <!-- Ngày HĐĐT khác ngày phiếu xuất = doanh thu kê lệch kỳ với chứng từ -->
              <div
                v-if="data.e_invoice_date && data.voucher_no && data.e_invoice_date !== data.date"
                class="text-xs text-amber-700"
                :title="`Phiếu xuất ${data.voucher_no} giữ ngày ${data.date}`"
              >
                ⚠ phiếu xuất: {{ data.date }}
              </div>
            </template>
          </Column>
          <Column header="Phiếu xuất">
            <template #body="{ data }">
              <span v-if="data.voucher_no">{{ data.voucher_no }}</span>
              <!-- Đã phát hành mà chưa có phiếu xuất = doanh thu chưa vào sổ, tờ
                   khai sẽ thiếu. Bấm 🧾 trên dòng để lập. -->
              <span
                v-else-if="data.status === 'official' || data.status === 'exported'"
                class="text-xs text-amber-700"
                :title="`Chưa có phiếu xuất — doanh thu chưa vào sổ. Bấm nút lập phiếu xuất ở cột Thao tác (${data.number}).`"
              >
                ⚠ chưa có
              </span>
              <span v-else>—</span>
            </template>
          </Column>
          <template #actions="{ data }">
            <div v-if="auth.canAccounting" class="flex items-center justify-center gap-1">
              <!-- Xem PDF có mặt ở mọi trạng thái (kể cả đã hủy / đã thay thế). -->
              <Button
                icon="pi pi-file-pdf"
                text
                rounded
                size="small"
                aria-label="Xem PDF hóa đơn"
                v-tooltip="'Xem PDF hóa đơn'"
                @click="openPdf(data)"
              />
              <!-- Đã bị thay thế thì không chép/liên kết được nữa -->
              <template v-if="data.status !== 'cancelled' && data.status !== 'replaced'">
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
                  v-if="data.e_invoice_no"
                  icon="pi pi-sync"
                  text
                  rounded
                  size="small"
                  aria-label="Lập hóa đơn thay thế"
                  v-tooltip="'Lập hóa đơn thay thế (đảo doanh thu + tạo HĐ nháp mới)'"
                  severity="danger"
                  @click="openStatus(data, 'replace')"
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
                <!-- Nháp thì sửa/xoá được; các trạng thái sau nháp giữ lại dấu vết -->
                <!-- Đã phát hành mà chưa có phiếu xuất → sổ đang thiếu khoản bán này -->
                <Button
                  v-if="
                    (data.status === 'official' || data.status === 'exported') && !data.voucher_no
                  "
                  icon="pi pi-receipt"
                  text
                  rounded
                  size="small"
                  aria-label="Lập phiếu xuất từ hóa đơn"
                  v-tooltip="'Lập phiếu xuất (trừ tồn + ghi doanh thu)'"
                  severity="success"
                  @click="makeOutbound(data)"
                />
                <Button
                  v-if="data.status === 'draft'"
                  icon="pi pi-file-edit"
                  text
                  rounded
                  size="small"
                  aria-label="Sửa hóa đơn nháp"
                  v-tooltip="'Sửa hóa đơn nháp'"
                  severity="secondary"
                  @click="openEdit(data)"
                />
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
              <span v-else class="text-xs text-gray-500" :title="data.replace_reason">
                {{ data.replace_reason || statusText[data.status] || data.status }}
              </span>
            </div>
          </template>
          <template #empty>
            <EmptyState
              :text="onlyDuplicates ? 'Không còn hóa đơn trùng số nào.' : 'Chưa có hóa đơn nào.'"
              icon="pi pi-receipt"
            />
          </template>
        </AppDataTable>
      </template>
    </Card>

    <!-- Lập mới / sửa hóa đơn nháp -->
    <InvoiceDraftDialog
      v-model:visible="draftDialog"
      :edit-invoice="editingInvoice"
      @done="reload"
    />

    <!-- Dialog chi tiết hóa đơn -->
    <AppDialog
      v-model:visible="detailDialog"
      header="Chi tiết hóa đơn"
      width="max-w-3xl"
      cancel-label="Đóng"
      :show-action="false"
    >
      <template v-if="detail">
        <!-- File XML của chính hóa đơn này — app không tự tải lúc gán số nên
             để đây cho người dùng bấm khi cần (hoặc tự tìm khi xuất báo cáo). -->
        <div
          v-if="canSaveXml"
          data-testid="invoice-xml-row"
          class="mb-4 flex flex-wrap items-center justify-between gap-2 rounded-md border border-surface-200 dark:border-surface-700 bg-surface-50 dark:bg-surface-800 px-3 py-2"
        >
          <span class="text-xs text-surface-600 dark:text-surface-300">
            File XML HĐĐT:
            <b :class="xmlFile ? 'text-emerald-600' : 'text-amber-600'">
              {{ xmlFile ? `đã lưu (${xmlFile.file_name})` : "chưa tải" }}
            </b>
          </span>
          <Button
            :label="xmlFile ? 'Tải lại XML' : 'Tải XML'"
            icon="pi pi-download"
            size="small"
            outlined
            :loading="xmlSaving"
            data-testid="download-invoice-xml"
            @click="downloadXml"
          />
        </div>
        <div class="grid grid-cols-1 sm:grid-cols-2 gap-3 text-sm mb-4">
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
    <!-- Ghi nhận số HĐĐT sau khi phát hành bên kia -->
    <InvoiceLinkDialog v-model:visible="linkDialog" :invoice="linkTarget" @done="afterLink" />

    <!-- Chép hóa đơn sang dịch vụ HĐĐT khác: kiểm tra trước + lưu bản chốt -->
    <InvoiceExportDialog v-model:visible="exportVisible" :invoice="exportTarget" @done="reload" />

    <!-- Thay thế / ghi nhận bị sửa bên kia -->
    <InvoiceStatusDialog
      v-model:visible="statusVisible"
      :invoice="statusTarget"
      :mode="statusMode"
      @done="afterStatusChange"
      @replaced="onReplaced"
    />

    <!-- Xem trước / in / tải PDF của chính hóa đơn ở dòng -->
    <InvoicePdfDialog v-model:visible="pdfVisible" :detail-json="pdfDetail" :title="pdfTitle" />
  </div>
</template>
