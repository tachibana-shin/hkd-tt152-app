<script setup lang="ts">
import { storeToRefs } from "pinia";
import { useBusinessStore } from "@/stores/business";
import { api } from "@/db";
import { exportXlsx, buildTaxBookXlsx, type XlsxColumn } from "@/utils/excel";
import TaxEntryForm from "@/components/TaxEntryForm.vue";
import { TT152_BOOKS } from "@/composables/useTaxBooks";
import { buildTaxEntryXlsx } from "@/utils/taxEntry";
import { blobToBase64, downloadZip } from "@/utils/download";
import type {
  RevenueExpenseRow,
  CogsBackfillPending,
  TrialBalanceRow,
  TaxDeclarationRow,
  TaxOverview,
  TaxSettlement,
} from "@/types";
import { useAuthStore } from "@/stores/auth";
import { fmtInt as fmt, fmtPct as pct, fmtThreshold, fmtVnd, toIsoDate } from "@/utils/format";
import { describeRange, monthRange, quarterRange, sameDay, taxPeriodRange } from "@/utils/period";
import { useKeepAliveRefresh } from "@/composables/useKeepAliveRefresh";

const business = useBusinessStore();
const auth = useAuthStore();
const { config } = storeToRefs(business);
const toast = useToast();

const loading = ref(false);
// Tờ khai thuế: MỘT bảng duy nhất, theo khoảng ngày đang chọn ở thanh công cụ.
// (Trước đây có hai bảng — theo nhóm ngành nghề và theo kỳ — trùng dữ liệu nhau.)
const tb = ref<TrialBalanceRow[]>([]);
const re = ref<RevenueExpenseRow>({
  revenue_up: 0,
  revenue_down: 0,
  expense_up: 0,
  expense_down: 0,
  cogs: 0,
});
// Phiếu xuất lịch sử chưa có bút toán Nợ 632 / Có 152 → bảng cân đối thiếu cột
// giá vốn. Đếm trên TOÀN bộ lịch sử chứ không theo khoảng ngày đang xem.
const cogsBackfill = ref<CogsBackfillPending>({ count: 0, amount: 0 });
const backfilling = ref(false);

// Chi phí kinh doanh = chứng từ chi trong kỳ + giá vốn hàng đã xuất kho (FIFO).
// Không cộng giá vốn thì hàng bán ra không hiện ở cột chi phí nào nên lợi nhuận
// trước thuế in ra cao hơn số quyết toán thuế ngay trên cùng màn hình.
const expenseNet = () => re.value.expense_up - re.value.expense_down + re.value.cogs;
const revenueNet = () => re.value.revenue_up - re.value.revenue_down;
const profitNet = () => revenueNet() - expenseNet();

const unitOptions = [
  { label: "Toàn bộ", value: "" },
  { label: "Đơn vị chính (HKD)", value: "HKD" },
];
const unitCode = ref("HKD");

const fromDate = ref<Date | null>(null);
const toDate = ref<Date | null>(null);

const backupDialog = ref(false);
const configDialog = ref(false);

function openConfig() {
  configDialog.value = true;
}

function openBackupDialog() {
  backupDialog.value = true;
}

async function loadReports() {
  loading.value = true;
  try {
    const year = new Date().getFullYear();
    const f = toIsoDate(fromDate.value) || `${year}-01-01`;
    const t = toIsoDate(toDate.value) || `${year}-12-31`;
    const [rev, balance, pending] = await Promise.all([
      api.getRevenueExpense(f, t),
      api.getTrialBalance(f, t),
      api.getCogsBackfillPending(),
    ]);
    re.value = rev;
    tb.value = balance;
    cogsBackfill.value = pending;
  } catch (e) {
    toast.add({
      severity: "error",
      summary: "Lỗi tải báo cáo",
      detail: String(e),
    });
  } finally {
    loading.value = false;
  }
}

/** Ghi bút toán Nợ 632 / Có 152 cho các phiếu xuất lịch sử rồi nạp lại báo cáo. */
async function doBackfillCogs() {
  backfilling.value = true;
  try {
    const written = await api.backfillCogsEntries();
    if (written.count > 0) {
      toast.add({
        severity: "success",
        summary: "Đã ghi bút toán giá vốn",
        detail: `${written.count} bút toán Nợ 632 / Có 152, tổng ${fmt(written.amount)} đ`,
      });
      // Bảng cân đối và card chi phí đọc từ sổ cái → phải nạp lại mới thấy.
      await loadReports();
    } else {
      toast.add({
        severity: "info",
        summary: "Không còn bút toán nào thiếu",
        detail: "Toàn bộ phiếu xuất đã có bút toán giá vốn.",
      });
      cogsBackfill.value = { count: 0, amount: 0 };
    }
  } catch (e) {
    toast.add({
      severity: "error",
      summary: "Lỗi ghi bút toán giá vốn",
      detail: String(e),
    });
  } finally {
    backfilling.value = false;
  }
}

// ——— Tờ khai thuế theo kỳ (To Khai Thue) + tổng hợp thuế theo quy định 2026 ———
const declRows = ref<TaxDeclarationRow[]>([]);
const declLoading = ref(false);
const overview = ref<TaxOverview | null>(null);
/**
 * Căn cứ tính thuế của kỳ: hộ thuộc diện nộp cả năm, hay kỳ này chưa vượt ngưỡng.
 *
 * Hai trường hợp cho hai con số khác nhau nên phải nói rõ, tránh tưởng app tính sai:
 * hồ sơ chốt Nhóm 2/3/4 → thuộc diện nộp thuế suốt năm, tính trên toàn bộ doanh
 * thu của kỳ; hồ sơ để Nhóm 1 hoặc chưa chốt → chỉ nộp từ kỳ doanh thu lũy kế vượt
 * ngưỡng (Điều 8 khoản 1a).
 */
function taxBasisText(o: TaxOverview): string {
  const th = fmtThreshold(o.exempt_threshold);
  if (o.taxed_from_start) {
    return `Hộ thuộc diện nộp thuế suốt năm (hồ sơ chốt Nhóm ${o.group}). Thuế GTGT tính trên toàn bộ doanh thu ghi trên hóa đơn; còn thuế TNCN tính trên phần doanh thu còn lại sau khi trừ mức trừ lũy kế ${th}/năm (trừ dần theo doanh thu các kỳ trước và kỳ này), nên ${o.taxable_revenue > 0 ? `kỳ này còn ${fmt(o.taxable_revenue)} đ chịu thuế.` : `khi doanh thu cả năm chưa vượt ${th} thì thuế TNCN bằng 0.`}`;
  }
  return o.taxable_period
    ? `Ngưỡng ${th} đã bị vượt trong năm nên từ kỳ vượt ngưỡng hộ phải nộp thuế; thuế TNCN vẫn trừ mức trừ lũy kế ${th}/năm.`
    : `Chưa vượt ngưỡng ${th}/năm nên kỳ này chưa phát sinh thuế.`;
}

