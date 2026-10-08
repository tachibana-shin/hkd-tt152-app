<script setup lang="ts">
import { storeToRefs } from "pinia";
import { useAuthStore } from "@/stores/auth";
import { useCatalogStore } from "@/stores/catalog";
import { useSettingsStore } from "@/stores/settings";
import { useStockStore } from "@/stores/stock";
import PartnerDialog from "@/components/PartnerDialog.vue";
import InboundExcelDialog from "@/components/InboundExcelDialog.vue";
import { api } from "@/db";
import { fmtDec, fmtInt as fmt, fmtVnd, toIsoDate } from "@/utils/format";
import type { Account, InboundExcelLine, StockVoucherRow } from "@/types";
import { useKeepAliveRefresh } from "@/composables/useKeepAliveRefresh";
import { useLazyPage } from "@/composables/useLazyPage";

const catalog = useCatalogStore();
const stock = useStockStore();
const auth = useAuthStore();
const settings = useSettingsStore();
const { products, suppliers, warehouses } = storeToRefs(catalog);
const { loading } = storeToRefs(stock);
/**
 * Bảng gom theo phiếu: 1 phiếu = 1 dòng (số loại mặt hàng, tổng chiết khấu, thành
 * tiền). Trước đây bảng in thẳng `journal_entry` nên 1 phiếu N mặt hàng ra N dòng
 * trùng số phiếu. Chi tiết từng dòng vẫn xem được khi bấm số phiếu.
 */
const voucherRows = ref<StockVoucherRow[]>([]);
const voucherTotal = ref(0);
const voucherTotals = computed(() => voucherRows.value.reduce((s, r) => s + r.amount, 0));
const voucherDiscount = computed(() => voucherRows.value.reduce((s, r) => s + r.discount, 0));
const toast = useToast();

// Danh mục tài khoản (DMTK) — chọn TK Nợ / TK Có theo loại nhập.
const accounts = ref<Account[]>([]);
const accountOptions = computed(() =>
  accounts.value.map((a) => ({
    code: a.code,
    label: `${a.code} — ${a.name}`,
  })),
);

// Loại phiếu nhập kho (TT 88/2021/TT-BTC — mẫu 03-VT): Phiếu nhập kho áp dụng cho
// MỌI trường hợp hàng vào kho — mua ngoài (hóa đơn/bảng kê), tự sản xuất, gia công,
// nhập khác (thừa kiểm kê...) — không chỉ riêng mua hàng.
const typeOptions = [
  { label: "Mua hàng ngoài", value: "purchase" },
  { label: "Tự sản xuất / gia công", value: "production" },
  { label: "Nhập khác", value: "other" },
  { label: "Điều chỉnh hóa đơn mua", value: "adjust" },
];

// "Hướng điều chỉnh" chỉ xuất hiện với loại "adjust": bên bán trừ bớt (down —
// trả lại NCC, giảm tồn + giảm phải trả) hoặc bên bán thêm (up — nhập như thường).
const adjustDirOptions = [
  { label: "Tăng (bên bán thêm)", value: "up" },
  { label: "Giảm (trả lại NCC)", value: "down" },
];

// Gợi ý nội dung ô "Theo chứng từ" (cột "Theo..." của mẫu 03-VT) theo loại nhập.
const referencePlaceholder = computed(() =>
  form.inbound_type === "production"
    ? "Số lệnh nhập kho / lệnh sản xuất (VD: Lệnh SX 015)"
    : form.inbound_type === "other"
      ? "Số chứng từ nhập khác (VD: Biên bản kiểm kê 01)"
      : form.inbound_type === "adjust"
        ? "Số hóa đơn / chứng từ điều chỉnh (VD: HĐĐC 01 ngày 20/01/2026)"
        : "Số hóa đơn / bảng kê (VD: HĐ 170 ngày 16/01/2026)",
);

