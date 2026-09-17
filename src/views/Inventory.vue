<script setup lang="ts">
import { storeToRefs } from "pinia";
import { useAuthStore } from "@/stores/auth";
import { useCatalogStore } from "@/stores/catalog";
import { useStockStore } from "@/stores/stock";
import { useInvoiceStore } from "@/stores/invoice";
import { fmtDec as fmt } from "@/utils/format";

const catalog = useCatalogStore();
const stock = useStockStore();
const invoice = useInvoiceStore();
const auth = useAuthStore();
const { products, warehouses } = storeToRefs(catalog);
const { summary, lots, loading } = storeToRefs(stock);
const { counts, countDetail, loading: cntLoading } = storeToRefs(invoice);
const toast = useToast();

const tab = ref("summary");
const productFilter = ref("");
const counting = ref(false);
const saving = ref(false);
const detailDialog = ref(false);
const selectedCount = ref<number | null>(null);

const countForm = reactive({
  date: new Date(),
  note: "",
  items: [] as { product_code: string; warehouse_code: string; counted_qty: number }[],
});

const iso = (d: Date | null) => (d ? d.toISOString().slice(0, 10) : "");

const totalValue = computed(() =>
  summary.value.reduce((s, r) => s + r.balance, 0),
);
const lowStock = computed(() =>
  summary.value.filter(
    (r) =>
      r.balance <=
      (catalog.productByCode(r.product_code)?.min_stock ?? 0),
  ),
);

function openCounting() {
  Object.assign(countForm, {
    date: new Date(),
    note: "",
    items: summary.value.map((r) => ({
      product_code: r.product_code,
      warehouse_code: warehouses.value[0]?.code ?? "",
      counted_qty: r.balance,
    })),
  });
  counting.value = true;
}

function addRow() {
  countForm.items.push({
    product_code: "",
    warehouse_code: warehouses.value[0]?.code ?? "",
    counted_qty: 0,
  });
}

function removeRow(i: number) {
  countForm.items.splice(i, 1);
}

async function saveCount() {
  if (!countForm.items.length) {
    toast.add({ severity: "warn", summary: "Chưa có dòng nào", detail: "Thêm ít nhất 1 mặt hàng" });
    return;
  }
  saving.value = true;
  try {
    await invoice.saveCount({
      date: iso(countForm.date),
      note: countForm.note,
      items: countForm.items.map((it) => ({ ...it })),
    });
    toast.add({ severity: "success", summary: "Đã lưu phiếu kiểm kê" });
    counting.value = false;
    await stock.loadSummary();
  } catch (e) {
    toast.add({ severity: "error", summary: "Lỗi", detail: String(e) });
  } finally {
    saving.value = false;
  }
}

async function showDetail(id: number) {
  selectedCount.value = id;
  detailDialog.value = true;
  await invoice.loadCountDetail(id);
}

onMounted(async () => {
  await Promise.all([
    catalog.loadAll(),
    stock.loadSummary(),
    stock.loadLots(""),
    invoice.loadCounts(),
  ]);
});
</script>