/** Nhóm hộ đang dùng lấy từ đâu — nói rõ để người dùng tin con số. */
function groupSourceText(o: TaxOverview): string {
  if (o.group_from_profile) return "lấy từ hồ sơ HKD (bạn đã chốt, dùng cho mọi kỳ).";
  if (o.frozen_group != null)
    return `đã chốt cho năm ${o.year} theo doanh thu khi vượt ngưỡng, giữ nguyên cả năm.`;
  return "app tự xếp từ doanh thu năm.";
}
/**
 * Năm tính thuế — lấy theo NĂM của khoảng ngày đang chọn (mức trừ ngưỡng và hạn
 * nộp đều tính theo năm dương lịch), nên đổi khoảng ngày sang năm khác thì tờ
 * khai và quyết toán cùng đổi theo.
 */
const declYear = computed(
  () => Number(toIsoDate(fromDate.value).slice(0, 4)) || new Date().getFullYear(),
);
/**
 * Khoảng ngày kê khai ISO — một nguồn duy nhất cho CẢ HAI mục "Tờ khai thuế" và
 * "Tờ khai 01/CNKD", để báo cáo số thuế và bản kê khai không bao giờ lệch kỳ.
 * Thiếu khoảng ngày thì về trọn năm của năm tờ khai (giống khi chưa chọn gì).
 */
const declRange = computed(() => ({
  from: toIsoDate(fromDate.value) || `${declYear.value}-01-01`,
  to: toIsoDate(toDate.value) || `${declYear.value}-12-31`,
}));
/** Tờ khai 01/CNKD — "Xuất báo cáo kỳ thuế" lấy số đã sửa tay của nó. */
const taxEntry = ref<InstanceType<typeof TaxEntryForm> | null>(null);
// (Bộ chọn của SỔ — "Năm" / "Loại kỳ sổ" / "Kỳ" — nằm trọn ở tab Sổ kế toán,
// không phụ thuộc màn này.)

// Cấu hình kê khai thuế của hộ (app_setting): kỳ khai + phương pháp TNCN
const taxDialog = ref(false);
const taxPeriod = ref<string>("quarter"); // year | quarter | month | per_occurrence
const taxMethod = ref<string>("revenue"); // revenue | profit
const taxPeriodLabel = computed(() => {
  switch (taxPeriod.value) {
    case "month":
      return "Theo tháng";
    case "year":
      return "Theo năm";
    case "per_occurrence":
      return "Theo từng lần phát sinh";
    default:
      return "Theo quý";
  }
});
const taxMethodLabel = computed(() =>
  taxMethod.value === "profit" ? "Theo lợi nhuận" : "Theo doanh thu",
);
/** Ngưỡng đang áp dụng — đọc từ Cài đặt, không gắn cứng 01 tỷ trong chữ. */
const thExempt = computed(() => overview.value?.exempt_threshold ?? 1_000_000_000);
const thGroup3 = computed(() => overview.value?.group3_threshold ?? 3_000_000_000);
const thGroup4 = computed(() => overview.value?.group4_threshold ?? 50_000_000_000);
const groupLabel = computed(() => {
  const ex = fmtThreshold(thExempt.value);
  const g3 = fmtThreshold(thGroup3.value);
  const g4 = fmtThreshold(thGroup4.value);
  switch (overview.value?.group) {
    case 1:
      return `Nhóm 1 — doanh thu ≤ ${ex}: miễn thuế`;
    case 2:
      return `Nhóm 2 — doanh thu > ${ex} đến ${g3}`;
    case 3:
      return `Nhóm 3 — doanh thu > ${g3} đến ${g4}`;
    default:
      return `Nhóm 4 — doanh thu > ${g4}`;
  }
});
const settlement = ref<TaxSettlement | null>(null);
const settleLoading = ref(false);

async function loadSettlement() {
  settleLoading.value = true;
  try {
    settlement.value = await api.getTaxSettlement(declYear.value);
  } catch (e) {
    settlement.value = null;
    toast.add({ severity: "error", summary: "Lỗi tải số tạm nộp", detail: String(e) });
  } finally {
    settleLoading.value = false;
  }
}

const declTotals = computed(() => {
  const t = {
    revenue_up: 0,
    revenue_down: 0,
    revenue_taxable: 0,
    pit_deduction: 0,
    vat_tax: 0,
    vat_payable: 0,
    pit_tax: 0,
  };
  for (const r of declRows.value) {
    t.revenue_up += r.revenue_up;
    t.revenue_down += r.revenue_down;
    t.revenue_taxable += r.revenue_taxable;
    t.pit_deduction += r.pit_deduction;
    t.vat_tax += r.vat_tax;
    t.vat_payable += r.vat_payable;
    t.pit_tax += r.pit_tax;
  }
  return t;
});

function printReport() {
  window.print();
}

async function loadDeclaration() {
  declLoading.value = true;
  try {
    // Cùng khoảng ngày với các bảng khác trên màn → tờ khai luôn khớp số liệu.
    const f = declRange.value.from;
    const t = declRange.value.to;
    const [rows, ov] = await Promise.all([
      api.getTaxDeclaration(f, t, unitCode.value),
      api.getTaxOverview(f, t, unitCode.value),
    ]);
    declRows.value = rows;
    overview.value = ov;
    // Số tạm nộp cả năm — bảng quyết toán vẫn nằm trên màn này.
    await loadSettlement();
  } catch (e) {
    toast.add({
      severity: "error",
      summary: "Lỗi tải tờ khai thuế",
      detail: String(e),
    });
  } finally {
    declLoading.value = false;
  }
}

