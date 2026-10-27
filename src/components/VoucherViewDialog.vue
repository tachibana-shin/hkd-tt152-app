<script setup lang="ts">
/**
 * Xem 1 phiếu nhập kho / phiếu xuất kho (chỉ đọc).
 *
 * Dùng chung cho: danh sách Nhập kho, danh sách Xuất kho, màn Đồng bộ HĐĐT
 * (bấm vào số `PN…` trong cột "Phiếu nhập") và màn Lô sản xuất. Dữ liệu lấy từ
 * `get_voucher` — cùng nguồn với màn in nên số tiền/đơn giá luôn khớp chứng từ.
 *
 * `with-components`: lô sản xuất hiện kèm bảng thành phần đã chụp (vật tư, SL,
 * đơn giá khai, thành tiền, tiền vốn FIFO) ngay trong phiếu — không cần mở
 * thêm hộp thoại khác.
 */
import { api } from "@/db";
import type { ProductionLotComponentRow, VoucherRow } from "@/types";
import { fmtDec, fmtInt as fmt, fmtVnd as money } from "@/utils/format";

const visible = defineModel<boolean>("visible", { default: false });
const props = defineProps<{ voucherNo: string; withComponents?: boolean }>();

const toast = useToast();
const router = useRouter();
const rows = ref<VoucherRow[]>([]);
const loading = ref(false);
const errorMsg = ref("");

// Hóa đơn điện tử đã gắn với phiếu (nếu có) — bấm vào để xem/in PDF.
const invoiceDetail = ref("");
const invoicePdfVisible = ref(false);

const first = computed(() => rows.value[0]);
const isOutbound = computed(() => first.value?.entry_type === "PX");
const title = computed(() => (isOutbound.value ? "PHIẾU XUẤT KHO" : "PHIẾU NHẬP KHO"));
const formNo = computed(() => (isOutbound.value ? "02-VT" : "01-VT"));
const partnerName = computed(() => first.value?.supplier_name || first.value?.customer_name || "");
const partnerCode = computed(() => first.value?.supplier_code || first.value?.customer_code || "");

/** Dòng hàng hóa thật (bỏ dòng bút toán tách thuế GTGT: không mã hàng, SL = 0). */
const lines = computed(() => rows.value.filter((r) => r.product_code !== ""));
/** Dòng bút toán không gắn hàng (chiết khấu/thuế tách riêng) — hiện dạng ghi chú. */
const extraLines = computed(() => rows.value.filter((r) => r.product_code === ""));

// `amount` trên sổ là thành tiền SAU chiết khấu; tiền CK lưu riêng cột `discount`.
const disc = (r: VoucherRow) => r.discount || 0;
// Dòng cũ chưa ghi `amount` (nhưng cũng không có CK) mới cần tính SL × ĐG;
// amount = 0 mà có CK nghĩa là CK 100% → thành tiền sau CK đúng là 0.
const lineNet = (r: VoucherRow) =>
  r.amount !== 0 || disc(r) !== 0 ? r.amount : r.quantity * r.unit_price;
/** Thành tiền TRƯỚC chiết khấu (giá gốc) = thành tiền sau CK + tiền CK. */
const lineGross = (r: VoucherRow) => lineNet(r) + disc(r);
/** Đơn giá GỐC / đơn vị — nhập kho lưu `unit_price` đã trừ CK nên phải tính lại. */
const lineUnitGross = (r: VoucherRow) => (r.quantity ? lineGross(r) / r.quantity : r.unit_price);

const grossTotal = computed(() => lines.value.reduce((s, r) => s + lineGross(r), 0));
const discountTotal = computed(() => lines.value.reduce((s, r) => s + disc(r), 0));
const netTotal = computed(() => grossTotal.value - discountTotal.value);
/** Phiếu nào có dòng hàng mang % GTGT riêng (hóa đơn mua đa thuế suất) mới hiện cột. */
const hasVatRows = computed(() => lines.value.some((r) => r.vat_rate > 0));

