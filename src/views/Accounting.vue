<script setup lang="ts">
import { storeToRefs } from "pinia";
import { useBusinessStore } from "@/stores/business";
import { api } from "@/db";
import { exportXlsx, type XlsxColumn } from "@/utils/excel";
import type { TaxSummaryRow, RevenueExpenseRow, VatReductionRow, TaxDeclarationRow } from "@/types";
import { useAuthStore } from "@/stores/auth";
import { fmtInt as fmt, fmtPct as pct } from "@/utils/format";

const business = useBusinessStore();
const auth = useAuthStore();
const { config } = storeToRefs(business);
const toast = useToast();
const router = useRouter();

const loading = ref(false);
const taxRows = ref<TaxSummaryRow[]>([]);
const vatRows = ref<VatReductionRow[]>([]);
const re = ref<RevenueExpenseRow>({
  revenue_up: 0,
  revenue_down: 0,
  expense_up: 0,
  expense_down: 0,
});

const unitOptions = [{ label: "Toàn bộ", value: "" }, { label: "Hà Nội (HaNoi-01)", value: "HaNoi-01" }];
const unitCode = ref("HaNoi-01");

const fromDate = ref<Date | null>(null);
const toDate = ref<Date | null>(null);

const backupDialog = ref(false);
const configDialog = ref(false);

const iso = (d: Date | null) => (d ? d.toISOString().slice(0, 10) : "");

function openConfig() {
  configDialog.value = true;
}

function openBackupDialog() {
  backupDialog.value = true;
}

async function loadReports() {
  loading.value = true;
  try {
    const f = iso(fromDate.value) || config.value?.report_from || `${config.value?.fiscal_year ?? new Date().getFullYear()}-01-01`;
    const t = iso(toDate.value) || config.value?.report_to || `${config.value?.fiscal_year ?? new Date().getFullYear()}-12-31`;
    const [tax, rev] = await Promise.all([
      api.getTaxSummary(f, t, unitCode.value),
      api.getRevenueExpense(f, t),
      loadVatReduction(),
    ]);
    taxRows.value = tax;
    re.value = rev;
  } catch (e) {
    toast.add({ severity: "error", summary: "Lỗi tải báo cáo", detail: String(e) });
  } finally {
    loading.value = false;
  }
}

async function loadVatReduction() {
  const f = iso(fromDate.value) || config.value?.report_from || `${config.value?.fiscal_year ?? new Date().getFullYear()}-01-01`;
  const t = iso(toDate.value) || config.value?.report_to || `${config.value?.fiscal_year ?? new Date().getFullYear()}-12-31`;
  vatRows.value = await api.getVatReductionList(f, t);
}

const vatReducedTotal = computed(() =>
  vatRows.value.reduce((s, r) => s + r.vat_reduced, 0),
);

