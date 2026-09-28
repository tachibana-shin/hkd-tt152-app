<script setup lang="ts">
import { storeToRefs } from "pinia";
import { useBusinessStore } from "@/stores/business";
import { api } from "@/db";
import { exportXlsx, type XlsxColumn } from "@/utils/excel";
import type {
  TaxSummaryRow,
  RevenueExpenseRow,
  TrialBalanceRow,
  TaxDeclarationRow,
  TaxOverview,
} from "@/types";
import { useAuthStore } from "@/stores/auth";
import { fmtInt as fmt, fmtPct as pct, fmtVnd, toIsoDate } from "@/utils/format";
import { describeRange, taxPeriodRange } from "@/utils/period";
import { useKeepAliveRefresh } from "@/composables/useKeepAliveRefresh";

const business = useBusinessStore();
const auth = useAuthStore();
const { config } = storeToRefs(business);
const toast = useToast();
const router = useRouter();

const loading = ref(false);
const taxRows = ref<TaxSummaryRow[]>([]);
const tb = ref<TrialBalanceRow[]>([]);
const re = ref<RevenueExpenseRow>({
  revenue_up: 0,
  revenue_down: 0,
  expense_up: 0,
  expense_down: 0,
});

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
    const [tax, rev, balance] = await Promise.all([
      api.getTaxSummary(f, t, unitCode.value),
      api.getRevenueExpense(f, t),
      api.getTrialBalance(f, t),
    ]);
    taxRows.value = tax;
    re.value = rev;
    tb.value = balance;
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

// ——— Tờ khai thuế theo kỳ (To Khai Thue) + tổng hợp thuế theo quy định 2026 ———
const declRows = ref<TaxDeclarationRow[]>([]);
const declLoading = ref(false);
const overview = ref<TaxOverview | null>(null);
const declYear = ref(new Date().getFullYear());
// Bộ chọn kỳ để XEM tờ khai: quý / tháng / năm (mặc định theo kỳ khai đã cấu hình)
const declPeriod = ref<"year" | "quarter" | "month">("quarter");
const declPeriodNo = ref(1);
const declPeriodTypeOptions: { label: string; value: "year" | "quarter" | "month" }[] = [
  { label: "Theo quý", value: "quarter" },
  { label: "Theo tháng", value: "month" },
  { label: "Theo năm", value: "year" },
];
const declPeriodNoOptions = computed(() => {
  if (declPeriod.value === "quarter")
    return [1, 2, 3, 4].map((q) => ({ label: `Quý ${q}`, value: q }));
  if (declPeriod.value === "month")
    return Array.from({ length: 12 }, (_, i) => ({
      label: `Tháng ${i + 1}`,
      value: i + 1,
    }));
  return [];
});

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
const groupLabel = computed(() => {
  switch (overview.value?.group) {
    case 1:
      return "Nhóm 1 — doanh thu ≤ 1 tỷ: miễn thuế";
    case 2:
      return "Nhóm 2 — doanh thu > 1 tỷ đến 3 tỷ";
    case 3:
      return "Nhóm 3 — doanh thu > 3 tỷ đến 50 tỷ";
    default:
      return "Nhóm 4 — doanh thu > 50 tỷ";
  }
});
const declTotals = computed(() => {
  const t = {
    revenue_up: 0,
    revenue_down: 0,
    vat_tax: 0,
    vat_payable: 0,
    pit_tax: 0,
  };
  for (const r of declRows.value) {
    t.revenue_up += r.revenue_up;
    t.revenue_down += r.revenue_down;
    t.vat_tax += r.vat_tax;
    t.vat_payable += r.vat_payable;
    t.pit_tax += r.pit_tax;
  }
  return t;
});

function printReport() {
  window.print();
}

function exportExcel() {
  const cols: XlsxColumn[] = [
    { header: "Mã ngành", key: "industry_code" },
    { header: "Nhóm ngành", key: "industry_name" },
    { header: "Thuế GTGT %", key: "vat_percent" },
    { header: "Thuế TNCN %", key: "pit_percent" },
    { header: "Doanh thu tính thuế", key: "revenue_up" },
    { header: "Giảm trừ DT", key: "revenue_down" },
    { header: "Thuế GTGT phải nộp", key: "vat_tax" },
    { header: "Thuế TNCN phải nộp", key: "pit_tax" },
  ];
  const rows = taxRows.value.map((r) => ({
    industry_code: r.industry_code,
    industry_name: r.industry_name,
    vat_percent: r.vat_rate * 100,
    pit_percent: r.pit_rate * 100,
    revenue_up: r.revenue_up,
    revenue_down: r.revenue_down,
    vat_tax: r.vat_tax,
    pit_tax: r.pit_tax,
  }));
  exportXlsx(`to-khai-thue-${new Date().getFullYear()}`, cols, rows);
}