// Đổi TK mặc định theo loại nhập (vẫn sửa tay được):
//   purchase  → Nợ 152 hàng hóa / Có 331 phải trả người bán
//   production→ Nợ 155 thành phẩm / Có 154 chi phí SXKD dở dang
//   other     → Nợ 152 / Có 154 (người dùng tự chọn theo nghiệp vụ)
//   adjust    → Nợ 152 / Có 331 (với hướng "Giảm" backend sẽ đảo ngược:
//               Nợ TK đối ứng / Có 152 giảm hàng trả lại).
function onTypeChange() {
  // Ô "Tự xuất NVL theo định mức" chỉ dành cho loại sản xuất — đổi loại thì tắt.
  form.auto_bom = false;
  if (form.inbound_type === "production") {
    form.debit_account = "155";
    form.credit_account = "154";
  } else if (form.inbound_type === "other") {
    form.debit_account = "152";
    form.credit_account = "154";
  } else {
    form.debit_account = "152";
    form.credit_account = "331";
  }
}

// ─── Xem phiếu (chỉ đọc) — dùng chung VoucherViewDialog ───
const viewVoucherNo = ref("");
const viewVoucherVisible = ref(false);

function openVoucher(no: string) {
  viewVoucherNo.value = no;
  viewVoucherVisible.value = true;
}

// Lập phiếu điều chỉnh (trả lại NCC) nhanh từ 1 phiếu nhập gốc: tự điền nhà cung
// cấp + từng mặt hàng (SL, đơn giá) theo đúng chứng từ đang xem.
async function adjustFromVoucher(row: { voucher_no: string }) {
  try {
    const voucher = await api.getVoucher(row.voucher_no);
    const lines = voucher.filter((r) => r.entry_type === "PN" && r.product_code && r.quantity > 0);
    if (!lines.length) {
      toast.add({
        severity: "warn",
        summary: "Không điều chỉnh được",
        detail: `Chứng từ ${row.voucher_no} không có dòng hàng hóa để trả lại`,
      });
      return;
    }
    Object.assign(form, {
      posting_date: new Date(),
      description: `Trả lại NCC — theo ${row.voucher_no}`,
      supplier_code: voucher[0]?.supplier_code ?? "",
      note: "",
      items: lines.map((l) => ({
        product_code: l.product_code,
        quantity: l.quantity,
        unit_price: l.unit_price,
        discount: 0,
      })),
      inbound_type: "adjust",
      adjust_dir: "down",
      reference_no: row.voucher_no,
      vat_rate: 0,
      debit_account: "152",
      credit_account: "331",
      pay_now: false,
      auto_bom: false,
    });
    nextVoucherNo();
    dialog.value = true;
  } catch (e) {
    toast.add({ severity: "error", summary: "Lỗi", detail: String(e) });
  }
}

const dialog = ref(false);
const saving = ref(false);
/** Popup "Nhập hàng từ Excel" — mở từ nút trong popup tạo phiếu nhập. */
const excelVisible = ref(false);

const form = reactive({
  posting_date: new Date(),
  voucher_no: "PN001",
  description: "",
  supplier_code: "",
  warehouse_code: "",
  note: "",
  items: [] as {
    product_code: string;
    quantity: number;
    unit_price: number;
    discount: number; // số tiền CK (đ)
  }[],
  inbound_type: "purchase" as "purchase" | "production" | "other" | "adjust",
  reference_no: "",
  vat_rate: 0,
  debit_account: "152",
  credit_account: "331",
  // Hướng điều chỉnh hóa đơn mua (chỉ dùng khi loại = "adjust"):
  //   up   = bên bán thêm → nhập kho như phiếu thường
  //   down = bên bán trừ bớt → trả lại NCC, giảm tồn + giảm phải trả
  adjust_dir: "up" as "up" | "down",
  // Trả tiền ngay: bật mặc định — khi lưu tự tạo phiếu chi (PC) thanh toán cho NCC.
  pay_now: true,
  // F4 — Tự xuất NVL theo định mức: chỉ loại nhập "production". Bật → app tự
  // tính NVL = định mức × SL, tự trừ tồn FIFO + tự sinh phiếu xuất (Nợ 154 / Có 152).
  auto_bom: false,
});

/** Số phiếu do backend sinh (PN0001…) — không đoán từ danh sách đang lọc. */
async function nextVoucherNo() {
  form.voucher_no = await api.nextVoucherNo("PN");
}