// ——— Tờ khai thuế theo kỳ (To Khai Thue) ———
const declRows = ref<TaxDeclarationRow[]>([]);
const declLoading = ref(false);
const declYear = ref(config.value?.fiscal_year ?? new Date().getFullYear());
const declMonth = ref(new Date().getMonth() + 1);
const declPeriodOptions: { label: string; value: number }[] = [
  { label: "Cả năm", value: 0 },
  ...Array.from({ length: 12 }, (_, i) => ({ label: `Tháng ${i + 1}`, value: i + 1 })),
];
const declTotals = computed(() => {
  const t = {
    revenue_up: 0,
    revenue_down: 0,
    vat_tax: 0,
    vat_reduced: 0,
    vat_payable: 0,
    pit_tax: 0,
  };
  for (const r of declRows.value) {
    t.revenue_up += r.revenue_up;
    t.revenue_down += r.revenue_down;
    t.vat_tax += r.vat_tax;
    t.vat_reduced += r.vat_reduced;
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
  exportXlsx(`to-khai-thue-${config.value?.fiscal_year ?? ""}`, cols, rows);
}

function exportVatReductionExcel() {
  const cols: XlsxColumn[] = [
    { header: "Ngày", key: "posting_date" },
    { header: "Số phiếu", key: "voucher_no" },
    { header: "Mã VT", key: "product_code" },
    { header: "Tên hàng hóa DV", key: "product_name" },
    { header: "SL", key: "quantity" },
    { header: "Đơn giá", key: "unit_price" },
    { header: "Thành tiền", key: "amount" },
    { header: "Tỷ lệ quy định (%)", key: "vat_rate" },
    { header: "Tỷ lệ sau giảm (%)", key: "reduced_rate" },
    { header: "Thuế GTGT được giảm", key: "vat_reduced" },
  ];
  const rows = vatRows.value.map((r) => ({
    posting_date: r.posting_date,
    voucher_no: r.voucher_no,
    product_code: r.product_code,
    product_name: r.product_name,
    quantity: r.quantity,
    unit_price: r.unit_price,
    amount: r.amount,
    vat_rate: r.vat_rate * 100,
    reduced_rate: r.reduced_rate * 100,
    vat_reduced: r.vat_reduced,
  }));
  exportXlsx(`bang-ke-giam-thue-gtgt-${config.value?.fiscal_year ?? ""}`, cols, rows);
}

async function loadDeclaration() {
  declLoading.value = true;
  try {
    declRows.value = await api.getTaxDeclaration(declYear.value, declMonth.value);
  } catch (e) {
    toast.add({ severity: "error", summary: "Lỗi tải tờ khai thuế", detail: String(e) });
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
    { header: "Thuế GTGT được giảm", key: "vat_reduced" },
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
    vat_reduced: r.vat_reduced,
    vat_payable: r.vat_payable,
    pit_tax: r.pit_tax,
  }));
  const month = declMonth.value === 0 ? "ca-nam" : String(declMonth.value);
  exportXlsx(`to-khai-thue-ky-${declYear.value}-${month}`, cols, rows);
}