// ─── Lô sản xuất: thành phần đã chụp lúc tạo (SL + đơn giá khai + vốn FIFO) ───
const components = ref<ProductionLotComponentRow[]>([]);
const componentsLoading = ref(false);
const componentsError = ref("");
const componentTotal = computed(() => components.value.reduce((s, r) => s + r.amount, 0));
const componentCost = computed(() => components.value.reduce((s, r) => s + r.cost_amount, 0));
const componentHasMultipleProducts = computed(
  () => new Set(components.value.map((r) => r.product_code)).size > 1,
);
const componentHasService = computed(() => components.value.some((r) => r.is_service));

/** "YYYY-MM-DD" → "dd/MM/yyyy" (ngày lập phiếu trong DB là ngày, không cần giờ). */
const postingDate = computed(() => {
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(first.value?.posting_date ?? "");
  return m ? `${m[3]}/${m[2]}/${m[1]}` : (first.value?.posting_date ?? "");
});

/** In phiếu: dùng mẫu 01-VT / 02-VT của màn in sẵn có. */
function printVoucher() {
  if (!props.voucherNo) return;
  visible.value = false;
  router.push("/print/" + encodeURIComponent(props.voucherNo));
}

// Nạp dữ liệu ngay khi mở dialog (không dùng onMounted: mỗi lần mở là 1 chứng từ khác).
watch(
  () => [visible.value, props.voucherNo] as const,
  async ([isOpen, no]) => {
    if (!isOpen || !no) return;
    loading.value = true;
    errorMsg.value = "";
    rows.value = [];
    components.value = [];
    componentsError.value = "";
    invoiceDetail.value = "";
    try {
      rows.value = await api.getVoucher(no);
    } catch (e) {
      errorMsg.value = String(e);
      toast.add({ severity: "error", summary: "Không xem được phiếu", detail: String(e) });
    } finally {
      loading.value = false;
    }
    if (props.withComponents) {
      componentsLoading.value = true;
      try {
        components.value = await api.getProductionLotComponents(no);
      } catch (e) {
        componentsError.value = String(e);
      } finally {
        componentsLoading.value = false;
      }
    }
    // Hỏi thêm hóa đơn đã gắn (phiếu nhập tay / phiếu xuất không có → lỗi và
    // ẩn nút, không báo toast vì đây chỉ là tính năng bổ sung).
    try {
      invoiceDetail.value = await api.hddtVoucherInvoiceDetail(no);
    } catch {
      invoiceDetail.value = "";
    }
  },
  { immediate: true },
);
</script>

