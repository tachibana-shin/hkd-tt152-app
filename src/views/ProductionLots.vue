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
 * — màn Nhập kho vẫn hiện đủ mọi phiếu nhập như trước. Form tạo lô nằm ở
 * `ProductionLotDialog` (dùng chung với popup hóa đơn).
 */
import { storeToRefs } from "pinia";
import { useAuthStore } from "@/stores/auth";
import { useCatalogStore } from "@/stores/catalog";
import { useStockStore } from "@/stores/stock";
import { api } from "@/db";
import { fmtDec, fmtVnd, toIsoDate } from "@/utils/format";
import type { ProductionLotRow } from "@/types";
import { useKeepAliveRefresh } from "@/composables/useKeepAliveRefresh";
import { useLazyPage } from "@/composables/useLazyPage";

const auth = useAuthStore();
const catalog = useCatalogStore();
const stockStore = useStockStore();
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
// Toàn bộ form + bản xem trước nằm trong ProductionLotDialog dùng chung với
// popup hóa đơn — màn này chỉ mở ra và nạp lại danh sách khi tạo xong.
const dialog = ref(false);

function openCreate() {
  dialog.value = true;
}

/** Tạo xong lô (nhận số phiếu) → nạp lại danh sách + danh mục. */
async function onLotCreated() {
  await reload();
}

async function reload() {
  await Promise.all([catalog.loadAll(), page.reload()]);
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

    <ProductionLotDialog v-model:visible="dialog" @done="onLotCreated" />

    <!-- Xem lô (chỉ đọc) — dùng chung popup phiếu nhập / xuất -->
    <VoucherViewDialog v-model:visible="viewVoucherVisible" :voucher-no="viewVoucherNo" />
  </div>
</template>