onMounted(async () => {
  await business.load();
  if (business.reportFrom) fromDate.value = new Date(business.reportFrom);
  if (business.reportTo) toDate.value = new Date(business.reportTo);
  if (config.value?.fiscal_year) declYear.value = config.value.fiscal_year;
  await Promise.all([loadReports(), loadDeclaration()]);
});
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
            <Select v-model="unitCode" :options="unitOptions" optionLabel="label" optionValue="value" class="w-44" />
          </div>
          <Button label="Xem báo cáo" icon="pi pi-search" size="small" @click="loadReports" />
          <span v-if="config" class="text-sm text-gray-500 ml-auto">
            Năm tài chính <b>{{ config.fiscal_year }}</b> • {{ config.report_from }} → {{ config.report_to }}
          </span>
        </div>
      </template>
    </Card>

    <!-- Tờ khai thuế theo nhóm ngành -->
    <SectionCard title="Tờ khai thuế (theo nhóm ngành - TT 152/2025)">
      <template #icon><i-mdi-file-certificate class="text-rose-500" /></template>
        <DataTable :value="taxRows" :loading="loading" stripedRows>
          <Column field="industry_code" header="Mã ngành" style="width: 110px" />
          <Column field="industry_name" header="Nhóm ngành nghề" />
          <Column field="vat_rate" header="Thuế GTGT" style="width: 100px" align="right">
            <template #body="{ data }">{{ (data.vat_rate * 100).toFixed(1) }}%</template>
          </Column>
          <Column field="pit_rate" header="Thuế TNCN" style="width: 100px" align="right">
            <template #body="{ data }">{{ (data.pit_rate * 100).toFixed(1) }}%</template>
          </Column>
          <Column field="revenue_up" header="Doanh thu tính thuế" style="width: 160px" align="right">
            <template #body="{ data }">{{ fmt(data.revenue_up) }}</template>
          </Column>
          <Column field="revenue_down" header="Giảm trừ DT" style="width: 130px" align="right">
            <template #body="{ data }">{{ fmt(data.revenue_down) }}</template>
          </Column>
          <Column field="vat_tax" header="Thuế GTGT phải nộp" style="width: 160px" align="right">
            <template #body="{ data }">
              <b>{{ fmt(data.vat_tax) }}</b>
            </template>
          </Column>
          <Column field="pit_tax" header="Thuế TNCN phải nộp" style="width: 160px" align="right">
            <template #body="{ data }">
              <b>{{ fmt(data.pit_tax) }}</b>
            </template>
          </Column>
          <template #empty><EmptyState text="Chưa có dữ liệu doanh thu trong kỳ." icon="pi pi-chart-line" /></template>
        </DataTable>
        <div v-if="taxRows.length" class="mt-4 flex justify-end gap-8 text-sm border-t pt-3">
          <span class="text-gray-500">Tổng thuế GTGT:</span>
          <b class="text-rose-600">{{ fmt(taxRows.reduce((s, r) => s + r.vat_tax, 0)) }} đ</b>
          <span class="text-gray-500 ml-4">Tổng thuế TNCN:</span>
          <b class="text-rose-600">{{ fmt(taxRows.reduce((s, r) => s + r.pit_tax, 0)) }} đ</b>
        </div>
    </SectionCard>

    <!-- Bảng kê giảm thuế GTGT (Giam Thue GTGT) -->
    <SectionCard title="Bảng kê giảm thuế GTGT (Giam Thue GTGT)">
      <template #icon><i-mdi-percent class="text-emerald-500" /></template>
      <template #actions>
        <Button
          label="Xuất Excel"
          icon="pi pi-file-excel"
          size="small"
          outlined
          :disabled="!vatRows.length"
          @click="exportVatReductionExcel"
        />
      </template>
        <DataTable :value="vatRows" :loading="loading" stripedRows paginator :rows="10">
          <Column field="posting_date" header="Ngày" style="width: 110px" />
          <Column field="voucher_no" header="Số phiếu" style="width: 120px" />
          <Column field="product_code" header="Mã VT" style="width: 110px" />
          <Column field="product_name" header="Tên hàng hóa DV" />
          <Column field="quantity" header="SL" style="width: 90px" align="right" />
          <Column field="unit_price" header="Đơn giá" style="width: 130px" align="right">
            <template #body="{ data }">{{ fmt(data.unit_price) }}</template>
          </Column>
          <Column field="amount" header="Thành tiền" style="width: 150px" align="right">
            <template #body="{ data }">{{ fmt(data.amount) }}</template>
          </Column>
          <Column field="vat_rate" header="Tỷ lệ quy định" style="width: 130px" align="right">
            <template #body="{ data }">{{ (data.vat_rate * 100).toFixed(1) }}%</template>
          </Column>
          <Column field="reduced_rate" header="Tỷ lệ sau giảm" style="width: 130px" align="right">
            <template #body="{ data }">{{ (data.reduced_rate * 100).toFixed(1) }}%</template>
          </Column>
          <Column field="vat_reduced" header="Thuế GTGT được giảm" style="width: 160px" align="right">
            <template #body="{ data }">
              <b>{{ fmt(data.vat_reduced) }}</b>
            </template>
          </Column>
          <template #empty><EmptyState text="Không có hàng hóa dịch vụ giảm thuế trong kỳ." icon="pi pi-percentage" /></template>
        </DataTable>
        <div v-if="vatRows.length" class="mt-4 flex justify-end gap-8 text-sm border-t pt-3">
          <span class="text-gray-500">Tổng thuế được giảm:</span>
          <b class="text-emerald-600">{{ fmt(vatReducedTotal) }} đ</b>
        </div>
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
            <label class="text-xs text-gray-500 block mb-1">Kỳ khai thuế</label>
            <Select v-model="declMonth" :options="declPeriodOptions" optionLabel="label" optionValue="value" class="w-44" />
          </div>
          <div>
            <label class="text-xs text-gray-500 block mb-1">Năm</label>
            <InputNumber v-model="declYear" :min="2000" :max="2100" class="w-36" />
          </div>
          <Button label="Tải tờ khai" icon="pi pi-search" size="small" :loading="declLoading" @click="loadDeclaration" />
        </div>
        <DataTable :value="declRows" :loading="declLoading" stripedRows>
          <Column header="STT" style="width: 70px" align="center">
            <template #body="{ index }">{{ index + 1 }}</template>
            <template #footer>{{ declRows.length ? "Tổng cộng" : "" }}</template>
          </Column>
          <Column field="industry_name" header="Nhóm ngành nghề" />
          <Column field="industry_code" header="Mã" style="width: 110px" />
          <Column header="Tỷ lệ GTGT %" style="width: 120px" align="right">
            <template #body="{ data }">{{ pct(data.vat_rate) }}</template>
          </Column>
          <Column header="Tỷ lệ TNCN %" style="width: 120px" align="right">
            <template #body="{ data }">{{ pct(data.pit_rate) }}</template>
          </Column>
          <Column header="DT tính thuế GTGT – Tăng" style="width: 170px" align="right">
            <template #body="{ data }">{{ fmt(data.revenue_up) }}</template>
            <template #footer>{{ declRows.length ? fmt(declTotals.revenue_up) : "" }}</template>
          </Column>
          <Column header="DT tính thuế GTGT – Giảm" style="width: 170px" align="right">
            <template #body="{ data }">{{ fmt(data.revenue_down) }}</template>
            <template #footer>{{ declRows.length ? fmt(declTotals.revenue_down) : "" }}</template>
          </Column>
          <Column header="Thuế GTGT" style="width: 150px" align="right">
            <template #body="{ data }">{{ fmt(data.vat_tax) }}</template>
            <template #footer>{{ declRows.length ? fmt(declTotals.vat_tax) : "" }}</template>
          </Column>
          <Column header="Thuế GTGT được giảm" style="width: 170px" align="right">
            <template #body="{ data }">
              <span class="text-emerald-600">{{ fmt(data.vat_reduced) }}</span>
            </template>
            <template #footer>{{ declRows.length ? fmt(declTotals.vat_reduced) : "" }}</template>
          </Column>
          <Column header="Thuế GTGT phải nộp" style="width: 170px" align="right">
            <template #body="{ data }">
              <b>{{ fmt(data.vat_payable) }}</b>
            </template>
            <template #footer>{{ declRows.length ? fmt(declTotals.vat_payable) : "" }}</template>
          </Column>
          <Column header="Thuế TNCN phải nộp" style="width: 170px" align="right">
            <template #body="{ data }">{{ fmt(data.pit_tax) }}</template>
            <template #footer>{{ declRows.length ? fmt(declTotals.pit_tax) : "" }}</template>
          </Column>
          <template #empty><EmptyState text="Chưa có dữ liệu doanh thu trong kỳ này." icon="pi pi-chart-line" /></template>
        </DataTable>
    </SectionCard>

    <div class="grid grid-cols-2 gap-4">
      <!-- Tổng hợp doanh thu - chi phí -->
      <SectionCard title="Tổng hợp doanh thu - chi phí">
        <template #icon><i-mdi-chart-box class="text-emerald-500" /></template>
          <DataTable :value="[]" :loading="loading" class="hidden">
            <Column field="x" header="x" />
          </DataTable>
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
                {{ fmt((re.revenue_up - re.revenue_down) - (re.expense_up - re.expense_down)) }} đ
              </b>
            </div>
          </div>
      </SectionCard>

      <!-- Sổ sách (mẫu TT 152) -->
      <SectionCard title="Sổ sách theo mẫu TT 152/2025">
        <template #icon><i-mdi-book-open-page-variant class="text-sky-500" /></template>
          <div class="grid grid-cols-2 gap-3">
            <Button label="Sổ S2a-HKD" icon="pi pi-book" outlined class="justify-start" @click="router.push('/ledger')" />
            <Button label="Sổ S2b-HKD" icon="pi pi-book" outlined class="justify-start" @click="router.push('/ledger')" />
            <Button label="Sổ S3a-HKD" icon="pi pi-book" outlined class="justify-start" @click="router.push('/cash')" />
            <Button label="In báo cáo" icon="pi pi-print" outlined class="justify-start" @click="printReport" />
            <Button label="Xuất Excel" icon="pi pi-file-excel" outlined class="justify-start" @click="exportExcel" />
            <Button v-if="auth.isAdmin" label="Sao lưu dữ liệu" icon="pi pi-database" outlined class="justify-start" @click="openBackupDialog" />
          </div>
      </SectionCard>
    </div>

    <!-- Dialog cấu hình -->
    <BusinessConfigDialog v-model:visible="configDialog" :config="config" @saved="loadReports" />

    <!-- Dialog sao lưu & khôi phục -->
    <BackupDialog v-model:visible="backupDialog" />
  </div>
</template>