<template>
  <div class="space-y-4">
    <Toolbar>
      <template #start>
        <div class="flex items-center gap-2">
          <i-mdi-warehouse class="text-xl text-emerald-500" />
          <h3 class="font-semibold">Tồn kho &amp; kiểm kê</h3>
        </div>
      </template>
      <template #end>
        <Button v-if="auth.canStock" label="Kiểm kê" icon="pi pi-clipboard" severity="warning" @click="openCounting" />
      </template>
    </Toolbar>

    <!-- Thẻ tóm tắt -->
    <div class="grid grid-cols-3 gap-4">
      <Card>
        <template #content>
          <div class="text-sm text-gray-500">Tổng tồn (số lượng)</div>
          <div class="text-2xl font-bold mt-1">{{ fmt(totalValue) }}</div>
        </template>
      </Card>
      <Card>
        <template #content>
          <div class="text-sm text-gray-500">Mặt hàng đang tồn</div>
          <div class="text-2xl font-bold mt-1">{{ summary.filter((r) => r.balance > 0).length }}</div>
        </template>
      </Card>
      <Card>
        <template #content>
          <div class="text-sm text-gray-500">Dưới tồn tối thiểu</div>
          <div class="text-2xl font-bold mt-1 text-amber-600">{{ lowStock.length }}</div>
        </template>
      </Card>
    </div>

    <Card>
      <template #content>
        <Tabs v-model:value="tab">
          <TabList>
            <Tab value="summary"><i class="pi pi-table mr-2" />Tổng hợp N-X-T</Tab>
            <Tab value="lots"><i class="pi pi-list mr-2" />Chi tiết lô (FIFO)</Tab>
            <Tab value="counts"><i class="pi pi-clipboard mr-2" />Phiếu kiểm kê</Tab>
          </TabList>
          <TabPanels>
            <!-- Tổng hợp -->
            <TabPanel value="summary">
              <DataTable :value="summary" :loading="loading" stripedRows paginator :rows="15">
                <Column field="product_code" header="Mã SP" style="width: 110px" />
                <Column field="product_name" header="Tên sản phẩm" />
                <Column field="inbound" header="Nhập" style="width: 110px" align="right">
                  <template #body="{ data }">{{ fmt(data.inbound) }}</template>
                </Column>
                <Column field="outbound" header="Xuất" style="width: 110px" align="right">
                  <template #body="{ data }">{{ fmt(data.outbound) }}</template>
                </Column>
                <Column field="balance" header="Tồn cuối" style="width: 120px" align="right">
                  <template #body="{ data }">
                    <span :class="data.balance < 0 ? 'text-red-600 font-bold' : 'font-semibold'">
                      {{ fmt(data.balance) }}
                    </span>
                  </template>
                </Column>
                <template #empty><EmptyState text="Chưa có dữ liệu nhập/xuất kho." icon="pi pi-box" /></template>
              </DataTable>
            </TabPanel>

            <!-- Chi tiết lô -->
            <TabPanel value="lots">
              <div class="flex items-center gap-3 mb-3">
                <IconField>
                  <InputIcon class="pi pi-search" />
                  <InputText v-model="productFilter" placeholder="Lọc theo mã sản phẩm..." class="w-64" />
                </IconField>
                <Button
                  icon="pi pi-refresh"
                  text
                  rounded
                  size="small"
                  @click="stock.loadLots(productFilter)"
                />
              </div>
              <DataTable :value="lots" :loading="loading" stripedRows>
                <Column field="product_code" header="Mã SP" style="width: 110px" />
                <Column field="product_name" header="Tên sản phẩm" />
                <Column field="warehouse_code" header="Kho" style="width: 90px" />
                <Column field="received_at" header="Ngày nhập" style="width: 130px" />
                <Column field="quantity" header="Còn lại" style="width: 110px" align="right">
                  <template #body="{ data }">{{ fmt(data.quantity) }}</template>
                </Column>
                <Column field="unit_cost" header="Đơn giá nhập" style="width: 140px" align="right">
                  <template #body="{ data }">{{ fmt(data.unit_cost) }}</template>
                </Column>
                <template #empty><EmptyState text="Không có lô hàng nào." icon="pi pi-box" /></template>
              </DataTable>
            </TabPanel>

            <!-- Phiếu kiểm kê -->
            <TabPanel value="counts">
              <DataTable :value="counts" :loading="cntLoading" stripedRows>
                <Column field="date" header="Ngày" style="width: 130px" />
                <Column field="note" header="Ghi chú" />
                <Column field="id" header="Số dòng" style="width: 100px" align="right">
                  <template #body="{ data }">
                    <Button
                      label="Xem chi tiết"
                      size="small"
                      text
                      icon="pi pi-eye"
                      @click="showDetail(data.id)"
                    />
                  </template>
                </Column>
                <template #empty><EmptyState text="Chưa có phiếu kiểm kê nào." icon="pi pi-clipboard" /></template>
              </DataTable>
            </TabPanel>
          </TabPanels>
        </Tabs>
      </template>
    </Card>

    <!-- Dialog kiểm kê -->
    <AppDialog
      v-model:visible="counting"
      header="Phiếu kiểm kê hàng hóa"
      width="max-w-3xl"
      action-label="Lưu phiếu kiểm kê"
      :saving="saving"
      :show-action="auth.canStock"
      @action="saveCount"
    >
      <div class="grid grid-cols-2 gap-4 py-2">
        <FormField label="Ngày kiểm kê">
          <DatePicker v-model="countForm.date" dateFormat="dd/mm/yy" class="w-full" />
        </FormField>
        <FormField label="Ghi chú">
          <InputText v-model="countForm.note" placeholder="Ví dụ: Cuối quý 3/2026" />
        </FormField>
      </div>

      <div class="mt-4 flex items-center justify-between">
        <h4 class="text-sm font-semibold flex items-center gap-1.5">
          <i class="pi pi-list text-gray-400"></i> Mặt hàng (số kiểm đếm)
        </h4>
        <Button v-if="auth.canStock" label="Thêm dòng" icon="pi pi-plus" size="small" text @click="addRow" />
      </div>

      <DataTable :value="countForm.items" class="mt-2">
        <Column header="Sản phẩm" style="min-width: 220px">
          <template #body="{ index }">
            <Select
              v-model="countForm.items[index].product_code"
              :options="products"
              optionLabel="name"
              optionValue="code"
              filter
              showClear
              class="w-full"
            />
          </template>
        </Column>
        <Column header="Kho" style="width: 140px">
          <template #body="{ index }">
            <Select
              v-model="countForm.items[index].warehouse_code"
              :options="warehouses"
              optionLabel="name"
              optionValue="code"
              class="w-full"
            />
          </template>
        </Column>
        <Column header="Tồn sổ cái" style="width: 110px" align="right">
          <template #body="{ data }">
            {{ fmt(summary.find((r) => r.product_code === data.product_code)?.balance ?? 0) }}
          </template>
        </Column>
        <Column header="Đếm thực tế" style="width: 110px">
          <template #body="{ index }">
            <InputNumber v-model="countForm.items[index].counted_qty" :min="0" class="w-full" />
          </template>
        </Column>
        <Column header="" style="width: 60px">
          <template #body="{ index }">
            <Button v-if="auth.canStock" icon="pi pi-times" text rounded size="small" severity="danger" @click="removeRow(index)" />
          </template>
        </Column>
        <template #empty>
          <EmptyState text="Chưa có dòng nào" icon="pi pi-list" />
        </template>
      </DataTable>
    </AppDialog>

    <!-- Dialog chi tiết phiếu kiểm kê -->
    <AppDialog
      v-model:visible="detailDialog"
      header="Chi tiết phiếu kiểm kê"
      width="max-w-3xl"
      cancel-label="Đóng"
      :show-action="false"
    >
      <DataTable :value="countDetail" stripedRows>
        <Column field="product_code" header="Mã SP" style="width: 110px" />
        <Column field="product_name" header="Tên sản phẩm" />
        <Column field="book_qty" header="Tồn sổ cái" style="width: 110px" align="right">
          <template #body="{ data }">{{ fmt(data.book_qty) }}</template>
        </Column>
        <Column field="counted_qty" header="Đếm thực tế" style="width: 110px" align="right">
          <template #body="{ data }">{{ fmt(data.counted_qty) }}</template>
        </Column>
        <Column field="variance" header="Chênh lệch" style="width: 110px" align="right">
          <template #body="{ data }">
            <span :class="data.variance !== 0 ? (data.variance > 0 ? 'text-emerald-600 font-semibold' : 'text-red-600 font-semibold') : ''">
              {{ fmt(data.variance) }}
            </span>
          </template>
        </Column>
        <template #empty><EmptyState text="Không có dữ liệu." icon="pi pi-inbox" /></template>
      </DataTable>
    </AppDialog>
  </div>
</template>