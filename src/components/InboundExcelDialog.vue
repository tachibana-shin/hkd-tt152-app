<script setup lang="ts">
// Popup "Nhập hàng từ Excel" cho phiếu nhập kho: chọn file → xem trước (có phân
// trang vì file có thể vài nghìn dòng) → tạo mặt hàng thiếu 1 lần rồi thêm dòng
// vào phiếu. Cột đọc theo THỨ TỰ: Tên mặt hàng · Đơn vị tính · Số lượng · Tổng tiền.
import { normInboundName, readInboundExcel, type InboundExcelRow } from "@/utils/inboundExcel";
import { api } from "@/db";
import { fmtVnd } from "@/utils/format";
import type { InboundExcelLine, Product } from "@/types";
import { useClientPage } from "@/composables/useClientPage";

const props = defineProps<{
  visible: boolean;
  /** Danh mục hiện có — để dò tên đã tồn tại (tránh tạo trùng). */
  products: Product[];
}>();

const emit = defineEmits<{
  (e: "update:visible", value: boolean): void;
  (e: "confirm", lines: InboundExcelLine[]): void;
}>();

const toast = useToast();
const fileInput = ref<HTMLInputElement | null>(null);
const fileName = ref("");
const parsing = ref(false);
const saving = ref(false);
const rows = ref<InboundExcelRow[]>([]);
const skipped = ref(0);

/** Tên đã chuẩn hoá → hàng hoá trong danh mục. */
const byName = computed(() => {
  const m = new Map<string, Product>();
  for (const p of props.products) {
    const k = normInboundName(p.name);
    if (k && !m.has(k)) m.set(k, p);
  }
  return m;
});

/** Dòng preview: đã dò được mã hàng hay phải tạo mới. */
const preview = computed(() =>
  rows.value.map((r) => {
    const hit = byName.value.get(normInboundName(r.name));
    return { ...r, product_code: hit?.code ?? "", is_new: !hit };
  }),
);

/** Mặt hàng CHƯA có trong danh mục, gộp trùng tên (gộp cả khác hoa/thường/cách). */
const newOnes = computed(() => {
  const seen = new Map<string, { name: string; unit: string; cost_price: number }>();
  for (const r of preview.value) {
    const k = normInboundName(r.name);
    if (r.is_new && !seen.has(k)) {
      seen.set(k, { name: r.name, unit: r.unit || "Cái", cost_price: r.unit_price });
    }
  }
  return [...seen.values()];
});

const totalAmount = computed(() => preview.value.reduce((s, r) => s + r.amount, 0));

/** Số dòng của FILE — tóm tắt và nút thêm luôn tính theo toàn bộ file. */
const rowCount = computed(() => preview.value.length);

// Ô "Tìm kiếm…" ở header bảng (AppDataTable có sẵn cho mọi bảng) lọc nhanh tên/
// mã hàng khi xem trước file vài nghìn dòng. Bảng chạy lazy nên PrimeVue không
// tự lọc → tự lọc ngay ở đây; tóm tắt/nút thêm vẫn theo toàn bộ file.
const keyword = ref("");
const filtered = computed(() => {
  const q = keyword.value.trim().toLowerCase();
  if (!q) return preview.value;
  return preview.value.filter(
    (r) =>
      r.name.toLowerCase().includes(q) ||
      (!!r.product_code && r.product_code.toLowerCase().includes(q)),
  );
});

// Preview phân trang phía client — file lớn không render hàng nghìn dòng 1 lúc.
const {
  first,
  rows: pageRows,
  totalRows,
  paged,
  showPager,
  onPage,
  reset,
} = useClientPage(() => filtered.value, 20);

function pick() {
  fileInput.value?.click();
}

async function onFile(e: Event) {
  const input = e.target as HTMLInputElement;
  const file = input.files?.[0];
  if (!file) return;
  parsing.value = true;
  try {
    const parsed = await readInboundExcel(file);
    fileName.value = file.name;
    rows.value = parsed.rows;
    skipped.value = parsed.skipped;
    keyword.value = "";
    reset();
    if (!parsed.rows.length) {
      toast.add({
        severity: "warn",
        summary: "Không có dòng dữ liệu nào",
        detail:
          "Cần ít nhất cột Tên mặt hàng và số liệu (cột theo thứ tự: Tên · ĐVT · Số lượng · Tổng tiền)",
      });
    }
  } catch (err) {
    toast.add({ severity: "error", summary: "Không đọc được file Excel", detail: String(err) });
  } finally {
    parsing.value = false;
    input.value = ""; // cho phép chọn lại đúng file đó
  }
}