async function exportDeclarationExcel() {
  const cols: XlsxColumn[] = [
    { header: "Nhóm ngành nghề", key: "industry_name" },
    { header: "Mã", key: "industry_code" },
    { header: "Tỷ lệ GTGT %", key: "vat_percent" },
    { header: "Tỷ lệ TNCN %", key: "pit_percent" },
    { header: "DT tính thuế GTGT – Tăng", key: "revenue_up" },
    { header: "DT tính thuế GTGT – Giảm", key: "revenue_down" },
    { header: "Mức trừ TNCN đã dùng trong kỳ", key: "pit_deduction" },
    { header: "DT tính thuế TNCN (đã trừ mức trừ)", key: "revenue_taxable" },
    { header: "Thuế GTGT", key: "vat_tax" },
    { header: "Thuế GTGT phải nộp", key: "vat_payable" },
    { header: "Thuế TNCN phải nộp", key: "pit_tax" },
  ];
  const rows = declRows.value.map((r) => ({
    industry_name: r.industry_name,
    industry_code: r.industry_code,
    vat_percent: r.vat_rate * 100,
    pit_percent: r.pit_rate * 100,
    revenue_up: r.revenue_up,
    revenue_down: r.revenue_down,
    pit_deduction: r.pit_deduction,
    revenue_taxable: r.revenue_taxable,
    vat_tax: r.vat_tax,
    vat_payable: r.vat_payable,
    pit_tax: r.pit_tax,
  }));
  // Tên file theo khoảng ngày đang xem (tờ khai không còn bộ chọn kỳ riêng).
  try {
    await exportXlsx(
      `to-khai-thue-${toIsoDate(fromDate.value)}-${toIsoDate(toDate.value)}`,
      cols,
      rows,
    );
  } catch (e) {
    toast.add({ severity: "error", summary: "Lỗi xuất file Excel", detail: String(e) });
  }
}

// ─── Xuất báo cáo kỳ thuế — 1 file ZIP gộp cả hồ sơ ───

const reportExporting = ref(false);

/**
 * Đóng gói **hồ sơ của kỳ thuế** rồi tải về một file ZIP duy nhất:
 *
 * ```
 * CHU-THICH.txt       hướng dẫn mở + số liệu từng nhóm file
 * to-khai/            tờ khai 01/CNKD (số ĐANG hiển thị, gồm ô sửa tay)
 * so-ke-toan/         CẢ 7 sổ kế toán theo năm — kỳ chưa tròn năm vẫn lấy đủ
 *                     dữ liệu hiện có (sổ ghi liên tục cả năm theo Luật Kế toán)
 * hoa-don/pdf/…       PDF hóa đơn mua vào & bán ra phát sinh trong kỳ
 * hoa-don/xml/…       TOÀN BỘ XML đã lưu — bản gốc từ cổng, không lọc kỳ
 * sao-luu/hkd.db      bản sao lưu cơ sở dữ liệu
 * ```
 *
 * Frontend chỉ dựng phần **Excel** (đã có `exceljs`, lại đang giữ đúng số liệu
 * người dùng sửa trên màn); phần PDF/XML/saoluu frontend không với tới được nên
 * gửi kèm rồi để backend gộp (`commands/tax_report.rs`).
 */
async function exportTaxReport() {
  if (reportExporting.value) return;
  reportExporting.value = true;
  const problems: string[] = [];
  try {
    const { from, to } = declRange.value;
    const files: { path: string; data: string }[] = [];

    // 1. Tờ khai 01/CNKD — lấy ĐÚNG bản đang hiện trên màn, không nạp lại,
    //    để các ô sửa tay đi theo vào hồ sơ.
    const payload = taxEntry.value?.exportPayload();
    if (payload?.rows.length) {
      files.push({
        path: `to-khai/${payload.fileName}`,
        data: await blobToBase64(await buildTaxEntryXlsx(payload)),
      });
    }

    // 2. Cả 7 sổ kế toán của năm — mẫu nào lỗi vẫn đóng gói nốt các sổ còn lại.
    const year = Number(from.slice(0, 4)) || new Date().getFullYear();
    for (const b of TT152_BOOKS) {
      try {
        const book = await api.getTaxBooks(year, "year", 0, b.value);
        files.push({
          path: `so-ke-toan/${b.value}-${year}.xlsx`,
          data: await blobToBase64(await buildTaxBookXlsx(book)),
        });
      } catch (e) {
        problems.push(`${b.value}: ${e}`);
      }
    }

    // 3. Backend thêm PDF/XML hóa đơn + sao lưu CSDL rồi nén 1 ZIP.
    const zip = await api.exportTaxReport({ fromDate: from, toDate: to, extraFiles: files });
    const name = `bao-cao-ky-thue-${from}_${to}.zip`;
    downloadZip(name, zip);
    toast.add({
      severity: problems.length ? "warn" : "success",
      summary: "Đã tải báo cáo kỳ thuế",
      detail: problems.length
        ? `${name} — ${files.length} file Excel; ${problems.length} sổ lỗi, xem CHU-THICH.txt trong ZIP`
        : `${name} — ${files.length} file Excel + hóa đơn PDF/XML + sao lưu CSDL`,
      life: 6000,
    });
  } catch (e) {
    toast.add({ severity: "error", summary: "Lỗi xuất báo cáo kỳ thuế", detail: String(e) });
  } finally {
    reportExporting.value = false;
  }
}

function openTaxConfig() {
  taxDialog.value = true;
}

/**
 * Đặt khoảng ngày xem báo cáo theo kỳ khai của hộ, tính từ hôm nay: khai theo
 * quý thì mở màn ra thấy đúng quý hiện tại, theo tháng thì tháng hiện tại, theo
 * năm/từng lần phát sinh thì cả năm. Gọi đúng một lần khi mở màn (không chạy
 * mỗi lần quay lại) để không mất khoảng ngày người dùng đã chọn.
 */
function applyReportPeriod() {
  const { from, to } = taxPeriodRange(taxPeriod.value);
  fromDate.value = from;
  toDate.value = to;
}

/** Nhãn kỳ đang xem, để khỏi tự nhẩy từ hai ô ngày. */
const reportPeriodLabel = computed(() => describeRange(fromDate.value, toDate.value));

/**
 * Kỳ khai của hộ quyết định có "chuyển nhanh" hay không và bao nhiêu kỳ:
 *   • `quarter` → 4 nút Quý 1-4 (bấm là xong, không cần mở menu)
 *   • `month`   → 12 tháng trong một Select (12 nút thì chiếm hết thanh lọc)
 *   • `year` / `per_occurrence` → cả năm là một kỳ duy nhất, không có gì để
 *     chuyển nên không hiện control cho khỏi rối.
 */
