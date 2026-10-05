<script setup lang="ts">
/**
 * Lô sản xuất — danh sách + tạo lô thành phẩm từ định mức vật tư.
 *
 * 1 lô = 1 phiếu nhập kho loại `production`: chọn thành phẩm + sản lượng, app
 * hiện trước NVL = định mức × sản lượng (kèm tồn kho của kho đang chọn) và giá
 * thành ước tính = Σ (NVL × giá vốn FIFO trong kho đó). Lưu gọi `save_inbound`
 * với `inbound_type: production` + `autoBom: true` → backend trừ tồn NVL và tự
 * sinh phiếu xuất NVL (Nợ 154 / Có 152) trong CÙNG transaction; thiếu tồn thì
 * cả lô không được lưu.
 *
 * Danh sách đọc đầu phiếu nhập loại `production` (lệnh `get_production_lots`)
 * — màn Nhập kho vẫn hiện đủ mọi phiếu nhập như trước.
 */
import { storeToRefs } from "pinia";
import { useAuthStore } from "@/stores/auth";
import { useCatalogStore } from "@/stores/catalog";
import { useStockStore } from "@/stores/stock";
import { api } from "@/db";
import { fmtDec, fmtVnd, toIsoDate } from "@/utils/format";
import type { BomItem, ProductionLotRow, StockLot } from "@/types";
import { useKeepAliveRefresh } from "@/composables/useKeepAliveRefresh";
import { useLazyPage } from "@/composables/useLazyPage";

const auth = useAuthStore();
const catalog = useCatalogStore();
const stockStore = useStockStore();
const { warehouses } = storeToRefs(catalog);
const { loading } = storeToRefs(stockStore);
const toast = useToast();

// ─── Danh sách lô (phân trang server-side) ───
const lots = ref<ProductionLotRow[]>([]);
const total = ref(0);
const f = reactive({ search: "", from: null as Date | null, to: null as Date | null });

const page = useLazyPage(async (lazy) => {
  loading.value = true;
  try {
    const res = await api.getProductionLots(lazy, toIsoDate(f.from), toIsoDate(f.to));
    lots.value = res.rows;
    total.value = res.total;
  } catch (e) {
    toast.add({ severity: "error", summary: "Không tải được lô sản xuất", detail: String(e) });
  } finally {
    loading.value = false;
  }
});

/** Đổi khoảng ngày / từ khoá tìm kiếm → nạp lại trang đầu. */
function loadLots() {
  return page.setKeyword(f.search.trim());
}

// ─── Xem phiếu (chỉ đọc) — dùng chung VoucherViewDialog ───
const viewVoucherNo = ref("");
const viewVoucherVisible = ref(false);

function openVoucher(no: string) {
  viewVoucherNo.value = no;
  viewVoucherVisible.value = true;
}

// ─── Tạo lô ───
const dialog = ref(false);
const saving = ref(false);

const form = reactive({
  product_code: "",
  quantity: 1,
  posting_date: new Date(),
  warehouse_code: "",
  voucher_no: "",
  description: "Nhập kho thành phẩm — lô sản xuất",
});

// Định mức + lô tồn kho (để tính giá thành và soi tồn trước khi lưu).
const bomItems = ref<BomItem[]>([]);
const stockLots = ref<StockLot[]>([]);

async function openCreate() {
  Object.assign(form, {
    product_code: "",
    quantity: 1,
    posting_date: new Date(),
    warehouse_code: warehouses.value[0]?.code ?? "",
    voucher_no: "",
    description: "Nhập kho thành phẩm — lô sản xuất",
  });
  dialog.value = true;
  try {
    // Số lô do backend sinh (PN0001…) — không đoán từ danh sách đang lọc.
    [form.voucher_no, stockLots.value] = await Promise.all([
      api.nextVoucherNo("PN"),
      api.getStockLots(""),
    ]);
  } catch (e) {
    toast.add({ severity: "error", summary: "Không mở được hộp thoại", detail: String(e) });
  }
}