/** Tạo mặt hàng thiếu (1 lần gọi cho cả file) rồi ghép mã cho từng dòng. */
async function confirm() {
  saving.value = true;
  try {
    const codeByNorm = new Map<string, string>();
    if (newOnes.value.length) {
      const res = await api.saveProductsBulk(newOnes.value);
      for (const [name, code] of Object.entries(res.map))
        codeByNorm.set(normInboundName(name), code);
    }
    const lines: InboundExcelLine[] = preview.value.map((r) => ({
      product_code: codeByNorm.get(normInboundName(r.name)) ?? r.product_code,
      quantity: r.quantity,
      unit_price: r.unit_price,
      discount: 0,
    }));
    const missing = lines.filter((l) => !l.product_code).length;
    if (missing > 0) throw new Error(`${missing} dòng chưa gán được mã mặt hàng`);
    emit("confirm", lines);
    emit("update:visible", false);
  } catch (err) {
    toast.add({ severity: "error", summary: "Không nhập được file", detail: String(err) });
  } finally {
    saving.value = false;
  }
}

// Mở lại dialog → bắt đầu từ đầu (không giữ file đã chọn lần trước).
watch(
  () => props.visible,
  (v) => {
    if (v) {
      rows.value = [];
      skipped.value = 0;
      fileName.value = "";
      keyword.value = "";
      reset();
    }
  },
);
</script>

<template>
  <input
    ref="fileInput"
    type="file"
    accept=".xlsx,.xls"
    class="hidden"
    data-testid="inbound-excel-file"
    @change="onFile"
  />

  <AppDialog
    :visible="visible"
    header="Nhập hàng từ Excel"
    width="max-w-4xl"
    cancel-label="Đóng"
    :show-action="!!rowCount"
    :action-label="`Thêm ${rowCount} dòng vào phiếu`"
    action-icon="pi pi-plus"
    :saving="saving"
    :action-disabled="parsing"
    @update:visible="emit('update:visible', $event)"
    @action="confirm"
  >
    <div class="space-y-3 py-1">
      <div class="flex flex-wrap items-center gap-2">
        <Button
          label="Chọn file Excel"
          icon="pi pi-file-excel"
          size="small"
          :loading="parsing"
          @click="pick"
        />
        <span v-if="fileName" class="text-sm text-gray-600">{{ fileName }}</span>
      </div>
      <p class="text-xs text-gray-400">
        Cột theo thứ tự: <b>Tên mặt hàng · Đơn vị tính · Số lượng tồn · Tổng tiền</b> (nhãn cột có
        thể khác chữ, thứ tự vẫn giữ nguyên). Tên chưa có trong danh mục sẽ tự thêm mới; đơn giá =
        tổng tiền ÷ số lượng.
      </p>

      <div
        v-if="rowCount"
        class="flex flex-wrap gap-x-5 gap-y-1 text-sm"
        data-testid="excel-preview-summary"
      >
        <span
          ><b>{{ rowCount }}</b> dòng</span
        >
        <span class="text-amber-600">
          <b>{{ newOnes.length }}</b> mặt hàng mới (tự thêm danh mục)
        </span>
        <span v-if="skipped" class="text-gray-500">{{ skipped }} dòng thiếu số liệu — bỏ qua</span>
        <span
          >Tổng tiền: <b>{{ fmtVnd(totalAmount) }}</b></span
        >
      </div>

      <div
        v-else-if="!parsing"
        class="rounded-md border border-dashed px-4 py-6 text-center text-sm text-gray-500"
      >
        Chưa chọn file — bấm "Chọn file Excel" để xem trước dữ liệu.
      </div>

      <AppDataTable
        v-if="rowCount"
        :value="paged"
        :first="first"
        :rows="pageRows"
        :total-records="totalRows"
        :paginator="showPager"
        :rows-per-page-options="[20, 50, 100, 200]"
        :resizable-columns="false"
        size="small"
        @page="onPage"
        @search="keyword = $event"
      >
        <Column header="STT" style="width: 56px">
          <template #body="{ index }">
            {{ first + index + 1 }}
          </template>
        </Column>
        <Column header="Tên mặt hàng" style="min-width: 180px">
          <template #body="{ data }">{{ data.name }}</template>
        </Column>
        <Column header="ĐVT" style="width: 80px">
          <template #body="{ data }">{{ data.unit || "—" }}</template>
        </Column>
        <Column header="Số lượng" style="width: 96px" align-right>
          <template #body="{ data }">{{ data.quantity }}</template>
        </Column>
        <Column header="Tổng tiền" style="width: 120px" align-right>
          <template #body="{ data }">{{ fmtVnd(data.amount) }}</template>
        </Column>
        <Column header="Đơn giá" style="width: 120px" align-right>
          <template #body="{ data }">{{ fmtVnd(data.unit_price) }}</template>
        </Column>
        <Column header="Mặt hàng" style="width: 130px">
          <template #body="{ data }">
            <Tag v-if="data.is_new" value="Tạo mới" severity="warn" />
            <span v-else class="text-xs">{{ data.product_code }}</span>
          </template>
        </Column>
        <template #empty>
          <EmptyState text="Không có dòng nào khớp từ khoá" icon="pi pi-search" />
        </template>
      </AppDataTable>
    </div>
  </AppDialog>
</template>