const quickQuarters = computed(() => (taxPeriod.value === "quarter" ? [1, 2, 3, 4] : []));
const quickMonths = computed(() =>
  taxPeriod.value === "month"
    ? Array.from({ length: 12 }, (_, i) => ({ label: `Tháng ${i + 1}`, value: i + 1 }))
    : [],
);

/**
 * Kỳ đang xem nếu khoảng ngày khớp **trọn** kỳ theo kỳ khai của hộ, ngược lại
 * `null` (đang xem khoảng tự chọn) — dùng để tô nút / chọn sẵn trong ô.
 */
const activeQuickNo = computed<number | null>(() => {
  const isQuarter = taxPeriod.value === "quarter";
  const isMonth = taxPeriod.value === "month";
  if (!isQuarter && !isMonth) return null;
  const from = fromDate.value;
  const to = toDate.value;
  if (!from || !to) return null;
  const no = isQuarter ? Math.floor(from.getMonth() / 3) + 1 : from.getMonth() + 1;
  const r = isQuarter ? quarterRange(from.getFullYear(), no) : monthRange(from.getFullYear(), no);
  return sameDay(from, r.from) && sameDay(to, r.to) ? no : null;
});

/**
 * Chuyển nhanh sang kỳ `no`: đặt hai ô ngày đúng ranh giới kỳ rồi nạp lại cả
 * màn (báo cáo doanh thu - chi phí, tờ khai, sổ kế toán).
 *
 * Năm lấy theo khoảng **đang xem** chứ không phải năm hệ thống: đang tra cứu
 * năm 2025 mà chọn Quý 3 thì phải ở quý 3/2025, nếu không người dùng đang so
 * sánh số liệu năm cũ bỗng bị nhảy sang năm nay.
 */
async function goQuick(no: number) {
  const isQuarter = taxPeriod.value === "quarter";
  const r = isQuarter ? quarterRange(declYear.value, no) : monthRange(declYear.value, no);
  fromDate.value = r.from;
  toDate.value = r.to;
  await reload();
}

/** `v-model` của bộ chọn chuyển nhanh theo tháng. */
const quickMonthModel = computed<number | null>({
  get: () => (taxPeriod.value === "month" ? activeQuickNo.value : null),
  set: (v) => {
    if (v != null) void goQuick(Number(v));
  },
});

async function onTaxConfigSaved(payload: { period: string; method: string }) {
  taxPeriod.value = payload.period;
  taxMethod.value = payload.method;
  // Đổi kỳ khai thì kỳ xem báo cáo cũng theo, nếu không người dùng phải tự
  // chỉnh lại khoảng ngào.
  applyReportPeriod();
  toast.add({
    severity: "success",
    summary: "Đã lưu cấu hình thuế",
    detail: `Kỳ khai: ${taxPeriodLabel.value} • TNCN: ${taxMethodLabel.value}`,
    life: 3000,
  });
  await loadDeclaration();
}

// Chỉ nạp lại báo cáo + tờ khai; phần đặt mặc định kỳ khai bên dưới chạy
// đúng một lần để không mất lựa chọn của người dùng khi quay lại màn.
async function reload() {
  await Promise.all([loadReports(), loadDeclaration()]);
}

void (async () => {
  await business.load();
  // Kỳ báo cáo không còn lưu — mặc định theo năm hiện tại của hệ thống.
  const settings = await api.getAppSettings().catch(() => null);
  if (settings) {
    taxPeriod.value = settings.tax_period;
    taxMethod.value = settings.tax_method;
  }
  applyReportPeriod();
  await reload();
})();

// Quay lại màn (KeepAlive giữ state) → nạp lại dữ liệu cho khỏi cũ.
useKeepAliveRefresh(reload);
</script>