async function loadDeclaration() {
  declLoading.value = true;
  try {
    const no = declPeriod.value === "year" ? 0 : declPeriodNo.value;
    const [rows, ov] = await Promise.all([
      api.getTaxDeclaration(declYear.value, declPeriod.value, no),
      api.getTaxOverview(declYear.value, declPeriod.value, no),
    ]);
    declRows.value = rows;
    overview.value = ov;
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

function exportDeclarationExcel() {
  const cols: XlsxColumn[] = [
    { header: "Nhóm ngành nghề", key: "industry_name" },
    { header: "Mã", key: "industry_code" },
    { header: "Tỷ lệ GTGT %", key: "vat_percent" },
    { header: "Tỷ lệ TNCN %", key: "pit_percent" },
    { header: "DT tính thuế GTGT – Tăng", key: "revenue_up" },
    { header: "DT tính thuế GTGT – Giảm", key: "revenue_down" },
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
    vat_tax: r.vat_tax,
    vat_payable: r.vat_payable,
    pit_tax: r.pit_tax,
  }));
  const periodLabel =
    declPeriod.value === "year" ? "ca-nam" : `${declPeriod.value}-${declPeriodNo.value}`;
  exportXlsx(`to-khai-thue-ky-${declYear.value}-${periodLabel}`, cols, rows);
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
  if (
    (payload.period === "month" && declPeriod.value !== "month") ||
    (payload.period === "quarter" && declPeriod.value !== "quarter")
  ) {
    declPeriod.value = payload.period === "month" ? "month" : "quarter";
    const now = new Date();
    declPeriodNo.value =
      declPeriod.value === "quarter" ? Math.floor(now.getMonth() / 3) + 1 : now.getMonth() + 1;
  } else if (
    (payload.period === "year" || payload.period === "per_occurrence") &&
    declPeriod.value !== "year"
  ) {
    declPeriod.value = "year";
    declPeriodNo.value = 0;
  }
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
    // Mặc định xem theo đúng kỳ khai của hộ (per_occurrence xem theo năm).
    if (taxPeriod.value === "month") declPeriod.value = "month";
    else if (taxPeriod.value === "year" || taxPeriod.value === "per_occurrence")
      declPeriod.value = "year";
    else declPeriod.value = "quarter";
    const now = new Date();
    if (declPeriod.value === "quarter") declPeriodNo.value = Math.floor(now.getMonth() / 3) + 1;
    else if (declPeriod.value === "month") declPeriodNo.value = now.getMonth() + 1;
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
          <Button label="Xem báo cáo" icon="pi pi-search" size="small" @click="loadReports" />
          <span v-if="config" class="text-sm text-gray-500 ml-auto">
            <i class="pi pi-calendar mr-1" />Kỳ khai thuế: <b>{{ taxPeriodLabel }}</b> • TNCN:
            <b>{{ taxMethodLabel }}</b>
            <span class="mx-1 text-gray-300">|</span>
            Đang xem:
            <b class="text-primary-600">{{ reportPeriodLabel }}</b>
          </span>
        </div>
      </template>
    </Card>

    <!-- Tờ khai thuế theo nhóm ngành -->
    <SectionCard title="Tờ khai thuế (theo nhóm ngành - TT 152/2025)">
      <template #icon><i-mdi-file-certificate class="text-rose-500" /></template>
      <AppDataTable :value="taxRows" :loading="loading" stripedRows>
        <Column field="industry_code" header="Mã ngành" />
        <Column field="industry_name" header="Nhóm ngành nghề" />
        <Column field="vat_rate" header="Thuế GTGT" align="right">
          <template #body="{ data }">{{ (data.vat_rate * 100).toFixed(1) }}%</template>
        </Column>
        <Column field="pit_rate" header="Thuế TNCN" align="right">
          <template #body="{ data }">{{ (data.pit_rate * 100).toFixed(1) }}%</template>
        </Column>
        <Column field="revenue_up" header="Doanh thu tính thuế" align="right">
          <template #body="{ data }">{{ fmtVnd(data.revenue_up) }}</template>
        </Column>
        <Column field="revenue_down" header="Giảm trừ DT" align="right">
          <template #body="{ data }">{{ fmtVnd(data.revenue_down) }}</template>
        </Column>
        <Column field="vat_tax" header="Thuế GTGT phải nộp" align="right">
          <template #body="{ data }">
            <b>{{ fmtVnd(data.vat_tax) }}</b>
          </template>
        </Column>
        <Column field="pit_tax" header="Thuế TNCN phải nộp" align="right">
          <template #body="{ data }">
            <b>{{ fmtVnd(data.pit_tax) }}</b>
          </template>
        </Column>
        <template #empty
          ><EmptyState text="Chưa có dữ liệu doanh thu trong kỳ." icon="pi pi-chart-line"
        /></template>
      </AppDataTable>
      <div v-if="taxRows.length" class="mt-4 flex justify-end gap-8 text-sm border-t pt-3">
        <span class="text-gray-500">Tổng thuế GTGT:</span>
        <b class="text-rose-600">{{ fmt(taxRows.reduce((s, r) => s + r.vat_tax, 0)) }} đ</b>
        <span class="text-gray-500 ml-4">Tổng thuế TNCN:</span>
        <b class="text-rose-600">{{ fmt(taxRows.reduce((s, r) => s + r.pit_tax, 0)) }} đ</b>
      </div>
    </SectionCard>

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
          <span class="text-gray-500 block text-xs mb-1">Chi phí kỳ</span>
          <b>{{ fmt(overview.expense) }} đ</b>
        </div>
        <div>
          <span class="text-gray-500 block text-xs mb-1">Lợi nhuận kỳ</span>
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
        <div class="col-span-2 md:col-span-2">
          <span class="text-gray-500 block text-xs mb-1">Tổng thuế phải nộp kỳ</span>
          <b class="text-primary-600 text-lg">{{ fmt(overview.total_tax) }} đ</b>
        </div>
      </div>
      <p v-if="overview?.group === 1" class="mt-3 text-xs text-emerald-600">
        Hộ có doanh thu cả năm ≤ 1 tỷ đồng được miễn thuế GTGT và thuế TNCN — chỉ cần thông báo
        doanh thu thực tế trong năm với cơ quan thuế (hạn 31/01 năm sau).
      </p>
      <p v-else class="mt-3 text-xs text-gray-400">
        Căn cứ: Nghị định 68/2026/NĐ-CP (bãi bỏ thuế khoán, chuyển sang kê khai), Nghị định
        141/2026/NĐ-CP (ngưỡng miễn thuế 1 tỷ đồng/năm), Luật Thuế GTGT 2024, Luật Thuế TNCN 2025.
        Nhóm hộ xếp theo tổng doanh thu cả năm; TNCN nhóm 2 theo doanh thu × tỷ lệ ngành hoặc lợi
        nhuận × 15%; nhóm 3/4 theo lợi nhuận × 17%/20%.
      </p>
    </SectionCard>

    <!-- Tờ khai thuế theo kỳ (To Khai Thue) -->
    <SectionCard>
      <template #icon><i-mdi-clipboard-text-outline class="text-violet-500" /></template>
      <template #title>
        <span>Tờ khai thuế theo kỳ</span>
        <span class="text-xs font-normal text-gray-400">Nhóm theo ngành nghề</span>
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
        <div>
          <label class="text-xs text-gray-500 block mb-1">Loại kỳ</label>
          <Select
            v-model="declPeriod"
            :options="declPeriodTypeOptions"
            optionLabel="label"
            optionValue="value"
            class="w-36"
            @change="loadDeclaration"
          />
        </div>
        <div v-if="declPeriod !== 'year'">
          <label class="text-xs text-gray-500 block mb-1">Kỳ</label>
          <Select
            v-model="declPeriodNo"
            :options="declPeriodNoOptions"
            optionLabel="label"
            optionValue="value"
            class="w-32"
            @change="loadDeclaration"
          />
        </div>
        <div>
          <label class="text-xs text-gray-500 block mb-1">Năm</label>
          <InputNumber
            v-model="declYear"
            :min="2000"
            :max="2100"
            class="w-32"
            @input="loadDeclaration"
          />
        </div>
        <Button
          label="Tải tờ khai"
          icon="pi pi-search"
          size="small"
          :loading="declLoading"
          @click="loadDeclaration"
        />
        <Tag
          v-if="overview"
          :value="groupLabel"
          :severity="overview.group === 1 ? 'success' : 'warning'"
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
        <AppDataTable
          :value="[]"
          :loading="loading"
          class="hidden"
          :resizable-columns="false"
          :sortable="false"
        >
          <Column field="x" header="x" />
        </AppDataTable>
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
            <b class="text-red-600">{{ fmt(re.expense_up - re.expense_down) }} đ</b>
          </div>
          <div class="flex justify-between items-center text-xs text-gray-400">
            <span>— trong đó giảm trừ chi phí: {{ fmt(re.expense_down) }} đ</span>
          </div>
          <Divider />
          <div class="flex justify-between items-center text-base">
            <span class="font-semibold">Lợi nhuận trước thuế</span>
            <b class="text-primary-600">
              {{ fmt(re.revenue_up - re.revenue_down - (re.expense_up - re.expense_down)) }}
              đ
            </b>
          </div>
        </div>
      </SectionCard>

      <!-- Sổ sách (mẫu TT 152) -->
      <SectionCard title="Sổ sách theo mẫu TT 152/2025">
        <template #icon><i-mdi-book-open-page-variant class="text-sky-500" /></template>
        <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
          <Button
            label="Sổ S2a-HKD"
            icon="pi pi-book"
            outlined
            class="justify-start"
            @click="router.push('/ledger')"
          />
          <Button
            label="Sổ S2b-HKD"
            icon="pi pi-book"
            outlined
            class="justify-start"
            @click="router.push('/ledger')"
          />
          <Button
            label="Sổ S3a-HKD"
            icon="pi pi-book"
            outlined
            class="justify-start"
            @click="router.push('/cash')"
          />
          <Button
            label="In báo cáo"
            icon="pi pi-print"
            outlined
            class="justify-start"
            @click="printReport"
          />
          <Button
            label="Xuất Excel"
            icon="pi pi-file-excel"
            outlined
            class="justify-start"
            @click="exportExcel"
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
