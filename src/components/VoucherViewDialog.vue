<script setup lang="ts">
/**
 * Xem 1 phiếu nhập kho / phiếu xuất kho (chỉ đọc).
 *
 * Dùng chung cho: danh sách Nhập kho, danh sách Xuất kho và màn Đồng bộ HĐĐT
 * (bấm vào số `PN…` trong cột "Phiếu nhập"). Dữ liệu lấy từ `get_voucher` — cùng
 * nguồn với màn in nên số tiền/đơn giá luôn khớp chứng từ gốc.
 */
import { api } from "@/db";
import type { VoucherRow } from "@/types";
import { fmtInt as fmt, fmtVnd as money } from "@/utils/format";

const visible = defineModel<boolean>("visible", { default: false });
const props = defineProps<{ voucherNo: string }>();

const toast = useToast();
const router = useRouter();
const rows = ref<VoucherRow[]>([]);
const loading = ref(false);
const errorMsg = ref("");

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
    try {
      rows.value = await api.getVoucher(no);
    } catch (e) {
      errorMsg.value = String(e);
      toast.add({ severity: "error", summary: "Không xem được phiếu", detail: String(e) });
    } finally {
      loading.value = false;
    }
  },
  { immediate: true },
);
</script>

<template>
  <AppDialog
    v-model:visible="visible"
    :header="`${title} · ${formNo}`"
    width="max-w-4xl"
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
              <td class="px-2 py-1 text-right font-medium">{{ money(lineGross(r)) }}</td>
              <td class="px-2 py-1 text-right">
                <span v-if="r.discount" class="text-emerald-600">−{{ money(r.discount) }}</span>
                <span v-else class="text-gray-400">—</span>
              </td>
            </tr>
            <tr v-if="!lines.length">
              <td colspan="8" class="px-2 py-3 text-center text-gray-400">
                Phiếu không có dòng hàng hóa
              </td>
            </tr>
          </tbody>
          <tfoot>
            <tr class="border-t border-gray-200 font-semibold">
              <td colspan="6" class="px-2 py-1 text-right text-gray-500">Cộng thành tiền</td>
              <td class="px-2 py-1 text-right">{{ money(grossTotal) }}</td>
              <td class="px-2 py-1 text-right text-emerald-600">
                {{ discountTotal ? `−${money(discountTotal)}` : "" }}
              </td>
            </tr>
            <tr class="font-semibold">
              <td colspan="6" class="px-2 py-1 text-right text-gray-500">Thành tiền (sau CK)</td>
              <td class="px-2 py-1 text-right">{{ money(netTotal) }}</td>
              <td class="px-2 py-1 text-right"></td>
            </tr>
          </tfoot>
        </table>
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
</template>
