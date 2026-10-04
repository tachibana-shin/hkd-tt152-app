<script setup lang="ts">
/**
 * Đồng bộ hóa đơn mua từ cổng HĐĐT → hóa đơn chính thức + phiếu nhập kho.
 *
 * Màn này đứng cùng cấp với "HĐĐT" trong menu bên trái. Cấu hình tài khoản cổng
 * nằm ở Hddt.vue, tra cứu hóa đơn nằm ở HddtLookup.vue; phần lấp hóa đơn mua nằm
 * ở đây để không phải cuộn qua danh sách dài.
 */
import { api } from "@/db";
import { useBusinessStore } from "@/stores/business";
import { usePortalSession } from "@/composables/usePortalSession";
import type { HddtSavedXml, HddtSyncPreview, HddtSyncRow } from "@/types";
import { downloadZip } from "@/utils/download";
import { useKeepAliveRefresh } from "@/composables/useKeepAliveRefresh";

const toast = useToast();
const confirm = useConfirm();
const business = useBusinessStore();

// Phiên cổng: chỉ biết đã đăng nhập chưa + nút đăng nhập (captcha tay thuộc màn HĐĐT).
const portal = usePortalSession();
const { statusLoaded, loggingIn, loggedIn } = portal;

// ─── Hiển thị ───
function toastError(summary: string, e: unknown) {
  toast.add({ severity: "error", summary, detail: String(e), life: 5000 });
}

function fmtMoney(v: unknown) {
  if (v === null || v === undefined || v === "") return "—";
  const n = typeof v === "number" ? v : Number(v);
  if (Number.isNaN(n)) return String(v);
  return n.toLocaleString("vi-VN");
}

// ─── Đồng bộ hóa đơn mua từ cổng HĐĐT ───
// Quét cổng 1 lần → cache theo ngày; xem trước (đọc cache, không mạng) →
// nhập kho (tạo mặt hàng/NCC/phiếu). Ngày hôm nay được quét nhưng không cache:
// hóa đơn hôm nay
// vẫn còn phát sinh nên mỗi lượt quét đều lấy lại. Mốc bắt đầu = hddt_start_date.
const syncFrom = ref<Date | null>(null);
const syncTo = ref<Date | null>(null);
const syncKinds = ref<{ regular: boolean; cashRegister: boolean }>({
  regular: true,
  cashRegister: true,
});
const syncRows = ref<HddtSyncRow[]>([]);
const syncSummary = ref<HddtSyncPreview["summary"] | null>(null);
const syncScanning = ref(false);
const syncImporting = ref(false);
const syncScanMsg = ref("");
/** File XML đã lưu theo khóa hóa đơn — để bảng hiện "Đã lưu" + chọn ra ZIP. */
const xmlSaved = ref<Record<string, HddtSavedXml>>({});
const savingXml = ref<Set<string>>(new Set());
const exportingXml = ref(false);

/** Khóa tra trạng thái XML — 4 tham số định danh hóa đơn (bỏ mã mẫu số vì
 *  người dùng nhìn tên file theo ký hiệu + số). */
function xmlKey(direction: string, kind: string, nbmst: string, khhdon: string, shdon: string) {
  return [direction, kind, nbmst, khhdon, shdon].join("|");
}

/** Khóa XML của 1 dòng bảng Đồng bộ (chiều mua). */
function rowXmlKey(row: HddtSyncRow) {
  return xmlKey("purchase", row.portal_kind, row.nbmst, row.khhdon, row.shdon);
}

/** Bản ghi XML đã lưu của dòng này (`undefined` = chưa có file). */
function rowXmlSaved(row: HddtSyncRow): HddtSavedXml | undefined {
  return xmlSaved.value[rowXmlKey(row)];
}

/** Đang tải file XML của dòng này (tránh bấm 2 lần). */
function rowXmlBusy(row: HddtSyncRow) {
  return savingXml.value.has(rowXmlKey(row));
}

async function loadXmlIndex() {
  try {
    const rows = await api.hddtListXml();
    const map: Record<string, HddtSavedXml> = {};
    for (const r of rows) {
      map[xmlKey(r.direction, r.kind, r.nbmst, r.khhdon, r.shdon)] = r;
    }
    xmlSaved.value = map;
  } catch {
    // Không có danh sách thì bảng hiện "Tải XML" hết — không chặn luồng chính.
    xmlSaved.value = {};
  }
}

