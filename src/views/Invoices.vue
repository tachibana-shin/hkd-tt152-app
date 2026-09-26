<script setup lang="ts">
import { storeToRefs } from "pinia";
import { useAuthStore } from "@/stores/auth";
import { useCatalogStore } from "@/stores/catalog";
import { useInvoiceStore } from "@/stores/invoice";
import type { Invoice, InvoiceItem } from "@/types";
import { fmtInt as fmt, fmtLocalDateTime, fmtVnd } from "@/utils/format";

const auth = useAuthStore();
const catalog = useCatalogStore();
const invoiceStore = useInvoiceStore();
const { industryGroups } = storeToRefs(catalog);
const { invoices, detail, loading } = storeToRefs(invoiceStore);
const toast = useToast();

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

const linkDialog = ref(false);
const linkTarget = ref<Invoice | null>(null);

function openLinkHddt(inv: Invoice) {
  linkTarget.value = inv;
  linkDialog.value = true;
}

/** Sau khi ghi số HĐĐT: nạp lại danh sách + cập nhật hóa đơn đang mở. */
async function afterLink() {
  await invoiceStore.loadInvoices();
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
  await invoiceStore.loadInvoices();
  if (statusTarget.value) {
    const fresh = invoices.value.find((i) => i.id === statusTarget.value?.id);
    if (fresh) statusTarget.value = fresh;
  }
}

async function showDetail(inv: Invoice) {
  await invoiceStore.loadDetail(inv.id);
  detailDialog.value = true;
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

    <!-- Lập mới / sửa hóa đơn nháp -->
    <InvoiceDraftDialog
      v-model:visible="draftDialog"
      :edit-invoice="editingInvoice"
      @done="invoiceStore.loadInvoices()"
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
    <!-- Ghi nhận số HĐĐT sau khi phát hành bên kia -->
    <InvoiceLinkDialog v-model:visible="linkDialog" :invoice="linkTarget" @done="afterLink" />

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
