<script setup lang="ts">
/**
 * Chờ xuất HĐĐT — danh sách việc phải làm, không phải danh sách hồ sơ.
 *
 * Ở đây chỉ có hóa đơn chưa phát hành (nháp hoặc đã chép sang máy tính tiền bên
 * kia), mỗi dòng kèm kết quả kiểm tra trước khi chép: bên kia chắc chắn từ chối
 * hóa đơn thiếu MST hợp lệ, thiếu nhóm ngành, tổng không khớp tổng dòng.
 *
 * Vòng lặp bàn phím: F8 chép dòng đang chọn (mở hộp thoại chép, bấm Chép rồi
 * dán sang bên kia), F9 ghi số HĐĐT sau khi phát hành xong → dòng biến khỏi
 * danh sách. Không phải mở từng hồ sơ.
 */
import { api } from "@/db";
import { useAuthStore } from "@/stores/auth";
// PrimeVue không được auto-import trong dự án (resolver chỉ khai các component đã
// dùng) — phải import tường minh, nếu không SelectButton không render.
import SelectButton from "primevue/selectbutton";
import type { Invoice, InvoiceQueueItem } from "@/types";
import { fmtVnd, toIsoDate } from "@/utils/format";
import { useKeepAliveRefresh } from "@/composables/useKeepAliveRefresh";
import { useLazyPage } from "@/composables/useLazyPage";

const auth = useAuthStore();
const toast = useToast();
const confirm = useConfirm();

const items = ref<InvoiceQueueItem[]>([]);
const loading = ref(false);
const keyword = ref("");
const statusFilter = ref<"all" | "draft" | "exported">("all");
const from = ref<Date | null>(null);
const to = ref<Date | null>(null);
const selectedId = ref<number | null>(null);

// Các bộ đếm chạy trên dữ liệu đang hiển thị (trang hiện tại) — server đã lọc
// theo từ khoá/trạng thái/ngày rồi, không còn lọc client.
const blockedCount = computed(() => items.value.filter((i) => i.error_count > 0).length);
const readyCount = computed(() => items.value.filter((i) => i.error_count === 0).length);
const copiedCount = computed(() => items.value.filter((i) => i.copied).length);

const statusText: Record<string, string> = {
  draft: "Nháp",
  exported: "Đã chép — chờ phát hành",
};

const selected = computed(() => items.value.find((i) => i.invoice.id === selectedId.value) ?? null);

/** Màn này thao tác bằng phím tắt nên luôn có 1 dòng được chọn sẵn. */
watch(items, (rows) => {
  if (selectedId.value && rows.some((r) => r.invoice.id === selectedId.value)) return;
  // Ưu tiên dòng chưa chép, không còn lỗi — việc cần làm gấp nhất.
  selectedId.value =
    (rows.find((r) => !r.copied && r.error_count === 0) ?? rows[0])?.invoice.id ?? null;
});

const exportVisible = ref(false);
const exportTarget = ref<Invoice | null>(null);
const linkVisible = ref(false);
const linkTarget = ref<Invoice | null>(null);
const draftVisible = ref(false);
const draftTarget = ref<Invoice | null>(null);
const pdfVisible = ref(false);
const pdfDetail = ref("");
const pdfTitle = ref("");

function openExport(inv: Invoice) {
  exportTarget.value = inv;
  exportVisible.value = true;
}
function openLink(inv: Invoice) {
  linkTarget.value = inv;
  linkVisible.value = true;
}
function openDraft(inv: Invoice) {
  draftTarget.value = inv;
  draftVisible.value = true;
}

/** Xem trước PDF hóa đơn ở hàng chờ — backend dựng `detail_json` từ hóa đơn nội bộ. */
async function openPdf(inv: Invoice) {
  try {
    pdfDetail.value = await api.invoiceDraftDetail(inv.id);
    pdfTitle.value = `Hóa đơn ${inv.number}`;
    pdfVisible.value = true;
  } catch (e) {
    toast.add({
      severity: "error",
      summary: "Không mở được PDF",
      detail: String(e),
      life: 4000,
    });
  }
}

function removeItem(inv: Invoice) {
  confirm.require({
    header: "Xoá hóa đơn nháp",
    message: `Xoá hóa đơn nháp ${inv.number} (${fmtVnd(inv.total)})?`,
    icon: "pi pi-exclamation-triangle",
    acceptLabel: "Xoá",
    rejectLabel: "Hủy",
    accept: async () => {
      try {
        await api.deleteInvoice(inv.id);
        toast.add({ severity: "success", summary: "Đã xoá hóa đơn nháp", detail: inv.number });
        await load();
      } catch (e) {
        toast.add({ severity: "error", summary: "Không xoá được", detail: String(e) });
      }
    },
  });
}