<template>
  <AppDialog
    v-model:visible="visible"
    :header="`${title} · ${formNo}`"
    :width="withComponents ? 'max-w-6xl' : 'max-w-4xl'"
    cancel-label="Đóng"
    action-label="In phiếu"
    action-icon="pi pi-print"
    :action-disabled="!first"
    @action="printVoucher"
  >
    <div v-if="loading" class="flex justify-center py-8">
      <ProgressSpinner />
    </div>

    <p v-else-if="errorMsg" class="py-4 text-sm text-red-600">{{ errorMsg }}</p>

    <div v-else-if="first" class="space-y-3">
      <div class="flex flex-wrap items-baseline justify-between gap-2">
        <span class="text-sm text-gray-500">Số phiếu</span>
        <span class="text-base font-semibold">{{ first.voucher_no }}</span>
      </div>

      <!-- Phiếu tạo từ hóa đơn HĐĐT mới có chi tiết để xem (nhập tay thì không). -->
      <div
        v-if="invoiceDetail"
        class="flex items-center justify-between rounded border border-gray-200 bg-gray-50 px-3 py-2 text-sm"
      >
        <span class="text-gray-600"><i class="pi pi-file-pdf mr-2" />Hóa đơn điện tử</span>
        <Button
          label="Xem PDF"
          icon="pi pi-eye"
          link
          size="small"
          class="text-blue-700"
          @click="invoicePdfVisible = true"
        />
      </div>

      <div class="grid grid-cols-1 sm:grid-cols-2 gap-3 text-sm md:grid-cols-4">
        <div>
          <div class="text-gray-500">Ngày lập phiếu</div>
          <div class="font-medium">{{ postingDate || "—" }}</div>
        </div>
        <div>
          <div class="text-gray-500">Loại</div>
          <div class="font-medium">{{ isOutbound ? "Xuất kho" : "Nhập kho" }}</div>
        </div>
        <div>
          <div class="text-gray-500">{{ isOutbound ? "Khách hàng" : "Nhà cung cấp" }}</div>
          <div class="font-medium">{{ partnerName || "—" }}</div>
        </div>
        <div>
          <div class="text-gray-500">Mã {{ isOutbound ? "khách" : "NCC" }}</div>
          <div class="font-medium">{{ partnerCode || "—" }}</div>
        </div>
      </div>

      <p v-if="first.description" class="text-sm text-gray-600">{{ first.description }}</p>

      <!-- Bảng dòng hàng: dùng table thường vì chỉ đọc, số dòng ít (1 phiếu) -->
      <div class="overflow-x-auto rounded border border-gray-200">
        <table class="w-full text-sm">
          <thead class="bg-gray-50 text-xs text-gray-500">
            <tr>
              <th class="w-10 px-2 py-1 text-right">#</th>
              <th class="px-2 py-1 text-left">Mã hàng</th>
              <th class="px-2 py-1 text-left">Tên hàng</th>
              <th class="w-14 px-2 py-1 text-left">ĐVT</th>
              <th class="w-24 px-2 py-1 text-right">SL</th>
              <th class="w-32 px-2 py-1 text-right">Đơn giá</th>
              <th v-if="hasVatRows" class="w-20 px-2 py-1 text-right">GTGT</th>
              <th class="w-36 px-2 py-1 text-right">Thành tiền</th>
              <th class="w-28 px-2 py-1 text-right">Tiền CK</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(r, i) in lines" :key="r.product_code + i" class="border-t border-gray-100">
              <td class="px-2 py-1 text-right text-gray-400">{{ i + 1 }}</td>
              <td class="px-2 py-1">{{ r.product_code }}</td>
              <td class="px-2 py-1">{{ r.product_name }}</td>
              <td class="px-2 py-1">{{ r.unit }}</td>
              <td class="px-2 py-1 text-right">{{ fmt(r.quantity) }}</td>
              <td class="px-2 py-1 text-right">{{ money(lineUnitGross(r)) }}</td>
              <td v-if="hasVatRows" class="px-2 py-1 text-right">
                <!-- Vòng 2 số lẻ để 0.08×100 không in ra "8.000000000000002". -->
                {{ r.vat_rate > 0 ? `${Math.round(r.vat_rate * 1000) / 10} %` : "—" }}
              </td>
              <td class="px-2 py-1 text-right font-medium">{{ money(lineGross(r)) }}</td>
              <td class="px-2 py-1 text-right">
                <span v-if="r.discount" class="text-emerald-600">−{{ money(r.discount) }}</span>
                <span v-else class="text-gray-400">—</span>
              </td>
            </tr>
            <tr v-if="!lines.length">
              <td :colspan="hasVatRows ? 9 : 8" class="px-2 py-3 text-center text-gray-400">
                Phiếu không có dòng hàng hóa
              </td>
            </tr>
          </tbody>
          <tfoot>
            <tr class="border-t border-gray-200 font-semibold">
              <td :colspan="hasVatRows ? 7 : 6" class="px-2 py-1 text-right text-gray-500">
                Cộng thành tiền
              </td>
              <td class="px-2 py-1 text-right">{{ money(grossTotal) }}</td>
              <td class="px-2 py-1 text-right text-emerald-600">
                {{ discountTotal ? `−${money(discountTotal)}` : "" }}
              </td>
            </tr>
            <tr class="font-semibold">
              <td :colspan="hasVatRows ? 7 : 6" class="px-2 py-1 text-right text-gray-500">
                Thành tiền (sau CK)
              </td>
              <td class="px-2 py-1 text-right">{{ money(netTotal) }}</td>
              <td class="px-2 py-1 text-right"></td>
            </tr>
          </tfoot>
        </table>
      </div>

      <!-- Lô sản xuất: bảng thành phần đã chụp — vật tư, SL, đơn giá khai, tiền
           vốn FIFO — hiện ngay trong phiếu để khỏi mở thêm hộp thoại khác. -->
      <div v-if="withComponents" class="space-y-2" data-testid="lot-components">
        <div class="flex flex-wrap items-baseline justify-between gap-2">
          <span class="text-sm font-semibold">Thành phần lô sản xuất</span>
          <span v-if="components.length" class="text-xs text-gray-500">
            Giá trị nhập theo định mức:
            <b class="text-emerald-700">{{ money(componentTotal) }}</b>
            · Tiền vốn FIFO NVL: <b>{{ money(componentCost) }}</b>
          </span>
        </div>
        <p v-if="componentsLoading" class="text-xs text-gray-500">
          <i class="pi pi-spin pi-spinner mr-1" />Đang nạp thành phần…
        </p>
        <p v-else-if="componentsError" class="text-xs text-red-600">{{ componentsError }}</p>
        <div v-else-if="components.length" class="overflow-x-auto rounded border border-gray-200">
          <table class="w-full text-sm" data-testid="lot-components-table">
            <thead class="bg-gray-50 text-xs text-gray-500">
              <tr>
                <th class="px-2 py-1 text-left">Vật tư / dịch vụ</th>
                <th class="w-14 px-2 py-1 text-left">ĐVT</th>
                <th v-if="componentHasMultipleProducts" class="px-2 py-1 text-left">Thành phẩm</th>
                <th class="w-24 px-2 py-1 text-right">Định mức/SP</th>
                <th class="w-24 px-2 py-1 text-right">SL cần</th>
                <th class="w-28 px-2 py-1 text-right">Đơn giá khai</th>
                <th class="w-32 px-2 py-1 text-right">Thành tiền</th>
                <th class="w-32 px-2 py-1 text-right">Tiền vốn FIFO</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="(r, i) in components" :key="i" class="border-t border-gray-100">
                <td class="px-2 py-1">
                  {{ r.material_name }} ({{ r.material_code }})
                  <Tag v-if="r.is_service" value="Dịch vụ" severity="warn" class="ml-1" />
                </td>
                <td class="px-2 py-1">{{ r.material_unit }}</td>
                <td v-if="componentHasMultipleProducts" class="px-2 py-1">
                  {{ r.product_name }} ({{ r.product_code }})
                </td>
                <td class="px-2 py-1 text-right">{{ fmtDec(r.per_unit_qty) }}</td>
                <td class="px-2 py-1 text-right">{{ fmtDec(r.quantity) }}</td>
                <td class="px-2 py-1 text-right">{{ money(r.unit_price) }}</td>
                <td class="px-2 py-1 text-right font-medium">{{ money(r.amount) }}</td>
                <td class="px-2 py-1 text-right">
                  <span v-if="r.is_service" class="text-gray-400">—</span>
                  <span v-else>{{ money(r.cost_amount) }}</span>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
        <p v-else class="text-xs text-gray-500">
          Lô này chưa có snapshot thành phần (tạo trước bản cập nhật).
        </p>
        <p v-if="componentHasService" class="text-xs text-gray-400">
          Dòng “Dịch vụ” chỉ tính vào giá trị lô theo đơn giá khai — không có tồn kho nên không trừ
          kho. NVL thật đã trừ FIFO khi lưu lô.
        </p>
      </div>

      <div v-if="first.note" class="text-xs text-gray-500">Ghi chú: {{ first.note }}</div>
      <div v-if="extraLines.length" class="space-y-0.5 text-xs text-gray-400">
        <div v-for="(r, i) in extraLines" :key="i">
          {{ r.description || "Bút toán điều chỉnh" }} · Nợ {{ r.debit_account }} · Có
          {{ r.credit_account }}
        </div>
      </div>
    </div>

    <p v-else-if="!loading" class="py-4 text-sm text-gray-500">Chứng từ không có dữ liệu.</p>
  </AppDialog>

  <InvoicePdfDialog
    v-model:visible="invoicePdfVisible"
    :detail-json="invoiceDetail"
    :title="`Hóa đơn của phiếu ${voucherNo}`"
  />
</template>