/** 1 dòng NVL của bản xem trước: định mức × sản lượng, tồn kho, giá vốn FIFO. */
type PreviewLine = {
  code: string;
  name: string;
  unit: string;
  /** Định mức của 1 thành phẩm. */
  perUnit: number;
  /** NVL cần cho toàn bộ sản lượng. */
  need: number;
  /** Tồn kho trong kho đang chọn. */
  onHand: number;
  /** Phần thiếu (0 = đủ). */
  shortage: number;
  /** Đơn giá vốn bình quân của phần sẽ xuất (FIFO). */
  unitCost: number;
  /** Tiền NVL của dòng. */
  cost: number;
};

const preview = computed<PreviewLine[]>(() => {
  const qty = form.quantity > 0 ? form.quantity : 0;
  return bomItems.value
    .filter((b) => b.product_code === form.product_code)
    .map((b) => {
      // Làm tròn 6 chữ số — nhân float hay dính nhiễu, đúng cách backend tính.
      const need = Math.round(b.quantity * qty * 1e6) / 1e6;
      const fifos = stockLots.value
        .filter(
          (l) => l.product_code === b.material_code && l.warehouse_code === form.warehouse_code,
        )
        .sort((a, c) => a.received_at.localeCompare(c.received_at) || a.id - c.id);
      const onHand = fifos.reduce((s, l) => s + l.quantity, 0);
      // Trừ FIFO đúng như backend khi lưu để giá thành ước tính không lệch.
      let left = need;
      let cost = 0;
      for (const lot of fifos) {
        if (left <= 0) break;
        const take = Math.min(left, lot.quantity);
        cost += take * lot.unit_cost;
        left -= take;
      }
      return {
        code: b.material_code,
        name: b.material_name,
        unit: b.material_unit,
        perUnit: b.quantity,
        need,
        onHand,
        shortage: Math.max(0, Math.round((need - onHand) * 1e6) / 1e6),
        unitCost: need > 0 ? cost / need : 0,
        cost,
      };
    });
});

const totalCost = computed(() => preview.value.reduce((s, l) => s + l.cost, 0));
const shortages = computed(() => preview.value.filter((l) => l.shortage > 0));
const hasBom = computed(() => preview.value.length > 0);

/** Đơn giá thành phẩm ghi trên phiếu nhập = giá thành ước tính ÷ sản lượng. */
const unitPrice = computed(() => (form.quantity > 0 ? totalCost.value / form.quantity : 0));

/**
 * Gõ sản lượng → tính lại bản xem trước NGAY, không phải chờ rời ô.
 * `InputNumber` chỉ gộp model khi blur/Enter, nên nghe thêm sự kiện `input`
 * (PrimeVue bắn mỗi lần gõ được chữ số) để bảng NVL + giá thành chạy thật.
 */
function onQtyInput(e: { value?: string | number | null }) {
  const n = Number(e?.value);
  form.quantity = Number.isFinite(n) ? n : 0;
}

/** Lý do chưa tạo được lô (rỗng = tạo được). */
const blockReason = computed(() => {
  if (!form.product_code) return "Chưa chọn thành phẩm";
  if (!(form.quantity > 0)) return "Sản lượng phải lớn hơn 0";
  if (!form.warehouse_code) return "Chưa chọn kho nhập";
  if (!hasBom.value)
    return "Thành phẩm chưa có định mức vật tư — khai ở Sản phẩm → tab Định mức vật tư";
  if (shortages.value.length)
    return `Thiếu tồn kho: ${shortages.value
      .map((l) => `${l.name} (cần ${fmtDec(l.need)}, còn ${fmtDec(l.onHand)})`)
      .join(", ")}`;
  return "";
});

const canSave = computed(() => !blockReason.value);