function openCreate() {
  Object.assign(form, {
    posting_date: new Date(),
    description: "Phiếu nhập kho",
    supplier_code: "",
    warehouse_code: warehouses.value[0]?.code ?? "",
    note: "",
    items: [],
    inbound_type: "purchase",
    reference_no: "",
    vat_rate: 0,
    debit_account: "152",
    credit_account: "331",
    adjust_dir: "up",
    pay_now: true,
    auto_bom: false,
  });
  dialog.value = true;
  // Điền số phiếu sau khi mở hộp thoại: lệnh sinh số là bất đồng bộ.
  void nextVoucherNo();
}

function addRow() {
  form.items.push({ product_code: "", quantity: 1, unit_price: 0, discount: 0 });
}

function removeRow(i: number) {
  if (i < 0) return;
  form.items.splice(i, 1);
}

/**
 * Nhận dòng từ popup "Nhập hàng từ Excel": bỏ hết dòng trống còn sót (dòng tự
 * thêm cuối ở kiểu nhập liên tục) để hàng mới không chen vào giữa, rồi thêm hết
 * và nạp lại danh mục (trong đó vừa có mặt hàng mới tạo từ file).
 */
async function onExcelRows(lines: InboundExcelLine[]) {
  while (form.items.length && !form.items[form.items.length - 1].product_code) {
    form.items.pop();
  }
  form.items.push(...lines);
  toast.add({
    severity: "success",
    summary: "Đã thêm hàng từ Excel",
    detail: `${lines.length} dòng · ${form.items.length} dòng trong phiếu`,
  });
  try {
    await catalog.loadAll();
  } catch (e) {
    toast.add({
      severity: "warn",
      summary: "Không nạp lại được danh mục",
      detail: String(e),
    });
  }
}

// ─── Thêm nhà cung cấp nhanh: dùng chung PartnerDialog (tự sinh mã + tra cứu MST) ───
const supplierDialog = ref(false);

function onSupplierSaved(code: string) {
  form.supplier_code = code;
}

async function save() {
  // Kiểu nhập liên tục (pre-input) luôn để lại dòng trống cuối → bỏ dòng chưa chọn hàng.
  const rows = form.items.filter((it) => it.product_code);
  if (!form.posting_date || !rows.length) {
    toast.add({
      severity: "warn",
      summary: "Thiếu thông tin",
      detail: "Cần ngày và ít nhất 1 mặt hàng",
    });
    return;
  }
  saving.value = true;
  try {
    const res = JSON.parse(
      await stock.inbound({
        posting_date: toIsoDate(form.posting_date),
        voucher_no: form.voucher_no,
        description: form.description,
        supplier_code: form.supplier_code,
        warehouse_code: form.warehouse_code,
        unit_code: "HKD",
        note: form.note,
        items: rows.map((it) => ({ ...it })),
        inbound_type: form.inbound_type,
        reference_no: form.reference_no,
        // % → tỷ lệ (0-1); backend tự bỏ qua thuế nếu hộ chưa bật khấu trừ GTGT.
        vat_rate: form.vat_rate / 100,
        debit_account: form.debit_account,
        credit_account: form.credit_account,
        // Trả tiền ngay: backend tự tạo + liên kết phiếu chi (PC) cho nhà cung cấp.
        pay_now: form.pay_now,
        adjust_dir: form.adjust_dir,
        // F4: tự xuất NVL theo định mức (chỉ loại sản xuất backend mới chạy).
        auto_bom: form.auto_bom,
      }),
    );
    toast.add({
      severity: "success",
      summary: `Đã nhập kho ${form.voucher_no}`,
      detail: `Giá trị nhập kho: ${fmt(res.total)} đ${
        res.vat ? ` · VAT khấu trừ: ${fmt(res.vat)} đ` : ""
      }${res.pc_no ? ` · Đã tạo phiếu chi ${res.pc_no}` : ""}${
        res.material_voucher
          ? ` · Đã tạo phiếu xuất NVL ${res.material_voucher} (Nợ 154 / Có 152)`
          : ""
      }`,
    });
    dialog.value = false;
    // Danh sách gom theo phiếu không do store nạp → nạp lại sau khi lưu.
    await loadVouchers();
  } catch (e) {
    toast.add({ severity: "error", summary: "Lỗi", detail: String(e) });
  } finally {
    saving.value = false;
  }
}