<template>
  <div class="space-y-4">
    <Toolbar>
      <template #start>
        <div class="flex items-center gap-2">
          <i-mdi-calculator class="text-xl text-rose-500" />
          <h3 class="font-semibold">Kế toán hộ kinh doanh</h3>
        </div>
      </template>
      <template #end>
        <Button label="Cấu hình hộ KD" icon="pi pi-cog" severity="secondary" @click="openConfig" />
        <Button
          label="Cấu hình thuế"
          icon="pi pi-percentage"
          severity="secondary"
          v-tooltip.top="'Kỳ khai thuế + phương pháp tính TNCN (NĐ 68/2026, NĐ 141/2026)'"
          @click="openTaxConfig"
        />
        <Button label="Tải lại báo cáo" icon="pi pi-refresh" @click="loadReports" />
      </template>
    </Toolbar>

    <Card>
      <template #content>
        <div class="flex flex-wrap items-center gap-3">
          <div>
            <label class="text-xs text-gray-500 block mb-1">Từ ngày</label>
            <DatePicker v-model="fromDate" dateFormat="dd/mm/yy" class="w-44" />
          </div>
          <div>
            <label class="text-xs text-gray-500 block mb-1">Đến ngày</label>
            <DatePicker v-model="toDate" dateFormat="dd/mm/yy" class="w-44" />
          </div>
          <div>
            <label class="text-xs text-gray-500 block mb-1">Đơn vị</label>
            <Select
              v-model="unitCode"
              :options="unitOptions"
              optionLabel="label"
              optionValue="value"
              class="w-44"
            />
          </div>
          <div v-if="quickQuarters.length || quickMonths.length">
            <label class="text-xs text-gray-500 block mb-1">Chuyển nhanh</label>
            <ButtonGroup
              v-if="quickQuarters.length"
              v-tooltip.top="'Đổi khoảng ngày sang quý đó rồi nạp lại cả màn'"
            >
              <Button
                v-for="q in quickQuarters"
                :key="q"
                :label="`Quý ${q}`"
                size="small"
                :severity="activeQuickNo === q ? 'primary' : 'secondary'"
                :outlined="activeQuickNo !== q"
                :data-testid="`quick-quarter-${q}`"
                @click="goQuick(q)"
              />
            </ButtonGroup>
            <Select
              v-else
              v-model="quickMonthModel"
              :options="quickMonths"
              option-label="label"
              option-value="value"
              placeholder="Chọn tháng"
              class="w-40"
              aria-label="Chuyển nhanh theo tháng"
              data-testid="quick-month"
              v-tooltip.top="'Đổi khoảng ngày sang tháng đó rồi nạp lại cả màn'"
            />
          </div>
          <Button label="Xem báo cáo" icon="pi pi-search" size="small" @click="loadReports" />
          <span v-if="config" class="text-sm text-gray-500 ml-auto">
            <i class="pi pi-calendar mr-1" />Kỳ khai thuế: <b>{{ taxPeriodLabel }}</b> • TNCN:
            <b>{{ taxMethodLabel }}</b>
          </span>
        </div>
      </template>
    </Card>

    <!-- Tổng hợp thuế phải nộp theo NĐ 68/2026 + NĐ 141/2026 -->
    <SectionCard title="Tổng hợp thuế phải nộp (NĐ 68/2026, NĐ 141/2026)">
      <template #icon><i-mdi-calculator-variant class="text-indigo-500" /></template>
      <div v-if="overview" class="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-4 gap-4 text-sm">
        <div>
          <span class="text-gray-500 block text-xs mb-1">Nhóm hộ kinh doanh</span>
          <b>{{ groupLabel }}</b>
          <div class="text-xs text-gray-400">DT cả năm: {{ fmt(overview.year_revenue) }} đ</div>
        </div>
        <div>
          <span class="text-gray-500 block text-xs mb-1">Doanh thu kỳ khai</span>
          <b>{{ fmt(overview.period_revenue) }} đ</b>
        </div>
        <div>
          <span class="text-gray-500 block text-xs mb-1">Doanh thu tính thuế TNCN</span>
          <b>{{ fmt(overview.taxable_revenue) }} đ</b>
          <div class="text-xs text-gray-400">
            DT kỳ {{ fmt(overview.period_revenue) }} đ − trừ mức trừ
            {{ fmt(overview.exempt_period) }} đ
          </div>
        </div>
        <div data-testid="exempt-quota">
          <span class="text-gray-500 block text-xs mb-1">Hạn ngạch mức trừ TNCN còn lại</span>
          <b>{{ fmt(overview.exempt_remaining ?? 0) }} đ</b>
          <div class="text-xs text-gray-400">
            Mức trừ {{ fmt(overview.exempt_threshold) }} đ/năm − đã dùng
            {{ fmt(overview.exempt_used ?? 0) }} đ
            {{
              (overview.exempt_remaining ?? 0) > 0
                ? " → doanh thu năm chưa vượt mức trừ nên TNCN theo doanh thu còn bằng 0."
                : " → đã dùng hết mức trừ, phần doanh thu vượt mức sẽ chịu thuế TNCN."
            }}
          </div>
        </div>
        <div>
          <span class="text-gray-500 block text-xs mb-1">Chi phí kỳ (được trừ)</span>
          <b>{{ fmt(overview.expense) }} đ</b>
          <div class="text-xs text-gray-400">
            Giá vốn FIFO {{ fmt(overview.cogs) }} đ + chi khác {{ fmt(overview.other_expense) }} đ
          </div>
        </div>
        <div>
          <span class="text-gray-500 block text-xs mb-1">Thu nhập tính thuế</span>
          <b>{{ fmt(overview.profit) }} đ</b>
        </div>
        <div>
          <span class="text-gray-500 block text-xs mb-1">Thuế GTGT phải nộp</span>
          <b class="text-rose-600">{{ fmt(overview.vat_payable) }} đ</b>
        </div>
        <div>
          <span class="text-gray-500 block text-xs mb-1">
            Thuế TNCN phải nộp
            <span v-if="overview.profit_rate" class="text-gray-400"
              >(lợi nhuận × {{ (overview.profit_rate * 100).toFixed(0) }}%)</span
            >
            <span v-else-if="overview.method === 'revenue'" class="text-gray-400"
              >(theo doanh thu)</span
            >
          </span>
          <b class="text-rose-600">{{ fmt(overview.pit_tax) }} đ</b>
        </div>
        <div>
          <span class="text-gray-500 block text-xs mb-1">Hạn nộp tờ khai</span>
          <b>{{ overview.deadline || "—" }}</b>
          <div class="text-xs text-gray-400">
            {{ overview.group === 1 ? "Thông báo doanh thu năm" : "Cùng hạn kê khai GTGT" }}
          </div>
        </div>
        <div>
          <span class="text-gray-500 block text-xs mb-1">TNCN tạm nộp kỳ</span>
          <b>{{ fmt(overview.pit_provisional) }} đ</b>
          <div class="text-xs text-gray-400">Theo tỷ lệ % × doanh thu kỳ</div>
        </div>
        <div class="col-span-2 md:col-span-2">
          <span class="text-gray-500 block text-xs mb-1">Tổng thuế phải nộp kỳ</span>
          <b class="text-primary-600 text-lg">{{ fmt(overview.total_tax) }} đ</b>
        </div>
      </div>
      <div
        v-if="overview && overview.non_deductible > 0"
        class="mt-3 rounded border border-amber-300 bg-amber-50 p-3 text-xs"
      >
        <b class="text-amber-800">
          {{ fmt(overview.non_deductible) }} đ chi KHÔNG được trừ khi tính thu nhập tính thuế
        </b>
        <ul class="mt-1 list-disc pl-5 text-amber-700">
          <li v-for="w in overview.cost_warnings" :key="w.voucher_no + w.posting_date">
            {{ w.voucher_no }} ({{ w.posting_date }}) — {{ fmtVnd(w.amount) }} đ: {{ w.reason }}
          </li>
        </ul>
        <p class="mt-1 text-amber-600">
          Bổ sung chứng từ thanh toán không dùng tiền mặt, hoặc gỡ dấu "không được trừ" ở màn
          Thu/Chi nếu thực tế được trừ (Điều 6 Nghị định 68/2026).
        </p>
      </div>
      <!-- Ghi chú nhóm hộ, KHÔNG phải cảnh báo: năm chưa kết thúc nên doanh thu còn
           tăng; theo nguyên tắc ổn định nhóm của năm không tự đổi giữa chừng. -->
      <div
        v-if="overview"
        data-testid="tax-group-source-note"
        class="mt-3 rounded border border-gray-200 bg-gray-50 p-2 text-xs text-gray-600"
      >
        <b>Nhóm hộ: Nhóm {{ overview.group }}</b> — {{ groupSourceText(overview) }}
        {{ taxBasisText(overview) }} Doanh thu cả năm hiện tại {{ fmt(overview.year_revenue) }} đ
        (năm chưa kết thúc nên mức này còn tăng). Muốn đổi ngưỡng: màn <b>Cài đặt</b>.
      </div>
      <p
        v-if="overview && !overview.group && !overview.taxable_period"
        class="mt-3 text-xs text-sky-700"
      >
        Kỳ này chưa phát sinh nghĩa vụ thuế: doanh thu lũy kế tới cuối kỳ chưa vượt mức ngưỡng
        {{ fmt(overview.exempt_threshold) }} đ/năm, nên theo Điều 8 khoản 1a Nghị định 68/2026 hộ
        khai, nộp thuế kể từ kỳ phát sinh doanh thu vượt ngưỡng.
      </p>
      <p v-if="overview && !overview.taxable_period" class="mt-3 text-xs text-emerald-600">
        Hộ có doanh thu cả năm không quá mức ngưỡng
        {{ fmt(overview.exempt_threshold) }} đồng được miễn thuế GTGT và thuế TNCN — chỉ cần thông
        báo doanh thu thực tế trong năm với cơ quan thuế (hạn 31/01 năm sau).
      </p>
      <p v-else class="mt-3 text-xs text-gray-400">
        Căn cứ: Nghị định 68/2026/NĐ-CP (bãi bỏ thuế khoán, chuyển sang kê khai), Nghị định
        141/2026/NĐ-CP (ngưỡng miễn thuế 01 tỷ đồng/năm — sửa được ở màn Cài đặt), Luật Thuế GTGT
        2024, Luật Thuế TNCN 2025. Nhóm hộ xếp theo tổng doanh thu cả năm; TNCN nhóm 2 theo doanh
        thu × tỷ lệ ngành hoặc lợi nhuận × 15%; nhóm 3/4 theo lợi nhuận × 17%/20%.
      </p>
    </SectionCard>

    <!-- Tờ khai thuế theo kỳ (To Khai Thue) -->
    <SectionCard>
      <template #icon><i-mdi-clipboard-text-outline class="text-violet-500" /></template>
      <template #title>
        <span>Tờ khai thuế</span>
        <span class="text-xs font-normal text-gray-400">Theo khoảng thời gian đang chọn</span>
      </template>
      <template #actions>
        <Button
          label="Xuất Excel"
          icon="pi pi-file-excel"
          size="small"
          outlined
          :disabled="!declRows.length"
          @click="exportDeclarationExcel"
        />
      </template>
      <div class="flex flex-wrap items-center gap-3 mb-3">
        <span class="text-xs text-gray-500">
          Tờ khai cho khoảng: <b class="text-primary-600">{{ reportPeriodLabel }}</b>
          <span v-if="reportPeriodLabel === 'Chưa chọn khoảng ngày'"> — chọn ở thanh công cụ</span>
        </span>
        <Button
          label="Tải lại tờ khai"
          icon="pi pi-refresh"
          size="small"
          :loading="declLoading"
          @click="loadDeclaration"
        />
        <Tag
          v-if="overview"
          :value="groupLabel"
          :severity="overview.group === 1 ? 'success' : 'warning'"
        />
        <Tag
          v-if="overview?.deadline"
          :value="`Hạn nộp: ${overview.deadline}`"
          severity="info"
          icon="pi pi-calendar-clock"
          class="ml-auto"
        />
        <Tag
          v-else-if="overview"
          value="Khoảng chưa trùng kỳ khai nên không có hạn nộp"
          severity="secondary"
          icon="pi pi-calendar"
          class="ml-auto"
        />
      </div>
      <AppDataTable :value="declRows" :loading="declLoading" stripedRows>
        <Column header="STT" align="center">
          <template #body="{ index }">{{ index + 1 }}</template>
          <template #footer>{{ declRows.length ? "Tổng cộng" : "" }}</template>
        </Column>
        <Column field="industry_name" header="Nhóm ngành nghề" />
        <Column field="industry_code" header="Mã" />
        <Column header="Tỷ lệ GTGT %" align="right">
          <template #body="{ data }">{{ pct(data.vat_rate) }}</template>
        </Column>
        <Column header="Tỷ lệ TNCN %" align="right">
          <template #body="{ data }">{{ pct(data.pit_rate) }}</template>
        </Column>
        <Column header="DT tính thuế GTGT – Tăng" align="right">
          <template #body="{ data }">{{ fmtVnd(data.revenue_up) }}</template>
          <template #footer>{{ declRows.length ? fmtVnd(declTotals.revenue_up) : "" }}</template>
        </Column>
        <Column header="DT tính thuế GTGT – Giảm" align="right">
          <template #body="{ data }">{{ fmtVnd(data.revenue_down) }}</template>
          <template #footer>{{ declRows.length ? fmtVnd(declTotals.revenue_down) : "" }}</template>
        </Column>
        <Column header="Trừ mức trừ TNCN" align="right">
          <template #body="{ data }">{{ fmtVnd(data.pit_deduction) }}</template>
          <template #footer>{{ declRows.length ? fmtVnd(declTotals.pit_deduction) : "" }}</template>
        </Column>
        <Column header="DT tính thuế TNCN (đã trừ mức trừ)" align="right">
          <template #body="{ data }">{{ fmtVnd(data.revenue_taxable) }}</template>
          <template #footer>
            {{ declRows.length ? fmtVnd(declTotals.revenue_taxable) : "" }}
          </template>
        </Column>
        <Column header="Thuế GTGT" align="right">
          <template #body="{ data }">{{ fmtVnd(data.vat_tax) }}</template>
          <template #footer>{{ declRows.length ? fmtVnd(declTotals.vat_tax) : "" }}</template>
        </Column>
        <Column header="Thuế GTGT phải nộp" align="right">
          <template #body="{ data }">
            <b>{{ fmtVnd(data.vat_payable) }}</b>
          </template>
          <template #footer>{{ declRows.length ? fmtVnd(declTotals.vat_payable) : "" }}</template>
        </Column>
        <Column header="Thuế TNCN phải nộp" align="right">
          <template #body="{ data }">{{ fmtVnd(data.pit_tax) }}</template>
          <template #footer>{{ declRows.length ? fmtVnd(declTotals.pit_tax) : "" }}</template>
        </Column>
        <template #empty
          ><EmptyState text="Chưa có dữ liệu doanh thu trong kỳ này." icon="pi pi-chart-line"
        /></template>
      </AppDataTable>
    </SectionCard>

    <!-- Tờ khai 01/CNKD — bản KÊ KHAI 6 dòng theo mẫu cổng (khác mục trên:
         mục trên là báo cáo số thuế phải nộp, mục này là bản để nộp). -->
    <TaxEntryForm
      ref="taxEntry"
      :period-from="declRange.from"
      :period-to="declRange.to"
      :unit-code="unitCode"
      data-testid="tax-entry-form"
    />

    <!-- Bảng cân đối số phát sinh — dư đầu kỳ (DMTK) + phát sinh trong kỳ -->
    <SectionCard title="Bảng cân đối số phát sinh">
      <template #icon><i-mdi-scale-balance class="text-amber-600" /></template>
      <AppDataTable :value="tb" :loading="loading" stripedRows>
        <Column field="code" header="Mã TK" style="width: 90px" />
        <Column field="name" header="Tên tài khoản" />
        <Column field="opening_debit" header="Dư Nợ đầu kỳ" align="right">
          <template #body="{ data }">{{ fmtVnd(data.opening_debit) }}</template>
        </Column>
        <Column field="opening_credit" header="Dư Có đầu kỳ" align="right">
          <template #body="{ data }">{{ fmtVnd(data.opening_credit) }}</template>
        </Column>
        <Column field="debit_mvmt" header="Phát sinh Nợ" align="right">
          <template #body="{ data }">{{ fmtVnd(data.debit_mvmt) }}</template>
        </Column>
        <Column field="credit_mvmt" header="Phát sinh Có" align="right">
          <template #body="{ data }">{{ fmtVnd(data.credit_mvmt) }}</template>
        </Column>
        <Column field="closing_debit" header="Dư Nợ cuối kỳ" align="right">
          <template #body="{ data }">
            <b>{{ fmtVnd(data.closing_debit) }}</b>
          </template>
        </Column>
        <Column field="closing_credit" header="Dư Có cuối kỳ" align="right">
          <template #body="{ data }">
            <b>{{ fmtVnd(data.closing_credit) }}</b>
          </template>
        </Column>
        <template #empty
          ><EmptyState text="Chưa có tài khoản nào trong danh mục (DMTK)." icon="pi pi-book"
        /></template>
      </AppDataTable>
      <div
        v-if="tb.length"
        class="mt-4 flex flex-wrap items-center justify-end gap-6 text-sm border-t pt-3"
      >
        <span class="text-gray-500"
          >Tổng Dư đầu:
          <b class="text-amber-700">{{ fmt(tb.reduce((s, r) => s + r.opening_debit, 0)) }}</b>
          <span class="text-gray-400">/</span>
          <b class="text-amber-700">{{ fmt(tb.reduce((s, r) => s + r.opening_credit, 0)) }}</b>
          đ</span
        >
        <span class="text-gray-500"
          >Phát sinh:
          <b class="text-primary-600">{{ fmt(tb.reduce((s, r) => s + r.debit_mvmt, 0)) }}</b>
          <span class="text-gray-400">/</span>
          <b class="text-primary-600">{{ fmt(tb.reduce((s, r) => s + r.credit_mvmt, 0)) }}</b>
          đ</span
        >
        <span class="text-gray-500"
          >Dư cuối:
          <b class="text-emerald-700">{{ fmt(tb.reduce((s, r) => s + r.closing_debit, 0)) }}</b>
          <span class="text-gray-400">/</span>
          <b class="text-emerald-700">{{ fmt(tb.reduce((s, r) => s + r.closing_credit, 0)) }}</b>
          đ</span
        >
      </div>
      <p class="mt-2 text-xs text-gray-400">
        Số dư đầu kỳ nhập tại màn Tài khoản; phát sinh lấy từ sổ nhật ký (journal_entry) trong
        khoảng ngày đã chọn.
      </p>
    </SectionCard>

    <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
      <!-- Tổng hợp doanh thu - chi phí -->
      <SectionCard title="Tổng hợp doanh thu - chi phí">
        <template #icon><i-mdi-chart-box class="text-emerald-500" /></template>
        <div class="space-y-3">
          <div class="flex justify-between items-center">
            <span class="text-sm text-gray-600">Doanh thu bán hàng</span>
            <b class="text-emerald-600">{{ fmt(re.revenue_up - re.revenue_down) }} đ</b>
          </div>
          <div class="flex justify-between items-center text-xs text-gray-400">
            <span>— trong đó giảm trừ doanh thu: {{ fmt(re.revenue_down) }} đ</span>
          </div>
          <Divider />
          <div class="flex justify-between items-center">
            <span class="text-sm text-gray-600">Chi phí kinh doanh</span>
            <b class="text-red-600">{{ fmt(expenseNet()) }} đ</b>
          </div>
          <div class="flex justify-between items-center text-xs text-gray-400">
            <span>— trong đó giá vốn hàng đã xuất kho (FIFO): {{ fmt(re.cogs) }} đ</span>
          </div>
          <div class="flex justify-between items-center text-xs text-gray-400">
            <span>— chứng từ chi: {{ fmt(re.expense_up) }} đ</span>
          </div>
          <div class="flex justify-between items-center text-xs text-gray-400">
            <span>— trong đó giảm trừ chi phí: {{ fmt(re.expense_down) }} đ</span>
          </div>
          <Divider />
          <div class="flex justify-between items-center text-base">
            <span class="font-semibold">Lợi nhuận trước thuế</span>
            <b class="text-primary-600">{{ fmt(profitNet()) }} đ</b>
          </div>
          <!-- Phiếu xuất trước khi app ghi giá vốn chưa có bút toán 632/152 →
               bảng cân đối thiếu cột giá vốn. Nút ghi bù một lần (idempotent). -->
          <div
            v-if="cogsBackfill.count > 0"
            data-testid="cogs-backfill"
            class="space-y-2 rounded border border-amber-300 bg-amber-50 p-2 text-xs text-amber-800"
          >
            <div>
              <b>{{ cogsBackfill.count }} lượt xuất lịch sử chưa có bút toán giá vốn</b>
              ({{ fmt(cogsBackfill.amount) }} đ): bảng cân đối đang thiếu cột Nợ 632 / Có 152 cho
              phần hàng đã bán này. Bấm ghi bổ sung — tính lại đúng FIFO nên khớp với số trên thẻ
              chi phí; đã ghi rồi thì chạy lại không sinh trùng.
            </div>
            <Button
              label="Ghi bổ sung bút toán 632/152"
              icon="pi pi-plus-circle"
              size="small"
              severity="warning"
              outlined
              :loading="backfilling"
              data-testid="cogs-backfill-run"
              class="w-full"
              @click="doBackfillCogs"
            />
          </div>
        </div>
      </SectionCard>

      <!-- In, xuất tờ khai & sao lưu — sổ đã tách sang tab "Sổ kế toán" -->
      <SectionCard title="In, xuất tờ khai & sao lưu">
        <template #icon><i-mdi-book-open-page-variant class="text-sky-500" /></template>
        <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
          <Button
            label="In báo cáo"
            icon="pi pi-print"
            outlined
            class="justify-start"
            @click="printReport"
          />
          <Button
            label="Xuất tờ khai Excel"
            icon="pi pi-file-excel"
            outlined
            class="justify-start"
            :disabled="!declRows.length"
            @click="exportDeclarationExcel"
          />
          <Button
            label="Xuất báo cáo kỳ thuế (ZIP)"
            icon="pi pi-download"
            outlined
            class="justify-start"
            :loading="reportExporting"
            data-testid="export-tax-report"
            v-tooltip.top="
              'Tờ khai + 7 sổ kế toán theo năm + PDF/XML hóa đơn + sao lưu CSDL, gói 1 file ZIP'
            "
            @click="exportTaxReport"
          />
          <Button
            v-if="auth.isAdmin"
            label="Sao lưu dữ liệu"
            icon="pi pi-database"
            outlined
            class="justify-start"
            @click="openBackupDialog"
          />
        </div>
      </SectionCard>
    </div>

    <!-- Dialog cấu hình -->
    <!-- @saved: reload — nhóm hộ vừa đổi nên tổng hợp thuế phải tính lại. -->
    <BusinessConfigDialog v-model:visible="configDialog" :config="config" @saved="reload" />

    <!-- Dialog cấu hình kê khai thuế -->
    <!-- Tạm nộp & quyết toán thu nhập cá nhân theo năm (Điều 10 khoản 2 NĐ 68/2026) -->
    <SectionCard>
      <template #icon><i-mdi-scale-balance class="text-teal-500" /></template>
      <template #title>
        <span>Tạm nộp &amp; quyết toán thu nhập cá nhân năm {{ declYear }}</span>
        <span class="text-xs font-normal text-gray-400">
          {{
            settlement?.by_revenue
              ? "Hộ nộp theo tỷ lệ % doanh thu — khai theo kỳ"
              : "Tạm nộp theo kỳ, quyết toán cả năm"
          }}
        </span>
      </template>
      <div v-if="settlement" class="text-sm">
        <p v-if="settlement.by_revenue" class="mb-3 rounded bg-sky-50 p-2 text-xs text-sky-800">
          Hộ chọn phương pháp <b>thuế suất × doanh thu tính thuế</b> nên khai thuế TNCN theo quý
          cùng hạn kê khai GTGT; tổng tạm nộp trong năm là số phải nộp, không có bước quyết toán
          riêng (Điều 10 khoản 2 điểm a).
        </p>
        <p v-else class="mb-3 rounded bg-teal-50 p-2 text-xs text-teal-800">
          Hộ chọn phương pháp <b>thu nhập tính thuế × thuế suất</b>: trong năm tạm nộp theo tỷ lệ %
          × doanh thu từng kỳ, kết thúc năm quyết toán (doanh thu − chi phí) ×
          {{ (settlement.final_rate * 100).toFixed(0) }}% và nộp bổ sung hoặc xử lý nộp thừa; hạn
          quyết toán <b>{{ settlement.settle_deadline }}</b
          >.
        </p>
        <div class="mb-3 grid grid-cols-1 gap-3 sm:grid-cols-2 md:grid-cols-4">
          <div>
            <span class="text-xs text-gray-500 block">Tổng TNCN tạm nộp</span>
            <b class="text-teal-700">{{ fmt(settlement.provisional_total) }} đ</b>
          </div>
          <div>
            <span class="text-xs text-gray-500 block">Doanh thu cả năm</span>
            <b>{{ fmt(settlement.year_revenue) }} đ</b>
          </div>
          <div>
            <span class="text-xs text-gray-500 block">Chi phí được trừ cả năm</span>
            <b>{{ fmt(settlement.expense) }} đ</b>
            <div class="text-xs text-gray-400">
              Giá vốn FIFO {{ fmt(settlement.cogs) }} đ + chi khác
              {{ fmt(settlement.other_expense) }} đ
            </div>
          </div>
          <div>
            <span class="text-xs text-gray-500 block">Thu nhập tính thuế</span>
            <b>{{ fmt(settlement.taxable_income) }} đ</b>
          </div>
          <div>
            <span class="text-xs text-gray-500 block">Số phải nộp khi quyết toán</span>
            <b class="text-rose-600">{{ fmt(settlement.final_tax) }} đ</b>
          </div>
          <div class="col-span-1">
            <span class="text-xs text-gray-500 block">Chênh lệch so với tạm nộp</span>
            <b :class="settlement.difference > 0 ? 'text-rose-600' : 'text-emerald-600'">
              {{
                settlement.difference > 0
                  ? "Nộp bổ sung "
                  : settlement.difference < 0
                    ? "Nộp thừa "
                    : ""
              }}{{ fmt(Math.abs(settlement.difference)) }} đ
            </b>
          </div>
        </div>
        <AppDataTable :value="settlement.periods" :loading="settleLoading" stripedRows>
          <Column field="label" header="Kỳ" />
          <Column header="Doanh thu" align="right">
            <template #body="{ data }">{{ fmtVnd(data.revenue) }}</template>
          </Column>
          <Column header="Doanh thu tính thuế" align="right">
            <template #body="{ data }">{{ fmtVnd(data.taxable_revenue) }}</template>
          </Column>
          <Column header="TNCN tạm nộp" align="right">
            <template #body="{ data }"
              ><b>{{ fmtVnd(data.pit_provisional) }} đ</b></template
            >
            <template #footer>{{ fmtVnd(settlement.provisional_total) }} đ</template>
          </Column>
          <Column header="Hạn nộp" align="center">
            <template #body="{ data }">{{ data.deadline || "—" }}</template>
          </Column>
          <template #empty
            ><EmptyState text="Chưa có kỳ khai nào trong năm." icon="pi pi-calendar"
          /></template>
        </AppDataTable>
      </div>
    </SectionCard>

    <TaxConfigDialog
      v-model:visible="taxDialog"
      :period="taxPeriod"
      :method="taxMethod"
      :group="overview?.group ?? null"
      @saved="onTaxConfigSaved"
    />

    <!-- Dialog sao lưu & khôi phục -->
    <BackupDialog v-model:visible="backupDialog" />
  </div>
</template>