async function save() {
  if (!canSave.value) {
    toast.add({ severity: "warn", summary: "Chưa tạo được lô", detail: blockReason.value });
    return;
  }
  saving.value = true;
  try {
    const res = JSON.parse(
      await stockStore.inbound({
        posting_date: toIsoDate(form.posting_date),
        voucher_no: form.voucher_no,
        description: form.description,
        supplier_code: "",
        warehouse_code: form.warehouse_code,
        unit_code: "HKD",
        note: "",
        items: [
          {
            product_code: form.product_code,
            quantity: form.quantity,
            unit_price: unitPrice.value,
            discount: 0,
          },
        ],
        inbound_type: "production",
        reference_no: "",
        vat_rate: 0,
        debit_account: "155",
        credit_account: "154",
        pay_now: false,
        adjust_dir: "up",
        // Bắt buộc: backend tự trừ tồn NVL theo định mức + tự sinh phiếu xuất.
        auto_bom: true,
      }),
    );
    toast.add({
      severity: "success",
      summary: `Đã tạo lô ${form.voucher_no}`,
      detail: `Giá trị nhập kho: ${fmtVnd(res.total)}${
        res.material_voucher ? ` · Đã tạo phiếu xuất NVL ${res.material_voucher}` : ""
      }`,
    });
    dialog.value = false;
    await page.reload();
  } catch (e) {
    toast.add({ severity: "error", summary: "Không tạo được lô", detail: String(e) });
  } finally {
    saving.value = false;
  }
}

