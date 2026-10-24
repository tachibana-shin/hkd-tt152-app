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
import { useSettingsStore } from "@/stores/settings";
import { usePortalSession } from "@/composables/usePortalSession";
import { restartAutoTasks } from "@/composables/useAutoTasks";
import JobProgressDialog from "@/components/JobProgressDialog.vue";
import type { AppSettings, HddtSavedXml, HddtSyncPreview, HddtSyncRow, JobProgress } from "@/types";
import { downloadZip } from "@/utils/download";
import { useKeepAliveRefresh } from "@/composables/useKeepAliveRefresh";

const toast = useToast();
const confirm = useConfirm();
const business = useBusinessStore();
const appSettings = useSettingsStore();

// Phiên cổng: chỉ biết đã đăng nhập chưa + nút đăng nhập (captcha tay thuộc màn HĐĐT).
const portal = usePortalSession();
const { statusLoaded, loggingIn, loggedIn } = portal;

// ─── Tự động nhập hóa đơn (chạy nền) ───
// Cài đặt thật nằm ở bảng app_setting, dùng chung với màn Cài đặt và composable
// useAutoTasks. Đặt ngay màn Đồng bộ để người dùng thấy được (issue #7: trước
// đây chỉ nằm trong Cài đặt nên tưởng app không có tính năng này).
const autoForm = reactive({
  enabled: true,
  importAfter: true,
  intervalMin: 15,
});
const autoLastRunAt = ref("");
const autoLastResult = ref("");
const autoSaving = ref(false);

function fillAutoForm(s: AppSettings | null) {
  if (!s) return;
  autoForm.enabled = s.auto_sync_enabled;
  autoForm.importAfter = s.auto_sync_import;
  autoForm.intervalMin = s.auto_sync_interval_min;
  autoLastRunAt.value = s.auto_sync_last_run_at;
  autoLastResult.value = s.auto_sync_last_result;
}

async function loadAutoSettings() {
  try {
    // force: việc chạy nền ghi auto_sync_last_* sau khi store đã cache.
    await appSettings.load(true);
    fillAutoForm(appSettings.settings);
  } catch {
    // Không đọc được cài đặt thì giữ mặc định — màn đồng bộ vẫn dùng được.
  }
}

/** `auto_sync_last_run_at` là ISO có Z (new Date().toISOString()) → giờ máy. */
function fmtAutoRun(iso: string) {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getHours())}:${p(d.getMinutes())} ${p(d.getDate())}/${p(d.getMonth() + 1)}/${d.getFullYear()}`;
}

async function saveAutoSettings() {
  if (autoSaving.value) return;
  autoSaving.value = true;
  try {
    const interval = Math.min(1440, Math.max(1, Math.round(autoForm.intervalMin) || 15));
    autoForm.intervalMin = interval;
    await appSettings.save({
      auto_sync_enabled: autoForm.enabled ? "1" : "0",
      auto_sync_import: autoForm.importAfter ? "1" : "0",
      auto_sync_interval_min: String(interval),
    });
    fillAutoForm(appSettings.settings);
    // Đổi chu kỳ/bật tắt phải có hiệu lực ngay, không cần mở lại app.
    await restartAutoTasks();
    toast.add({
      severity: "success",
      summary: "Đã lưu cài đặt tự động nhập hóa đơn",
      detail: autoForm.enabled
        ? `Chạy khi mở app và lặp lại mỗi ${interval} phút.`
        : "Đã tắt tự động — vẫn có thể bấm “Quét cổng” thủ công.",
      life: 4000,
    });
  } catch (e) {
    toastError("Không lưu được cài đặt tự động", e);
  } finally {
    autoSaving.value = false;
  }
}

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
/** Ngày cần quét LẠI (bỏ dấu "đã quét" + xoá cache của đúng ngày đó) — hóa
 *  đơn ký trễ / cổng cập nhật chậm thường rơi vào trường hợp này. */
const rescanDay = ref<Date | null>(null);
const syncKinds = ref<{ regular: boolean; cashRegister: boolean }>({
  regular: true,
  cashRegister: true,
});
const syncRows = ref<HddtSyncRow[]>([]);
const syncSummary = ref<HddtSyncPreview["summary"] | null>(null);
const syncScanning = ref(false);
const syncImporting = ref(false);
const syncScanMsg = ref("");

// ─── Nhật ký realtime khi bấm "Quét cổng" ───
// Quét rất lâu (nhiều ngày × 2 loại + từng chi tiết + từng file XML) nên backend
// ghi từng bước vào ô tiến độ chung; frontend hỏi lại mỗi 300ms để biết hóa đơn
// nào đang chờ cổng trả lời. Không dùng sự kiện Tauri vì bản web không nhận được.
const scanProgress = ref<JobProgress | null>(null);
const scanProgressOpen = ref(false);
const scanProgressDismissed = ref(false);
const scanPreLog = ref<string[]>([]);
let scanProgressTimer: ReturnType<typeof setInterval> | null = null;

/** Mã việc riêng cho lượt quét này. Không xài `crypto.randomUUID` vì bản desktop
 *  chạy ở scheme `tauri://` không phải context bảo mật nên bị chặn ở một số WebView. */