/** Id các file XML của **dòng đang xem** — đúng phạm vi người dùng muốn nộp. */
function displayedXmlIds(): number[] {
  return syncRows.value
    .map((row) => xmlSaved.value[rowXmlKey(row)]?.id)
    .filter((id): id is number => typeof id === "number");
}

/** Số file sẽ vào ZIP — hiện ngay trên nhãn nút để biết sắp tải bao nhiêu. */
const exportableXmlCount = computed(() => displayedXmlIds().length);

/** Tên ZIP theo ngày — cùng mẫu đặt tên file với các mục xuất khác của app. */
function zipFileName() {
  const d = new Date();
  const stamp = `${d.getFullYear()}${String(d.getMonth() + 1).padStart(2, "0")}${String(d.getDate()).padStart(2, "0")}`;
  return `hddt_xml_mua_${stamp}.zip`;
}

/** Nút "Tải ZIP XML" — gộp file XML của các dòng đang xem để nộp/gửi. */
async function exportXmlZip() {
  const ids = displayedXmlIds();
  if (exportingXml.value) return;
  if (ids.length === 0) {
    toast.add({
      severity: "info",
      summary: "Chưa có file XML nào trong danh sách này",
      detail: 'Bấm "Tải XML" từng dòng hoặc "Quét cổng" để lưu tự động trước.',
      life: 4500,
    });
    return;
  }
  exportingXml.value = true;
  try {
    downloadZip(zipFileName(), await api.hddtExportXmlZip(ids));
    toast.add({
      severity: "success",
      summary: `Đã tải ZIP ${ids.length} file XML`,
      detail: `Tệp ${zipFileName()} — gửi hoặc lưu trữ theo quy định.`,
      life: 4000,
    });
  } catch (e) {
    toastError("Không gộp được ZIP file XML", e);
  } finally {
    exportingXml.value = false;
  }
}

/** Nút "Tải XML" từng dòng — tải từ cổng rồi ghi vào CSDL + thư mục hồ sơ. */
async function saveRowXml(row: HddtSyncRow) {
  const key = rowXmlKey(row);
  if (savingXml.value.has(key)) return;
  savingXml.value.add(key);
  savingXml.value = new Set(savingXml.value);
  try {
    const saved = await api.hddtSaveXml({
      direction: "purchase",
      kind: row.portal_kind as "regular" | "cash-register",
      nbmst: row.nbmst,
      khmshdon: row.khmshdon,
      khhdon: row.khhdon,
      shdon: row.shdon,
      portalId: row.portal_id,
    });
    // Tải lại toàn bộ chỉ mục để lấy `id` (cần cho "Tải ZIP XML").
    await loadXmlIndex();
    toast.add({
      severity: "success",
      summary: saved.already ? "File XML đã có từ trước" : "Đã lưu file XML",
      detail: `${row.khhdon} ${row.shdon} → ${saved.file_name}`,
      life: 3500,
    });
  } catch (e) {
    toastError("Không tải được file XML", e);
  } finally {
    savingXml.value.delete(key);
    savingXml.value = new Set(savingXml.value);
  }
}

