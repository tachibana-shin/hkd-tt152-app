<script setup lang="ts">
// Popup "Nhập hàng từ Excel" cho phiếu nhập kho: chọn file → CHỌN CỘT (tự dò
// theo tiêu đề, người dùng sửa tay được — file không có hàng tiêu đề chuẩn,
// vd "Phụ lục bảng kê hoạt động kinh doanh" của phần mềm kế toán TT88: tiêu
// đề gộp ô nhiều hàng + mã cột [06]…[15]) → xem trước (phân trang vì file có
// thể vài nghìn dòng) → tạo mặt hàng thiếu 1 lần rồi thêm dòng vào phiếu.
import {
  colLetter,
  columnHints,
  findStart,
  mapInboundRows,
  normInboundName,
  readInboundSheet,
  usedColumns,
  type ColumnMapping,
  type InboundExcelSheet,
} from "@/utils/inboundExcel";
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

/** File đã đọc + ánh xạ tự đoán (null = chưa chọn file). */
const sheet = ref<InboundExcelSheet | null>(null);
/** Cột đang đọc — chỉnh từ các ô Select của panel "Chọn cột dữ liệu". */
const mapping = reactive<ColumnMapping>({
  name: -1,
  unit: -1,
  quantity: -1,
  amount: -1,
  start: 0,
});

/** Đọc lại theo ánh xạ hiện tại — đổi cột là preview đổi ngay. */
const built = computed(() =>
  sheet.value ? mapInboundRows(sheet.value, mapping) : { rows: [], skipped: 0 },
);
const rows = computed(() => built.value.rows);
const skipped = computed(() => built.value.skipped);

/** Danh sách "tên khác" của 1 sản phẩm (lưu phân tách phẩy — F5). */
function splitAliases(aliases?: string): string[] {
  return (aliases ?? "")
    .split(",")
    .map((a) => a.trim())
    .filter(Boolean);
}