// Hàng chờ xuất phình theo thời gian (hóa đơn nháp chưa xuất, hóa đơn đã xuất
// chờ xử lý) → phân trang server-side, mỗi lần đổi trang/lọc chỉ lấy đúng trang.
const total = ref(0);
const page = useLazyPage(async (lazy) => {
  loading.value = true;
  try {
    const res = await api.invoiceQueuePage(
      lazy,
      statusFilter.value === "all" ? "" : statusFilter.value,
      toIsoDate(from.value),
      toIsoDate(to.value),
    );
    items.value = res.rows;
    total.value = res.total;
    // Giữ dòng đang chọn nếu vẫn còn trong danh sách.
    if (selectedId.value && !items.value.some((i) => i.invoice.id === selectedId.value)) {
      selectedId.value = null;
    }
  } catch (e) {
    items.value = [];
    toast.add({ severity: "error", summary: "Không tải được hàng chờ xuất", detail: String(e) });
  } finally {
    loading.value = false;
  }
});

/**
 * Bộ lọc riêng của màn (từ khoá + trạng thái + khoảng ngày) — trước đây lọc ở
 * client trên toàn bộ hàng chờ, giờ server lọc nên đổi điều kiện phải nạp lại.
 */
function applyFilters() {
  void page.setExtra({});
}

/** Ô tìm kiếm riêng của màn → đưa từ khoá vào bộ lọc chung rồi nạp lại. */
function searchNow() {
  void page.setKeyword(keyword.value);
}

const load = () => page.reload();

void load();

// Quay lại màn (KeepAlive giữ state) → nạp lại dữ liệu cho khỏi cũ.
useKeepAliveRefresh(load);

// Phím tắt: chỉ khi màn này đang hiện và không có hộp thoại nào mở.
function onKeydown(e: KeyboardEvent) {
  if (e.key !== "F8" && e.key !== "F9") return;
  if (document.querySelector(".p-dialog-mask, .p-confirmdialog-mask")) return;
  const target = e.target as HTMLElement | null;
  if (target && ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName)) return;
  if (!selected.value) return;
  e.preventDefault();
  if (e.key === "F8") openExport(selected.value.invoice);
  else openLink(selected.value.invoice);
}

onMounted(() => window.addEventListener("keydown", onKeydown));
onBeforeUnmount(() => window.removeEventListener("keydown", onKeydown));
</script>