// ─── Bộ lọc danh sách phiếu (giữ khi chuyển menu nhờ KeepAlive) ───
const f = reactive({ search: "", from: null as Date | null, to: null as Date | null });

// Phân trang server-side: danh sách phiếu dài theo thời gian, mỗi phiếu 1 dòng.
const page = useLazyPage(async (lazy) => {
  loading.value = true;
  try {
    const res = await api.getStockVouchersPage(lazy, "PN", toIsoDate(f.from), toIsoDate(f.to));
    voucherRows.value = res.rows;
    voucherTotal.value = res.total;
  } catch (e) {
    toast.add({ severity: "error", summary: "Lỗi tải danh sách phiếu", detail: String(e) });
  } finally {
    loading.value = false;
  }
});

/** Đổi khoảng ngày / từ khoá tìm kiếm → nạp lại trang đầu. */
function loadVouchers() {
  return page.setKeyword(f.search.trim());
}

async function reload() {
  await Promise.all([catalog.loadAll(), loadVouchers(), settings.load()]);
  accounts.value = await api.getAccounts();
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
          <i-mdi-download class="text-xl text-blue-500" />
          <h3 class="font-semibold">Phiếu nhập kho</h3>
        </div>
      </template>
      <template #end>
        <Button v-if="auth.canStock" label="Tạo phiếu nhập" icon="pi pi-plus" @click="openCreate" />
      </template>
    </Toolbar>

    <!-- Tìm phiếu: danh sách sắp theo ngày nên phiếu ngày cũ nằm giữa danh sách. -->
    <VoucherFilterBar
      v-model:search="f.search"
      v-model:from="f.from"
      v-model:to="f.to"
      @change="loadVouchers"
    />

    <Card>
      <template #content>
        <AppDataTable
          :value="voucherRows"
          :loading="loading"
          :totalRecords="voucherTotal"
          :first="page.first.value"
          :rows="page.rowsPerPage.value"
          :rows-per-page-options="page.pageSizes"
          data-testid="voucher-table-inbound"
          sort-mode="single"
          removable-sort
          filter-toggle
          :global-filter-fields="['voucher_no', 'description', 'supplier_name']"
          stripedRows
          actions-header="Thao tác"
          actions-width="132"
          @page="page.onPage"
          @sort="page.onSort"
        >
          <Column field="voucher_no" header="Số phiếu" sortable>
            <template #body="{ data }">
              <Button
                :label="data.voucher_no"
                link
                class="!p-0 text-sm"
                :aria-label="`Xem phiếu ${data.voucher_no}`"
                v-tooltip="'Xem chi tiết dòng hàng của phiếu'"
                @click="openVoucher(data.voucher_no)"
              />
            </template>
          </Column>
          <Column field="posting_date" header="Ngày" sortable />
          <Column field="description" header="Diễn giải" />
          <Column field="supplier_name" header="Nhà cung cấp" sortable>
            <template #body="{ data }">{{ data.supplier_name || "—" }}</template>
          </Column>
          <Column field="item_count" header="Số loại MH" align="right" sortable>
            <template #body="{ data }">{{ data.item_count }}</template>
          </Column>
          <Column field="total_qty" header="Tổng SL" align="right">
            <template #body="{ data }">{{ fmtDec(data.total_qty) }}</template>
          </Column>
          <Column field="discount" header="Chiết khấu" align="right" sortable>
            <template #body="{ data }">
              <span :class="data.discount > 0 ? 'text-emerald-600' : 'text-gray-400'">
                {{ data.discount > 0 ? fmtVnd(data.discount) : "—" }}
              </span>
            </template>
            <template #footer>
              {{ voucherRows.length ? fmtVnd(voucherDiscount) : "" }}
            </template>
          </Column>
          <Column field="amount" header="Thành tiền" align="right" sortable>
            <template #body="{ data }"
              ><b>{{ fmtVnd(data.amount) }}</b></template
            >
            <template #footer>
              {{ voucherRows.length ? fmtVnd(voucherTotals) : "" }}
            </template>
          </Column>
          <template #actions="{ data }">
            <div class="flex justify-center gap-1">
              <Button
                icon="pi pi-eye"
                text
                rounded
                aria-label="Xem phiếu"
                v-tooltip="'Xem chi tiết dòng hàng của phiếu'"
                @click="openVoucher(data.voucher_no)"
              />
              <Button
                icon="pi pi-replay"
                text
                rounded
                severity="warn"
                :aria-label="`Lập phiếu điều chỉnh theo ${data.voucher_no}`"
                v-tooltip="'Lập phiếu điều chỉnh (trả lại NCC) theo phiếu này'"
                @click="adjustFromVoucher(data)"
              />
            </div>
          </template>
          <template #empty>
            <EmptyState text="Chưa có phiếu nào trong khoảng ngày đang lọc." icon="pi pi-inbox" />
          </template>
        </AppDataTable>
      </template>
    </Card>

    <AppDialog
      v-model:visible="dialog"
      :header="
        form.inbound_type === 'adjust' ? 'Tạo phiếu điều chỉnh hóa đơn mua' : 'Tạo phiếu nhập kho'
      "
      width="max-w-3xl"
      action-label="Lưu phiếu"
      :saving="saving"
      :show-action="auth.canStock"
      @action="save"
    >
      <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4 py-2">
        <FormField label="Loại nhập">
          <Select
            v-model="form.inbound_type"
            :options="typeOptions"
            optionLabel="label"
            optionValue="value"
            size="small"
            class="w-full"
            @change="onTypeChange"
          />
        </FormField>
        <FormField v-if="form.inbound_type === 'adjust'" label="Hướng điều chỉnh">
          <Select
            v-model="form.adjust_dir"
            :options="adjustDirOptions"
            optionLabel="label"
            optionValue="value"
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
        <FormField label="Số phiếu" input-id="inbound-voucher-no">
          <InputText :id="'inbound-voucher-no'" v-model="form.voucher_no" disabled size="small" />
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
        <FormField label="Theo chứng từ" class="col-span-2">
          <InputText v-model="form.reference_no" :placeholder="referencePlaceholder" size="small" />
        </FormField>
        <FormField v-if="form.inbound_type === 'purchase'" label="Nhà cung cấp" class="col-span-2">
          <div class="flex gap-2">
            <Select
              v-model="form.supplier_code"
              :options="suppliers"
              optionLabel="name"
              optionValue="code"
              :editable="true"
              filter
              size="small"
              class="w-full"
              placeholder="Chọn hoặc nhập tên NCC"
            />
            <Button
              v-if="auth.canStock"
              icon="pi pi-plus"
              text
              rounded
              severity="secondary"
              aria-label="Thêm nhà cung cấp nhanh"
              v-tooltip="'Thêm nhà cung cấp nhanh'"
              @click="supplierDialog = true"
            />
          </div>
        </FormField>
        <FormField
          v-if="form.inbound_type === 'purchase' && settings.deductVat"
          label="Thuế GTGT đầu vào (%)"
        >
          <InputNumber
            v-model="form.vat_rate"
            :min="0"
            :max="30"
            suffix=" %"
            size="small"
            class="w-full"
          />
        </FormField>
        <FormField label="TK Nợ (kho)">
          <Select
            v-model="form.debit_account"
            :options="accountOptions"
            option-label="label"
            option-value="code"
            filter
            size="small"
            class="w-full"
          />
        </FormField>
        <FormField label="TK Có (đối ứng)">
          <Select
            v-model="form.credit_account"
            :options="accountOptions"
            option-label="label"
            option-value="code"
            filter
            size="small"
            class="w-full"
          />
        </FormField>
        <FormField label="Diễn giải">
          <InputText v-model="form.description" size="small" />
        </FormField>
        <FormField v-if="form.inbound_type === 'production'" label="Tự xuất NVL theo định mức">
          <div class="flex h-full items-center gap-1.5">
            <ToggleSwitch v-model="form.auto_bom" class="shrink-0" />
            <i
              class="pi pi-info-circle cursor-help text-xs text-gray-400 shrink-0"
              v-tooltip="
                'Bật: khi lưu, app tự tính NVL = định mức × sản lượng từng mặt hàng, tự trừ tồn FIFO và tự sinh phiếu xuất NVL (Nợ 154 / Có 152) trong cùng lúc lưu phiếu nhập. Thiếu tồn NVL thì phiếu nhập KHÔNG được lưu. Khai định mức ở màn Sản phẩm → tab Định mức vật tư.'
              "
              aria-label="Trợ giúp"
            />
          </div>
        </FormField>
        <FormField v-if="form.inbound_type === 'purchase'" label="Trả tiền ngay">
          <div class="flex h-full items-center gap-1.5">
            <ToggleSwitch v-model="form.pay_now" class="shrink-0" />
            <i
              class="pi pi-info-circle cursor-help text-xs text-gray-400 shrink-0"
              v-tooltip="
                'Bật: khi lưu sẽ tự tạo phiếu chi (PC) thanh toán cho nhà cung cấp — cần chọn Nhà cung cấp.'
              "
              aria-hidden="true"
            />
          </div>
        </FormField>
      </div>
      <p class="mt-1 text-xs text-gray-400">
        Mua hàng ngoài: Nợ 152 / Có 331 — nhập số tiền chiết khấu ở cột Tiền CK, giá trị nhập kho =
        Thành tiền − Tiền CK. Tự sản xuất, gia công: nhập kho thành phẩm theo lệnh sản xuất (Nợ 155
        / Có 154) — bật "Tự xuất NVL theo định mức" để app tự trừ NVL và tự sinh phiếu xuất NVL (Nợ
        154 / Có 152) theo định mức đã khai ở màn Sản phẩm. Nhập khác: tùy chọn TK Nợ/Có (thừa kiểm
        kê, điều chỉnh…). Đơn giá nhập = giá sau chiết khấu, chưa thuế khi bật khấu trừ GTGT, ngược
        lại là giá đã gồm thuế. Điều chỉnh hóa đơn mua — Giảm: trả lại NCC, trừ lô FIFO theo ngày
        nhập, ghi Nợ TK đối ứng / Có TK hàng (kèm giảm thuế 133 nếu khấu trừ); Tăng: nhập kho như
        phiếu thường.
      </p>

      <!-- Nhập kho đang TĂNG tồn → tắt cảnh báo "Thiếu" của bảng dòng; riêng
           điều chỉnh hướng "Giảm (trả lại NCC)" mới bật lại. -->
      <LineItemsEditor
        :items="form.items"
        :products="products"
        :can-edit="auth.canStock"
        price-field="cost_price"
        show-amount
        compact
        show-add-product
        show-discount
        :warn-shortage="form.inbound_type === 'adjust' && form.adjust_dir === 'down'"
        :total-label="form.inbound_type === 'purchase' ? 'Giá trị nhập kho:' : 'Tổng tiền:'"
        @add="addRow"
        @remove="removeRow"
      >
        <!-- Nhập hàng từ Excel: tự dò cột (sửa tay được — file TT88 không có tiêu đề), thiếu hàng tự tạo mới. -->
        <template #actions>
          <Button
            v-if="auth.canStock"
            label="Nhập Excel"
            icon="pi pi-file-excel"
            size="small"
            text
            data-testid="inbound-excel-open"
            @click="excelVisible = true"
          />
        </template>
      </LineItemsEditor>
    </AppDialog>

    <!-- Chọn cột → xem trước file (phân trang vì file có thể vài nghìn dòng) rồi thêm hàng loạt -->
    <InboundExcelDialog
      v-model:visible="excelVisible"
      :products="products"
      @confirm="onExcelRows"
    />

    <!-- Xem phiếu (chỉ đọc) — dùng chung cho cả PN/PX -->
    <VoucherViewDialog v-model:visible="viewVoucherVisible" :voucher-no="viewVoucherNo" />

    <!-- Thêm nhà cung cấp nhanh — dùng chung dialog chuẩn (tự sinh mã + tra cứu MST) -->
    <PartnerDialog
      v-model:visible="supplierDialog"
      kind="supplier"
      :show-action="auth.canStock"
      @saved="onSupplierSaved"
    />
  </div>
</template>