/** Tên và tên khác đã chuẩn hoá → hàng hoá trong danh mục. */
const byName = computed(() => {
  const m = new Map<string, Product>();
  for (const p of props.products) {
    for (const key of [p.name, ...splitAliases(p.aliases)].map(normInboundName)) {
      if (key && !m.has(key)) m.set(key, p);
    }
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

// ─── Chọn cột (tự dò trước, sửa tay được) ─────────────────────────────────
/** Cột đang có dữ liệu, kèm nhãn tiêu đề/ô dữ liệu đầu để người chọn dễ nhìn. */
const columnOptions = computed(() => {
  const hints = sheet.value ? columnHints(sheet.value.rows, mapping.start) : [];
  return (sheet.value ? usedColumns(sheet.value.rows) : []).map((c) => ({
    label: `${colLetter(c)}${hints[c] ? ` — ${hints[c]}` : ""}`,
    value: c,
  }));
});

/** Các cột + mục "Không dùng" (ĐVT/Số lượng/Tiền đều bắt buộc được bỏ trống). */
const columnOptionsWithNone = computed(() => [
  { label: "— Không dùng —", value: -1 },
  ...columnOptions.value,
]);

/** Nhóm cột số liệu của phụ lục tồn kho (Số dư đầu kỳ / Nhập / Xuất / Tồn cuối). */
const groupOptions = computed(() =>
  (sheet.value?.groups ?? []).map((g) => ({ label: g.label, value: g.label })),
);

/** Nhóm khớp đúng cặp cột SL + Thành tiền đang chọn (rỗng = đang chọn cột lẻ). */
const matchedGroup = computed(() =>
  sheet.value?.groups.find((g) => g.quantity === mapping.quantity && g.amount === mapping.amount),
);

function applyGroup(label: string) {
  const g = sheet.value?.groups.find((x) => x.label === label);
  if (!g) return;
  mapping.quantity = g.quantity;
  mapping.amount = g.amount;
}

/** "Dòng bắt đầu" hiện ở ô InputNumber theo số đếm trong Excel (1-based). */
const startLine = computed({
  get: () => mapping.start + 1,
  set: (v: number) => {
    mapping.start = Math.max((v || 1) - 1, 0);
  },
});
const maxLine = computed(() => sheet.value?.rows.length ?? 1);

/** Nhãn cột hiển thị trong tóm tắt: A, B, C… (—"chưa chọn"). */
const colName = (c: number) => (c >= 0 ? colLetter(c) : "—");

// Đổi cột (đặc biệt cột Tên) → dò lại hàng đầu vùng dữ liệu. Đổi "Dòng bắt đầu"
// bằng tay không nằm trong danh sách này nên không bị ghi đè.
watch(
  () => [mapping.name, mapping.unit, mapping.quantity, mapping.amount],
  () => {
    if (sheet.value) mapping.start = findStart(sheet.value.rows, mapping);
  },
);

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
    const parsed = await readInboundSheet(file);
    fileName.value = file.name;
    sheet.value = parsed;
    Object.assign(mapping, parsed.mapping);
    keyword.value = "";
    reset();
    if (!mapInboundRows(parsed, mapping).rows.length) {
      toast.add({
        severity: "warn",
        summary: "Chưa đọc được dòng nào",
        detail: "Chọn lại cột Tên mặt hàng · Số lượng · Thành tiền cho khớp file.",
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
      sheet.value = null;
      fileName.value = "";
      keyword.value = "";
      Object.assign(mapping, { name: -1, unit: -1, quantity: -1, amount: -1, start: 0 });
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
        Cột đọc theo ánh xạ chọn tay ở dưới — file không có hàng tiêu đề chuẩn (phụ lục tồn kho
        TT88…) vẫn dùng được. Tên chưa có trong danh mục — kể cả khi trùng một "tên khác" — sẽ tự
        thêm mới; đơn giá = thành tiền ÷ số lượng.
      </p>

      <!-- Chọn cột: file không có tiêu đề → người dùng chỉ định từng cột. -->
      <div
        v-if="sheet"
        class="space-y-3 rounded-md border border-surface-200 p-3 dark:border-surface-700"
        data-testid="excel-mapping"
      >
        <div class="flex flex-wrap items-center gap-2">
          <span class="text-sm font-medium">Chọn cột dữ liệu</span>
          <span class="text-xs text-gray-400">Đã tự dò theo tiêu đề — sửa lại nếu chưa đúng.</span>
        </div>

        <div class="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
          <FormField
            v-if="groupOptions.length > 1"
            label="Nhóm cột số liệu"
            input-id="excel-map-group"
          >
            <Select
              input-id="excel-map-group"
              :model-value="matchedGroup?.label ?? ''"
              :options="groupOptions"
              option-label="label"
              option-value="value"
              placeholder="Tự chọn cột…"
              size="small"
              class="w-full"
              @update:model-value="applyGroup"
            />
            <!-- Ghi chú ngay dưới ô nhóm: nói rõ nó chỉ là phím tắt của 2 ô bên phải. -->
            <span class="text-xs text-gray-400">
              Phím tắt: chọn 1 nhóm để điền sẵn cả 2 cột <b>Số lượng</b> + <b>Thành tiền</b> của
              khối đó; tự đổi 1 trong 2 cột thì ô này chuyển sang “Tự chọn cột…”.
            </span>
          </FormField>
          <FormField label="Tên mặt hàng" input-id="excel-map-name">
            <Select
              input-id="excel-map-name"
              v-model="mapping.name"
              :options="columnOptions"
              option-label="label"
              option-value="value"
              placeholder="Chọn cột…"
              size="small"
              class="w-full"
            />
          </FormField>
          <FormField label="Đơn vị tính" input-id="excel-map-unit">
            <Select
              input-id="excel-map-unit"
              v-model="mapping.unit"
              :options="columnOptionsWithNone"
              option-label="label"
              option-value="value"
              placeholder="Chọn cột…"
              size="small"
              class="w-full"
            />
          </FormField>
          <FormField label="Số lượng" input-id="excel-map-qty">
            <Select
              input-id="excel-map-qty"
              v-model="mapping.quantity"
              :options="columnOptionsWithNone"
              option-label="label"
              option-value="value"
              placeholder="Chọn cột…"
              size="small"
              class="w-full"
            />
          </FormField>
          <FormField label="Thành tiền" input-id="excel-map-amount">
            <Select
              input-id="excel-map-amount"
              v-model="mapping.amount"
              :options="columnOptionsWithNone"
              option-label="label"
              option-value="value"
              placeholder="Chọn cột…"
              size="small"
              class="w-full"
            />
          </FormField>
          <FormField label="Dòng bắt đầu dữ liệu" input-id="excel-map-start">
            <InputNumber
              input-id="excel-map-start"
              v-model="startLine"
              :min="1"
              :max="maxLine"
              :use-grouping="false"
              size="small"
              class="w-full"
            />
          </FormField>
        </div>

        <p class="text-xs text-gray-400" data-testid="excel-mapping-detail">
          Đọc cột {{ colName(mapping.name) }} (tên) · {{ colName(mapping.unit) }} (ĐVT) ·
          {{ colName(mapping.quantity) }} (số lượng) · {{ colName(mapping.amount) }} (thành tiền) —
          dữ liệu từ dòng {{ startLine }}.
        </p>

        <Message v-if="!rowCount && !parsing" severity="warn" :closable="false" class="w-full">
          Chưa đọc được dòng nào — kiểm tra lại cột <b>Tên mặt hàng</b> và dòng bắt đầu.
        </Message>
      </div>

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
        v-else-if="!parsing && !sheet"
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