<template>
  <div class="space-y-4">
    <Toolbar>
      <template #start>
        <div class="flex items-center gap-2">
          <i-mdi-send-outline class="text-xl text-violet-500" />
          <h3 class="font-semibold">Chờ xuất HĐĐT</h3>
        </div>
      </template>
      <template #end>
        <div class="flex flex-wrap items-center gap-2">
          <span class="text-xs text-gray-500">
            {{ readyCount }} hóa đơn sẵn sàng chép · {{ copiedCount }} đã chép ·
            {{ blockedCount }} cần sửa
          </span>
          <Button
            icon="pi pi-refresh"
            text
            rounded
            size="small"
            aria-label="Nạp lại hàng chờ xuất"
            v-tooltip="'Nạp lại'"
            @click="load"
          />
        </div>
      </template>
    </Toolbar>

    <Card>
      <template #content>
        <div class="mb-3 flex flex-wrap items-end gap-3 text-sm">
          <IconField>
            <InputIcon class="pi pi-search" />
            <InputText
              v-model="keyword"
              placeholder="Số hóa đơn, khách hàng, MST…"
              size="small"
              @keyup.enter="searchNow()"
              class="w-64"
            />
          </IconField>
          <div>
            <label class="mb-1 block text-xs text-gray-500">Trạng thái</label>
            <SelectButton
              v-model="statusFilter"
              @change="applyFilters()"
              :options="[
                { label: 'Tất cả', value: 'all' },
                { label: 'Nháp', value: 'draft' },
                { label: 'Đã chép', value: 'exported' },
              ]"
              option-label="label"
              option-value="value"
              :allow-empty="false"
              size="small"
            />
          </div>
          <div>
            <label class="mb-1 block text-xs text-gray-500">Từ ngày</label>
            <DatePicker
              v-model="from"
              dateFormat="dd/mm/yy"
              size="small"
              show-icon
              @date-select="applyFilters()"
            />
          </div>
          <div>
            <label class="mb-1 block text-xs text-gray-500">Đến ngày</label>
            <DatePicker
              v-model="to"
              dateFormat="dd/mm/yy"
              size="small"
              show-icon
              @date-select="applyFilters()"
            />
          </div>
        </div>

        <DataTable
          :value="items"
          :loading="loading"
          :lazy="true"
          :paginator="true"
          :rows="page.rowsPerPage.value"
          :first="page.first.value"
          :totalRecords="total"
          :rows-per-page-options="page.pageSizes"
          @page="page.onPage"
          data-key="invoice.id"
          size="small"
          striped-rows
          highlight-on-select
          :row-class="() => ''"
          class="text-sm"
          @row-select="(e: any) => (selectedId = e.data.invoice.id)"
          @row-unselect="() => (selectedId = null)"
        >
          <Column selection-mode="single" header-style="width: 2.5rem" />
          <Column field="invoice.number" header="Số HĐ">
            <template #body="{ data }">
              <div class="font-medium">{{ data.invoice.number }}</div>
              <div v-if="data.edited_after_export" class="text-xs text-amber-700">
                ⚠ Đã sửa sau lần chép — cần sửa lại bên kia
              </div>
            </template>
          </Column>
          <Column field="invoice.date" header="Ngày" />
          <Column field="invoice.customer" header="Khách hàng" />
          <Column header="MST">
            <template #body="{ data }">{{ data.invoice.customer_tax_code || "—" }}</template>
          </Column>
          <Column header="Tổng tiền" align="right">
            <template #body="{ data }">{{ fmtVnd(data.invoice.total) }}</template>
          </Column>
          <Column header="Trạng thái">
            <template #body="{ data }">
              <Tag
                :value="statusText[data.invoice.status] ?? data.invoice.status"
                :severity="data.invoice.status === 'exported' ? 'info' : 'secondary'"
              />
            </template>
          </Column>
          <Column header="Phiếu xuất">
            <template #body="{ data }">
              <span v-if="data.invoice.voucher_no">{{ data.invoice.voucher_no }}</span>
              <span v-else class="text-xs text-amber-700" title="Doanh thu chưa vào sổ">
                ⚠ chưa có
              </span>
            </template>
          </Column>
          <Column header="Kiểm tra">
            <template #body="{ data }">
              <div v-if="data.error_count" class="text-xs text-red-600">
                <i class="pi pi-times-circle" /> {{ data.error_count }} lỗi
                <div
                  v-for="c in data.checks.filter((c: any) => c.level === 'error')"
                  :key="c.message"
                  class="max-w-72 truncate"
                  :title="c.message"
                >
                  {{ c.message }}
                </div>
              </div>
              <div v-else-if="data.warn_count" class="text-xs text-amber-700">
                <i class="pi pi-exclamation-triangle" /> {{ data.warn_count }} cảnh báo
                <div
                  v-for="c in data.checks.filter((c: any) => c.level === 'warn')"
                  :key="c.message"
                  class="max-w-72 truncate"
                  :title="c.message"
                >
                  {{ c.message }}
                </div>
              </div>
              <span v-else class="text-xs text-emerald-700">
                <i class="pi pi-check-circle" /> Sẵn sàng chép
              </span>
            </template>
          </Column>
          <Column v-if="auth.canAccounting" header="Thao tác" header-style="width: 9rem">
            <template #body="{ data }">
              <div class="flex items-center justify-center gap-1">
                <Button
                  icon="pi pi-file-pdf"
                  text
                  rounded
                  size="small"
                  aria-label="Xem PDF hóa đơn"
                  v-tooltip="'Xem PDF hóa đơn'"
                  @click="openPdf(data.invoice)"
                />
                <Button
                  icon="pi pi-copy"
                  text
                  rounded
                  size="small"
                  aria-label="Chép sang dịch vụ HĐĐT khác"
                  v-tooltip="'Chép sang dịch vụ HĐĐT khác (F8)'"
                  :disabled="data.error_count > 0"
                  @click="openExport(data.invoice)"
                />
                <Button
                  v-if="data.invoice.status === 'draft'"
                  icon="pi pi-file-edit"
                  text
                  rounded
                  size="small"
                  aria-label="Sửa hóa đơn nháp"
                  v-tooltip="'Sửa hóa đơn nháp'"
                  severity="secondary"
                  @click="openDraft(data.invoice)"
                />
                <Button
                  v-if="data.invoice.status === 'draft'"
                  icon="pi pi-trash"
                  text
                  rounded
                  size="small"
                  aria-label="Xoá hóa đơn nháp"
                  v-tooltip="'Xoá hóa đơn nháp'"
                  severity="danger"
                  @click="removeItem(data.invoice)"
                />
                <Button
                  icon="pi pi-link"
                  text
                  rounded
                  size="small"
                  aria-label="Ghi số HĐĐT đã phát hành"
                  v-tooltip="'Ghi số HĐĐT đã phát hành (F9)'"
                  severity="success"
                  @click="openLink(data.invoice)"
                />
              </div>
            </template>
          </Column>
          <template #empty>
            <EmptyState
              text="Không còn hóa đơn nào chờ xuất. Hóa đơn nháp mới lập sẽ hiện ở đây."
              icon="pi pi-check-circle"
            />
          </template>
        </DataTable>

        <p class="mt-2 text-xs text-gray-500">
          F8 chép dòng đang chọn · F9 ghi số HĐĐT sau khi phát hành. Hóa đơn đã phát hành biến khỏi
          danh sách này.
        </p>
      </template>
    </Card>

    <InvoiceExportDialog v-model:visible="exportVisible" :invoice="exportTarget" @done="load" />
    <InvoiceLinkDialog v-model:visible="linkVisible" :invoice="linkTarget" @done="load" />
    <InvoiceDraftDialog v-model:visible="draftVisible" :edit-invoice="draftTarget" @done="load" />
    <InvoicePdfDialog v-model:visible="pdfVisible" :detail-json="pdfDetail" :title="pdfTitle" />
  </div>
</template>