function syncIsoDate(d: Date | null) {
  if (!d) return undefined;
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

/** Mốc bắt đầu = hddt_start_date của hộ; mốc cuối = hôm qua. */
/** Mốc cuối mặc định = hôm nay (hôm nay luôn được quét nhưng không cache).
 *  Mốc đầu = ngày bắt đầu dùng HĐĐT khai trong popup cấu hình HKD. */
function syncDefaultRange() {
  const to = new Date();
  const start = business.config?.hddt_start_date
    ? new Date(`${business.config.hddt_start_date}T00:00:00`)
    : null;
  const from = start && !Number.isNaN(start.getTime()) ? start : new Date(to);
  return { from, to };
}

const syncKindList = computed(() => {
  const out: string[] = [];
  if (syncKinds.value.regular) out.push("regular");
  if (syncKinds.value.cashRegister) out.push("cash-register");
  return out;
});

async function loadSyncPreview(retryFailed = false) {
  syncScanning.value = true;
  syncScanMsg.value = "";
  try {
    const res = await api.hddtSyncPreview({
      from: syncIsoDate(syncFrom.value),
      to: syncIsoDate(syncTo.value),
      retryFailed,
    });
    syncRows.value = res.rows;
    syncSummary.value = res.summary;
    await loadXmlIndex();
  } catch (e) {
    toastError("Lỗi tải danh sách hóa đơn", e);
  } finally {
    syncScanning.value = false;
  }
}

/** Mô tả số liệu tự động lưu XML kèm thông báo quét/nhập (nếu có). */
function xmlFillText(f?: { saved: number; failed: number; first_error: string }) {
  if (!f) return "";
  if (f.saved === 0 && f.failed === 0) return " · XML đã lưu đủ";
  let s = ` · lưu XML ${f.saved}`;
  if (f.failed > 0) s += `, lỗi ${f.failed} (${f.first_error})`;
  return s;
}

async function onSyncScan() {
  if (!syncKindList.value.length) {
    toastError("Chưa chọn loại hóa đơn", "Chọn ít nhất một loại để quét cổng.");
    return;
  }
  syncScanning.value = true;
  syncScanMsg.value = "";
  try {
    const s = await api.hddtSyncScan({
      from: syncIsoDate(syncFrom.value),
      to: syncIsoDate(syncTo.value),
      kinds: syncKindList.value,
    });
    // Ngày đã có cache bị bỏ qua (không gọi lại cổng) — nói rõ để không tưởng quét thiếu.
    syncScanMsg.value =
      `Đã quét ${s.days_scanned} ngày` +
      (s.days_cached > 0 ? ` (bỏ qua ${s.days_cached} ngày đã có cache)` : "") +
      (s.details_retried > 0 ? ` · thử lại ${s.details_retried} HĐ lỗi` : "") +
      (s.days_today > 0 ? " · hôm nay (không cache)" : "") +
      ` · hóa đơn mới ${s.invoices_new} · có dòng hàng ${s.details_ok} · lỗi ${s.details_failed}` +
      ` · cần xử lý ${s.need_manual}` +
      xmlFillText(s.xml);
    // Không bật `retryFailed` ở đây: scan đã tự thử lại các HĐ lỗi chi tiết. Bật
    // ở đây sẽ xoá lỗi và biến HĐ chưa có dòng hàng thành "chờ nhập kho" → bấm
    // nhập kho sẽ báo "Hóa đơn không có dòng hàng".
    await loadSyncPreview();
  } catch (e) {
    toastError("Lỗi quét cổng HĐĐT", e);
  } finally {
    syncScanning.value = false;
  }
}

async function onSyncImport(ids?: number[]) {
  syncImporting.value = true;
  try {
    const res = await api.hddtSyncImport({ ids: ids ?? [] });
    const bad = res.results.filter((r) => !r.ok);
    const xmlNote = xmlFillText(res.xml);
    if (res.imported > 0) {
      toast.add({
        severity: "success",
        summary: `Đã tạo ${res.imported} phiếu nhập`,
        detail:
          (bad.length
            ? `${res.failed} hóa đơn lỗi: ${bad[0].message}`
            : "Tất cả hóa đơn đã được liên kết với phiếu nhập.") + xmlNote,
      });
    } else if (res.failed > 0) {
      toastError("Không tạo được phiếu nhập", bad[0]?.message ?? "Không rõ nguyên nhân");
    } else {
      toast.add({
        severity: "info",
        summary: "Không có hóa đơn nào chờ nhập kho",
        detail: xmlNote || undefined,
      });
    }
    await loadSyncPreview();
  } catch (e) {
    toastError("Lỗi nhập kho hóa đơn", e);
  } finally {
    syncImporting.value = false;
  }
}

/** Xoá cache đồng bộ để quét lại từ đầu (hóa đơn đã nhập kho giữ nguyên). */
function onSyncClearCache() {
  confirm.require({
    header: "Xoá cache đồng bộ",
    message:
      "Xoá toàn bộ hóa đơn chưa nhập kho và các dấu ngày đã quét, để lần sau " +
      'bấm "Quét cổng" tải lại từ cổng. Hóa đơn đã nhập kho (có phiếu) không bị xoá.',
    icon: "pi pi-exclamation-triangle",
    acceptLabel: "Xoá cache",
    rejectLabel: "Huỷ",
    acceptProps: { severity: "danger" },
    accept: async () => {
      syncScanning.value = true;
      try {
        const out = await api.hddtSyncClearCache();
        toast.add({
          severity: "success",
          summary: "Đã xoá cache đồng bộ",
          detail: `Xoá ${out.invoicesDeleted} hóa đơn chưa nhập · ${out.daysDeleted} ngày đã quét`,
          life: 4000,
        });
        syncScanMsg.value = "";
        await loadSyncPreview();
      } catch (e) {
        toastError("Không xoá được cache", e);
      } finally {
        syncScanning.value = false;
      }
    },
  });
}

function syncStatusSeverity(s: string) {
  return s === "imported" ? "success" : s === "manual" ? "warn" : "info";
}

function syncStatusLabel(s: string) {
  return s === "imported" ? "Đã nhập kho" : s === "manual" ? "Cần xử lý" : "Chờ nhập kho";
}

function syncKindLabel(k: string) {
  return k === "cash-register" ? "Máy tính tiền" : "HĐ điện tử";
}

// ─── Xem phiếu nhập kho đã tạo từ hóa đơn ───
const viewVoucherNo = ref("");
const viewVoucherVisible = ref(false);

function openVoucher(no: string) {
  viewVoucherNo.value = no;
  viewVoucherVisible.value = true;
}

// ─── Xem PDF hóa đơn điện tử ───
// Chi tiết đọc từ DB theo portal_id (đã cache khi quét, hoặc hóa đơn đã nhập
// kho) nên không cần gọi cổng.
const pdfVisible = ref(false);
const pdfDetail = ref("");
const pdfTitle = ref("");

async function openInvoicePdf(row: HddtSyncRow) {
  try {
    pdfDetail.value = await api.hddtSyncInvoiceDetail(row.portal_id);
  } catch (e) {
    toastError("Không lấy được chi tiết hóa đơn", e);
    return;
  }
  pdfTitle.value = `Hóa đơn ${row.khhdon} ${row.shdon}`;
  pdfVisible.value = true;
}

// Mốc bắt đầu = hddt_start_date của hộ → phải nạp config trước rồi mới tính;
// mốc cuối = hôm nay (hôm nay quét nhưng không cache). Không dùng onMounted vì
// không có thao tác ref/DOM ở đây.
// Mốc ngày chỉ đặt một lần lúc mount: quay lại màn không được xoá khoảng
// ngày người dùng đang lọc.
async function reload() {
  await loadSyncPreview();
}

void (async () => {
  await business.load();
  const range = syncDefaultRange();
  syncFrom.value = range.from;
  syncTo.value = range.to;
  // Cache đồng bộ là dữ liệu cục bộ → tải luôn để thấy trạng thái trước khi quét.
  await reload();
})();

// Quay lại màn (KeepAlive giữ state) → nạp lại dữ liệu cho khỏi cũ.
useKeepAliveRefresh(reload);
// App đã đảm bảo phiên lúc khởi động; gọi thêm ở đây để màn này tự đăng nhập
// nếu phiên đã mất (đăng xuất, token hết hạn) và mở popup nhập captcha tay.
void (async () => {
  const result = await portal.ensureSession();
  if (result === "need_manual") await portal.openManual();
  else if (result === "logged_in") portal.startHeartbeat();
})();
</script>

<template>
  <div class="space-y-4">
    <Toolbar>
      <template #start>
        <i class="pi pi-cloud-download mr-2 text-xl text-primary-500" />
        <h3 class="font-semibold">Đồng bộ hóa đơn mua</h3>
      </template>
      <template #end>
        <Button
          v-if="statusLoaded && !loggedIn"
          label="Đăng nhập cổng HĐĐT"
          icon="pi pi-sign-in"
          size="small"
          :loading="loggingIn"
          @click="portal.loginPortal"
        />
      </template>
    </Toolbar>

    <SectionCard>
      <p v-if="statusLoaded && !loggedIn" class="mb-3 text-sm text-orange-600">
        <i class="pi pi-exclamation-triangle mr-1" />Chưa đăng nhập cổng HĐĐT. Bấm
        <b>Đăng nhập cổng HĐĐT</b> ở góc trên bên phải; nếu cổng bắt nhập captcha thì mở màn
        <b>HĐĐT</b> để nhập tay. Hóa đơn đã lưu cache vẫn xem được.
      </p>

      <p class="mb-3 text-xs text-gray-500">
        Quét hóa đơn mua từ cổng HĐĐT và tạo phiếu nhập kho tự động. Ngày đã quét được lưu lại (hóa
        đơn ngày đã qua không đổi) — lần sau app không gọi lại cổng cho những ngày đó. Hôm nay không
        quét lại. Mặt hàng khớp theo
        <b>tên + đơn vị tính</b>; chưa có thì app tự tạo. Tên dòng dịch vụ (phí) giữ nguyên — vì tên
        có kẹp kỳ/tháng nên mỗi kỳ là một mặt hàng riêng; số mặt hàng sẽ tạo mới luôn hiện ở cột
        "Dòng / HH mới" để bạn xem trước khi nhập kho.
      </p>
      <div class="mb-3 flex flex-wrap items-end gap-3">
        <div>
          <label class="mb-1 block text-xs text-gray-500">Từ ngày lập</label>
          <DatePicker v-model="syncFrom" dateFormat="dd/mm/yy" class="w-40" />
        </div>
        <div>
          <label class="mb-1 block text-xs text-gray-500">Đến ngày lập</label>
          <DatePicker v-model="syncTo" dateFormat="dd/mm/yy" class="w-40" />
        </div>
        <div class="flex items-center gap-3 pb-1">
          <label class="flex items-center gap-2 text-xs text-gray-600">
            <Checkbox v-model="syncKinds.regular" binary />
            HĐ điện tử
          </label>
          <label class="flex items-center gap-2 text-xs text-gray-600">
            <Checkbox v-model="syncKinds.cashRegister" binary />
            Máy tính tiền
          </label>
        </div>
        <Button label="Quét cổng" icon="pi pi-sync" :loading="syncScanning" @click="onSyncScan" />
        <Button
          label="Xoá cache"
          icon="pi pi-trash"
          severity="secondary"
          :disabled="syncScanning || !syncSummary || syncSummary.total === 0"
          @click="onSyncClearCache"
        />
        <!-- Gộp file XML của đúng các dòng đang xem thành 1 ZIP để nộp/gửi. -->
        <Button
          :label="exportableXmlCount > 0 ? `Tải ZIP XML (${exportableXmlCount})` : 'Tải ZIP XML'"
          icon="pi pi-download"
          severity="secondary"
          :loading="exportingXml"
          :disabled="syncScanning"
          :aria-label="`Tải ZIP gộp file XML hóa đơn mua (${exportableXmlCount} file)`"
          v-tooltip="'Gộp file XML của các hóa đơn trong danh sách thành 1 file ZIP'"
          @click="exportXmlZip"
        />
        <Button
          v-if="syncSummary && syncSummary.pending > 0"
          :label="`Nhập tất cả (${syncSummary.pending})`"
          icon="pi pi-download"
          :loading="syncImporting"
          @click="onSyncImport()"
        />
      </div>

      <div class="mb-2 flex flex-wrap items-center gap-3 text-sm">
        <span v-if="syncScanMsg" class="text-gray-600">
          <i class="pi pi-info-circle mr-1" />{{ syncScanMsg }}
        </span>
        <span v-else-if="syncSummary" class="text-gray-500">
          Có {{ syncSummary.total }} hóa đơn trong cache · chờ nhập {{ syncSummary.pending }} · đã
          nhập {{ syncSummary.imported }} · cần xử lý {{ syncSummary.manual }} · sẽ tạo mới
          {{ syncSummary.new_products }} mặt hàng
        </span>
        <span v-else class="text-gray-400">
          Bấm "Quét cổng" để lấy hóa đơn mua về. Xem trước trước khi nhập kho.
        </span>
      </div>

      <AppDataTable
        :value="syncRows"
        :loading="syncScanning"
        stripedRows
        scrollable
        scrollHeight="420px"
        size="small"
      >
        <Column header="Ngày lập" :style="{ width: '7rem' }">
          <template #body="{ data }">{{ data.posting_date || "—" }}</template>
        </Column>
        <Column header="Loại" :style="{ width: '8rem' }">
          <template #body="{ data }">{{ syncKindLabel(data.portal_kind) }}</template>
        </Column>
        <Column header="Số HĐ" :style="{ width: '9rem' }">
          <template #body="{ data }">
            <span class="inline-flex items-center gap-1">
              {{ data.khhdon }} {{ data.shdon }}
              <Button
                icon="pi pi-file-pdf"
                text
                size="small"
                class="!p-1 text-gray-500"
                :aria-label="`Xem PDF hóa đơn ${data.khhdon} ${data.shdon}`"
                v-tooltip="'Xem PDF hóa đơn'"
                @click="openInvoicePdf(data)"
              />
            </span>
          </template>
        </Column>
        <Column header="Người bán">
          <template #body="{ data }">
            <div class="text-xs leading-5">
              <div>{{ data.nbten || "—" }}</div>
              <div class="text-gray-500">MST {{ data.nbmst || "—" }}</div>
            </div>
          </template>
        </Column>
        <Column header="Chiết khấu" align="right" :style="{ width: '9rem' }">
          <template #body="{ data }">
            <span :class="data.ttcktmai > 0 ? 'text-emerald-600' : 'text-gray-400'">
              {{ data.ttcktmai > 0 ? fmtMoney(data.ttcktmai) : "—" }}
            </span>
          </template>
        </Column>
        <Column header="Tổng TT" align="right">
          <template #body="{ data }">{{ fmtMoney(data.tgtttbso) }}</template>
        </Column>
        <Column header="Dòng / HH mới" :style="{ width: '8rem' }">
          <template #body="{ data }">{{ data.line_count }} / {{ data.new_product_count }}</template>
        </Column>
        <!-- File XML hóa đơn: quy định phải lưu trữ → hiện rõ hóa đơn nào đã có
             file trong thư mục hồ sơ, thiếu thì bấm tải ngay tại đây. -->
        <Column header="File XML" :style="{ width: '8.5rem' }">
          <template #body="{ data }">
            <span
              v-if="rowXmlSaved(data)"
              class="inline-flex items-center gap-1 text-xs text-emerald-700"
              :title="rowXmlSaved(data)?.file_name"
            >
              <i class="pi pi-check-circle" />
              Đã lưu
            </span>
            <Button
              v-else
              :label="rowXmlBusy(data) ? 'Đang tải' : 'Tải XML'"
              icon="pi pi-download"
              size="small"
              severity="secondary"
              :loading="rowXmlBusy(data)"
              :aria-label="`Tải file XML hóa đơn ${data.khhdon} ${data.shdon}`"
              @click="saveRowXml(data)"
            />
          </template>
        </Column>
        <!-- Ba cột phải cùng `frozen align-frozen="right"`: PrimeVue tính
             `inset-inline-end` theo cột frozen KẾ TIẾP nên pin cùng lúc mới không
             chồng lên cột thao tác (pin riêng 2 cột sẽ giấu mất nút "Nhập kho"). -->
        <Column header="Trạng thái" :frozen="true" align-frozen="right" :style="{ width: '9rem' }">
          <template #body="{ data }">
            <Tag
              :value="syncStatusLabel(data.status)"
              :severity="syncStatusSeverity(data.status)"
              :title="data.skip_reason || data.detail_error"
            />
          </template>
        </Column>
        <Column header="Phiếu nhập" :frozen="true" align-frozen="right" :style="{ width: '8rem' }">
          <template #body="{ data }">
            <Button
              v-if="data.voucher_no"
              :label="data.voucher_no"
              link
              class="!p-0 text-xs text-green-700"
              :aria-label="`Xem phiếu ${data.voucher_no}`"
              v-tooltip="'Xem phiếu nhập'"
              @click="openVoucher(data.voucher_no)"
            />
            <span v-else class="text-xs text-gray-400">—</span>
          </template>
        </Column>
        <Column
          v-if="!syncImporting"
          header=""
          :frozen="true"
          align-frozen="right"
          :style="{ width: '7rem' }"
        >
          <template #body="{ data }">
            <Button
              v-if="data.status === 'pending'"
              label="Nhập kho"
              size="small"
              severity="secondary"
              @click="onSyncImport([data.id])"
            />
          </template>
        </Column>
      </AppDataTable>
    </SectionCard>

    <VoucherViewDialog v-model:visible="viewVoucherVisible" :voucher-no="viewVoucherNo" />

    <InvoicePdfDialog v-model:visible="pdfVisible" :detail-json="pdfDetail" :title="pdfTitle" />
  </div>
</template>