const newScanJobId = () =>
  `scan-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;

async function pollScanProgress(jobId: string) {
  try {
    const p = await api.getJobProgress(jobId);
    if (p) scanProgress.value = p;
  } catch {
    // Không đọc được tiến độ thì vẫn chờ lệnh quét trả lời — không làm hỏng việc chính.
  }
}

function stopScanProgress() {
  if (scanProgressTimer) {
    clearInterval(scanProgressTimer);
    scanProgressTimer = null;
  }
}

function startScanProgress(jobId: string) {
  stopScanProgress();
  scanProgress.value = {
    id: jobId,
    label: "Quét cổng HĐĐT",
    step: "Đang gửi yêu cầu…",
    done: 0,
    total: 0,
    finished: false,
    error: null,
    log: [],
  };
  scanProgressDismissed.value = false;
  scanPreLog.value = [
    "Bắt đầu quét cổng HĐĐT…",
    "Mỗi trang danh sách, từng chi tiết hóa đơn và từng file XML đều ghi thời gian — dòng nào lâu sẽ thấy ngay.",
  ];
  scanProgressOpen.value = true;
  void pollScanProgress(jobId);
  scanProgressTimer = setInterval(() => void pollScanProgress(jobId), 300);
}

/** Đang chạy → "Chạy nền" (mở lại bằng nút Tiến độ); xong/lỗi → "Đóng". */
function closeScanProgress() {
  scanProgressOpen.value = false;
  if (!syncScanning.value) scanProgressDismissed.value = true;
}

onBeforeUnmount(stopScanProgress);

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
    const saved = await downloadZip(zipFileName(), await api.hddtExportXmlZip(ids));
    // Người dùng hủy hộp thoại "Lưu file" → chưa tải gì, khỏi toast "Đã tải".
    if (!saved) return;
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
  const jobId = newScanJobId();
  startScanProgress(jobId);
  try {
    const s = await api.hddtSyncScan({
      from: syncIsoDate(syncFrom.value),
      to: syncIsoDate(syncTo.value),
      kinds: syncKindList.value,
      jobId,
    });
    // Ngày đã có cache bị bỏ qua (không gọi lại cổng) — nói rõ để không tưởng quét thiếu.
    syncScanMsg.value =
      `Đã quét ${s.days_scanned} ngày` +
      (s.days_cached > 0 ? ` (bỏ qua ${s.days_cached} ngày đã có cache)` : "") +
      (s.details_retried > 0 ? ` · thử lại ${s.details_retried} HĐ lỗi` : "") +
      (s.days_today > 0 ? " · hôm nay (không cache)" : "") +
      ` · hóa đơn mới ${s.invoices_new} · có dòng hàng ${s.details_ok} · lỗi ${s.details_failed}` +
      ` · cần xử lý ${s.need_manual}` +
      (s.errors?.length ? ` · lỗi quét ${s.errors.length} phần` : "") +
      xmlFillText(s.xml);
    if (s.errors?.length) {
      // Lỗi chỉ ở tab máy tính tiền (`sco-query`) là chuyện phía cổng — HĐ điện
      // tử vẫn quét được. Báo đúng bản chất thay vì đổ nguyên chuỗi kỹ thuật.
      const cashOnly = s.errors.every((e) => e.startsWith("Máy tính tiền"));
      toast.add({
        severity: "warn",
        summary: cashOnly ? "Cổng HĐ máy tính tiền đang lỗi" : "Quét cổng xong một phần",
        detail: cashOnly
          ? "Máy chủ tra cứu hóa đơn máy tính tiền của cổng đang lỗi hoặc quá tải — " +
            "hóa đơn điện tử vẫn được quét bình thường. Thử lại sau."
          : s.errors.join("\n"),
        life: 12000,
      });
    }
    // Không bật `retryFailed` ở đây: scan đã tự thử lại các HĐ lỗi chi tiết. Bật
    // ở đây sẽ xoá lỗi và biến HĐ chưa có dòng hàng thành "chờ nhập kho" → bấm
    // nhập kho sẽ báo "Hóa đơn không có dòng hàng".
    await loadSyncPreview();
  } catch (e) {
    toastError("Lỗi quét cổng HĐĐT", e);
  } finally {
    // Hỏi lần cuối để lấy đúng trạng thái "✓ Hoàn tất" / lỗi mà backend vừa ghi.
    stopScanProgress();
    await pollScanProgress(jobId);
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

/**
 * Quét lại MỘT NGÀY: bỏ dấu "ngày đã quét" + xoá cache hóa đơn CỦA NGÀY ĐÓ để
 * lần bấm "Quét cổng" sau tải lại từ cổng. Hóa đơn ký trễ / cổng cập nhật chậm
 * sau khi ngày đó đã quét trước đây bị cache theo ngày bỏ sót vĩnh viễn.
 * Hóa đơn đã nhập kho (bảng riêng) giữ nguyên.
 */
function onRescanDay() {
  const day = syncIsoDate(rescanDay.value);
  if (!day) {
    toast.add({ severity: "warn", summary: "Chọn ngày cần quét lại", life: 3000 });
    return;
  }
  confirm.require({
    header: "Quét lại ngày này",
    message:
      `Xoá cache hóa đơn của ngày ${day} và bỏ dấu "ngày đã quét" — lần bấm ` +
      '"Quét cổng" sau sẽ tải lại toàn bộ hóa đơn ngày này từ cổng. Hóa đơn đã nhập kho giữ nguyên.',
    icon: "pi pi-exclamation-triangle",
    acceptLabel: "Xoá cache ngày",
    rejectLabel: "Huỷ",
    acceptProps: { severity: "danger" },
    accept: async () => {
      syncScanning.value = true;
      try {
        const out = await api.hddtUncacheDay(day);
        toast.add({
          severity: "success",
          summary: "Đã bỏ cache ngày để quét lại",
          detail: `${day}: xoá ${out.invoices_deleted} hóa đơn cache · ${out.days_deleted} dấu ngày — bấm "Quét cổng" để tải lại.`,
          life: 6000,
        });
        syncScanMsg.value = "";
        await loadSyncPreview();
      } catch (e) {
        toastError("Không bỏ được cache ngày", e);
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
// kho). Nếu lượt quét trước chưa lấy được dòng hàng, backend tự tải riêng hóa
// đơn này từ cổng — không bắt quét lại cả ngày.
const pdfVisible = ref(false);
const pdfDetail = ref("");
const pdfTitle = ref("");
/** portal_id đang tải chi tiết — khóa nút để tránh bấm 2 lần. */
const pdfLoadingKey = ref("");

async function openInvoicePdf(row: HddtSyncRow) {
  if (pdfLoadingKey.value) return;
  pdfLoadingKey.value = row.portal_id;
  const wasDetailError = row.status === "manual" && !!row.detail_error;
  try {
    pdfDetail.value = await api.hddtSyncInvoiceDetail(row.portal_id);
    // Backend vừa tải chi tiết từ cổng → cập nhật ngay số dòng trên bảng.
    try {
      const lines = JSON.parse(pdfDetail.value)?.hdhhdvu;
      if (Array.isArray(lines)) {
        row.line_count = lines.length;
        row.detail_error = "";
        if (wasDetailError) {
          row.status = "pending";
          row.skip_reason = "";
        }
      }
    } catch {
      // JSON hỏng thì bảng giữ số dòng cũ; dialog PDF vẫn báo lỗi rõ ràng.
    }
    pdfTitle.value = `Hóa đơn ${row.khhdon} ${row.shdon}`;
    pdfVisible.value = true;
  } catch (e) {
    toastError("Không lấy được chi tiết hóa đơn", e);
  } finally {
    pdfLoadingKey.value = "";
  }
}

// Mốc bắt đầu = hddt_start_date của hộ → phải nạp config trước rồi mới tính;
// mốc cuối = hôm nay (hôm nay quét nhưng không cache). Không dùng onMounted vì
// không có thao tác ref/DOM ở đây.
// Mốc ngày chỉ đặt một lần lúc mount: quay lại màn không được xoá khoảng
// ngày người dùng đang lọc.
async function reload() {
  await Promise.all([loadSyncPreview(), loadAutoSettings()]);
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

    <!-- Cài đặt tự động ngay màn Đồng bộ (issue #7 mục 1): trước đây chỉ nằm
         trong Cài đặt nên người dùng tưởng app không có nhập hóa đơn tự động. -->
    <SectionCard title="Tự động nhập hóa đơn">
      <template #icon>
        <i class="pi pi-bolt text-primary-500" />
      </template>
      <p class="mb-3 text-xs text-gray-500">
        Chạy khi mở app rồi lặp lại sau mỗi khoảng thời gian bên dưới. Quét xong có thể tự tạo phiếu
        nhập cho hóa đơn đã có dòng hàng; hóa đơn lỗi chi tiết vẫn nằm chờ xử lý. Cần đã đăng nhập
        cổng HĐĐT.
      </p>
      <div class="space-y-3">
        <div class="flex items-center justify-between gap-4">
          <label for="sync-auto-enabled" class="cursor-pointer">
            <span class="font-medium">Tự động nhập hóa đơn</span>
            <span class="block text-xs text-gray-500 mt-0.5">
              Bật = mở app là quét cổng và (nếu bật bên dưới) tạo phiếu nhập; tắt = chỉ quét khi bấm
              tay.
            </span>
          </label>
          <ToggleSwitch input-id="sync-auto-enabled" v-model="autoForm.enabled" />
        </div>
        <div class="flex items-center justify-between gap-4">
          <label for="sync-auto-import" class="cursor-pointer">
            <span class="font-medium">Quét xong tự tạo phiếu nhập</span>
            <span class="block text-xs text-gray-500 mt-0.5">
              Chỉ nhập hóa đơn đã có dòng hàng; hóa đơn lỗi chi tiết vẫn nằm chờ xử lý. Tắt = chỉ
              kéo danh sách về để xem trước.
            </span>
          </label>
          <ToggleSwitch
            input-id="sync-auto-import"
            v-model="autoForm.importAfter"
            :disabled="!autoForm.enabled"
          />
        </div>
        <div class="flex items-center justify-between gap-4">
          <label for="sync-auto-interval" class="cursor-pointer">
            <span class="font-medium">Khoảng thời gian tự gọi lại</span>
            <span class="block text-xs text-gray-500 mt-0.5">Từ 1 tới 1440 phút.</span>
          </label>
          <InputNumber
            v-model="autoForm.intervalMin"
            :min="1"
            :max="1440"
            :show-buttons="true"
            :step="5"
            suffix=" phút"
            input-id="sync-auto-interval"
            :disabled="!autoForm.enabled"
            class="w-44"
          />
        </div>
      </div>
      <div
        class="mt-4 flex flex-wrap items-center justify-between gap-3 border-t border-surface-200 pt-3"
      >
        <p class="text-xs text-gray-500">
          <i class="pi pi-clock mr-1" />
          <template v-if="autoLastRunAt">
            Lần chạy gần nhất: <b>{{ fmtAutoRun(autoLastRunAt) }}</b>
            <template v-if="autoLastResult"> — {{ autoLastResult }}</template>
          </template>
          <template v-else>Chưa có lần chạy nền nào.</template>
        </p>
        <Button
          label="Lưu cài đặt"
          icon="pi pi-check"
          size="small"
          :loading="autoSaving"
          @click="saveAutoSettings"
        />
      </div>
    </SectionCard>

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
          <label for="sync-from" class="mb-1 block text-xs text-gray-500">Từ ngày lập</label>
          <DatePicker v-model="syncFrom" inputId="sync-from" dateFormat="dd/mm/yy" class="w-40" />
        </div>
        <div>
          <label for="sync-to" class="mb-1 block text-xs text-gray-500">Đến ngày lập</label>
          <DatePicker v-model="syncTo" inputId="sync-to" dateFormat="dd/mm/yy" class="w-40" />
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
        <Button
          label="Quét cổng"
          icon="pi pi-sync"
          :loading="syncScanning"
          data-testid="hddt-scan"
          @click="onSyncScan"
        />
        <!-- Hiện khi người dùng bấm "Chạy nền" — mở lại nhật ký realtime, kể cả
             khi lượt quét đã xong (chưa bấm "Đóng"). -->
        <Button
          v-if="scanProgress && !scanProgressDismissed && !scanProgressOpen"
          label="Tiến độ"
          icon="pi pi-hourglass"
          severity="secondary"
          outlined
          data-testid="hddt-scan-progress-open"
          @click="scanProgressOpen = true"
        />
        <Button
          label="Xoá cache"
          icon="pi pi-trash"
          severity="secondary"
          :disabled="syncScanning || !syncSummary || syncSummary.total === 0"
          @click="onSyncClearCache"
        />
        <!-- Quét lại 1 ngày: hóa đơn ký trễ / cổng cập nhật chậm bị cache theo ngày bỏ sót. -->
        <div class="flex items-end gap-2">
          <div>
            <label class="mb-1 block text-xs text-gray-500">Ngày quét lại</label>
            <DatePicker
              v-model="rescanDay"
              dateFormat="dd/mm/yy"
              placeholder="Chọn ngày"
              class="w-40"
              :max-date="new Date()"
            />
          </div>
          <Button
            label="Quét lại ngày này"
            icon="pi pi-replay"
            severity="warn"
            :disabled="syncScanning || !rescanDay"
            v-tooltip="'Bỏ dấu ngày đã quét + xoá cache hóa đơn của ngày đó để tải lại từ cổng'"
            @click="onRescanDay"
          />
        </div>
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

      <!-- Nhật ký không giới hạn theo bản ghi (khoảng lọc theo ngày vẫn có thể
           trả hàng nghìn dòng) → phân trang, vẫn giữ khung cuộn cố định. -->
      <AppDataTable
        :value="syncRows"
        :loading="syncScanning"
        stripedRows
        scrollable
        scrollHeight="420px"
        size="small"
        paginator
        :rows="50"
        :rows-per-page-options="[20, 50, 100, 200]"
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
                :loading="pdfLoadingKey === data.portal_id"
                :disabled="!!pdfLoadingKey && pdfLoadingKey !== data.portal_id"
                :aria-label="`Xem PDF hóa đơn ${data.khhdon} ${data.shdon}`"
                v-tooltip="'Xem PDF hóa đơn (tự tải chi tiết nếu chưa có)'"
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

    <!-- Nhật ký realtime của lượt quét cổng: chỉ mở khi bấm tay (việc chạy nền
         không có jobId), giữ lại sau khi xong để đọc dòng nào chậm/lỗi. -->
    <JobProgressDialog
      :visible="scanProgressOpen"
      header="Quét cổng HĐĐT"
      :job="scanProgress"
      :pre-log="scanPreLog"
      :running="syncScanning"
      @update:visible="closeScanProgress"
    />

    <VoucherViewDialog v-model:visible="viewVoucherVisible" :voucher-no="viewVoucherNo" />

    <InvoicePdfDialog v-model:visible="pdfVisible" :detail-json="pdfDetail" :title="pdfTitle" />
  </div>
</template>