async function reload() {
  await Promise.all([catalog.loadAll(), page.reload()]);
  bomItems.value = await api.getProductBoms();
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
          <i class="pi pi-factory text-xl text-emerald-600" />
          <h3 class="font-semibold">Lô sản xuất</h3>
        </div>
      </template>
      <template #end>
        <Button
          v-if="auth.canStock"
          label="Tạo lô sản xuất"
          icon="pi pi-plus"
          @click="openCreate"
        />
      </template>
    </Toolbar>

    <VoucherFilterBar
      v-model:search="f.search"
      v-model:from="f.from"
      v-model:to="f.to"
      @change="loadLots"
    />

    <Card>
      <template #content>
        <AppDataTable
          :value="lots"
          :loading="loading"
          :totalRecords="total"
          :first="page.first.value"
          :rows="page.rowsPerPage.value"
          :rows-per-page-options="page.pageSizes"
          data-testid="production-lots-table"
          sort-mode="single"
          removable-sort
          filter-toggle
          :global-filter-fields="['voucher_no', 'products', 'description']"
          stripedRows
          actions-header="Thao tác"
          actions-width="80"
          @page="page.onPage"
          @sort="page.onSort"
        >
          <Column field="voucher_no" header="Số phiếu lô" sortable>
            <template #body="{ data }">
              <Button
                :label="data.voucher_no"
                link
                class="!p-0 text-sm"
                :aria-label="`Xem lô ${data.voucher_no}`"
                v-tooltip="'Xem chi tiết dòng hàng của lô'"
                @click="openVoucher(data.voucher_no)"
              />
            </template>
          </Column>
          <Column field="posting_date" header="Ngày" sortable />
          <Column field="products" header="Thành phẩm" sortable />
          <Column field="item_count" header="Số loại" align="right">
            <template #body="{ data }">{{ data.item_count }}</template>
          </Column>
          <Column field="total_qty" header="Sản lượng" align="right">
            <template #body="{ data }">{{ fmtDec(data.total_qty) }}</template>
          </Column>
          <Column field="warehouse_code" header="Kho" />
          <Column field="description" header="Diễn giải" />
          <Column field="amount" header="Giá trị nhập" align="right" sortable>
            <template #body="{ data }"
              ><b>{{ fmtVnd(data.amount) }}</b></template
            >
          </Column>
          <template #actions="{ data }">
            <div class="flex justify-center gap-1">
              <Button
                icon="pi pi-eye"
                text
                rounded
                aria-label="Xem lô sản xuất"
                v-tooltip="'Xem chi tiết dòng hàng của lô'"
                @click="openVoucher(data.voucher_no)"
              />
            </div>
          </template>
          <template #empty>
            <EmptyState
              text="Chưa có lô sản xuất nào — bấm “Tạo lô sản xuất” để nhập thành phẩm theo định mức."
              icon="pi pi-factory"
            />
          </template>
        </AppDataTable>
      </template>
    </Card>

    <AppDialog
      v-model:visible="dialog"
      header="Tạo lô sản xuất"
      width="max-w-4xl"
      action-label="Tạo lô"
      :saving="saving"
      :show-action="auth.canStock"
      @action="save"
    >
      <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4 py-2">
        <FormField label="Thành phẩm">
          <div data-testid="lot-fg">
            <ProductSelect v-model="form.product_code" size="small" />
          </div>
        </FormField>
        <FormField label="Sản lượng">
          <InputNumber
            v-model="form.quantity"
            @input="onQtyInput"
            :min="0"
            :min-fraction-digits="0"
            :max-fraction-digits="3"
            size="small"
            class="w-full"
          />
        </FormField>
        <FormField label="Kho nhập">
          <Select
            v-model="form.warehouse_code"
            :options="warehouses"
            optionLabel="name"
            optionValue="code"
            size="small"
            class="w-full"
          />
        </FormField>
        <FormField label="Ngày nhập">
          <DatePicker
            v-model="form.posting_date"
            dateFormat="dd/mm/yy"
            size="small"
            class="w-full"
          />
        </FormField>
        <FormField label="Số phiếu lô" input-id="lot-voucher-no">
          <InputText id="lot-voucher-no" v-model="form.voucher_no" disabled size="small" />
        </FormField>
        <FormField label="Diễn giải">
          <InputText v-model="form.description" size="small" />
        </FormField>
      </div>

      <!-- NVL cần cho sản lượng: định mức × sản lượng, soi ngay với tồn kho. -->
      <div class="mt-2" data-testid="lot-preview">
        <div class="mb-1 flex flex-wrap items-baseline justify-between gap-2">
          <span class="text-sm font-semibold">Vật tư theo định mức</span>
          <span class="text-xs text-gray-500">
            Giá thành ước tính:
            <b class="text-emerald-700">{{ fmtVnd(totalCost) }}</b>
            · Đơn giá thành phẩm:
            <b class="text-emerald-700">{{ fmtVnd(unitPrice) }}</b>
          </span>
        </div>
        <DataTable
          :value="preview"
          size="small"
          striped-rows
          :rows="50"
          data-testid="lot-preview-table"
        >
          <Column field="name" header="Vật tư" style="min-width: 12rem">
            <template #body="{ data }">{{ data.name }} ({{ data.code }})</template>
          </Column>
          <Column field="perUnit" header="Định mức/SP" align="right">
            <template #body="{ data }">{{ fmtDec(data.perUnit) }} {{ data.unit }}</template>
          </Column>
          <Column field="need" header="SL cần" align="right">
            <template #body="{ data }">{{ fmtDec(data.need) }}</template>
          </Column>
          <Column field="onHand" header="Tồn kho" align="right">
            <template #body="{ data }">
              <span :class="data.shortage > 0 ? 'text-red-600' : 'text-emerald-700'">
                {{ fmtDec(data.onHand) }}
              </span>
            </template>
          </Column>
          <Column field="unitCost" header="Đơn giá vốn" align="right">
            <template #body="{ data }">{{ fmtVnd(data.unitCost) }}</template>
          </Column>
          <Column field="cost" header="Thành tiền" align="right">
            <template #body="{ data }"
              ><b>{{ fmtVnd(data.cost) }}</b></template
            >
          </Column>
          <template #empty>
            <EmptyState
              icon="pi pi-list-check"
              text="Chưa khai định mức cho thành phẩm này — khai ở Sản phẩm → tab Định mức vật tư."
            />
          </template>
        </DataTable>
        <Message v-if="shortages.length" severity="warn" :closable="false" class="mt-2">
          <span class="text-sm">
            Thiếu tồn kho: {{ shortages.map((l) => l.name).join(", ") }} — nhập thêm vật tư rồi tạo
            lô lại (lưu khi thiếu tồn sẽ bị backend từ chối).
          </span>
        </Message>
        <p v-else class="mt-2 text-xs text-gray-400">
          Lưu lô: nhập thành phẩm (Nợ 155 / Có 154) theo giá thành ước tính, đồng thời tự xuất vật
          tư theo định mức (Nợ 154 / Có 152) — thiếu tồn thì cả lô không được lưu.
        </p>
      </div>
    </AppDialog>

    <!-- Xem lô (chỉ đọc) — dùng chung popup phiếu nhập / xuất -->
    <VoucherViewDialog v-model:visible="viewVoucherVisible" :voucher-no="viewVoucherNo" />
  </div>
</template>
