<script setup lang="ts">
// Màn in phiếu nhập kho (01-VT) / phiếu xuất kho (02-VT) theo bộ mẫu TT152.
import { api } from "@/db";
import type { BusinessConfig, VoucherRow } from "@/types";
import { fmtInt as fmt, fmtVnd } from "@/utils/format";

const route = useRoute();
const router = useRouter();
const toast = useToast();

const rows = ref<VoucherRow[]>([]);
const biz = ref<BusinessConfig | null>(null);
const loading = ref(true);
const errorMsg = ref("");

const voucherNo = computed(() => String(route.params.voucherNo ?? ""));
const first = computed(() => rows.value[0]);
const entryType = computed(() => first.value?.entry_type ?? "");
const isInbound = computed(() => entryType.value === "PN");
const title = computed(() => (isInbound.value ? "PHIẾU NHẬP KHO" : "PHIẾU XUẤT KHO"));
const formNo = computed(() => (isInbound.value ? "01-VT" : "02-VT"));

/** Dòng bảng in: khi in Phiếu nhập kho (PN), lọc bỏ bút toán tách thuế GTGT đầu vào
 *  (dòng không có mã hàng, SL = 0) để bảng in chỉ hiển thị dòng hàng hóa thực nhập. */
const displayRows = computed(() =>
  isInbound.value ? rows.value.filter((r) => r.product_code !== "") : rows.value,
);

const total = computed(() =>
  displayRows.value.reduce(
    (s, r) => s + (r.amount > 0 ? r.amount : r.quantity * r.unit_price),
    0,
  ),
);

const postedDateLabel = computed(() => formatVnDate(first.value?.posting_date ?? ""));

/** "YYYY-MM-DD" → "Ngày 05 tháng 01 năm 2030" */
function formatVnDate(dateStr: string): string {
  const m = /^(\d{4})-(\d{1,2})-(\d{1,2})/.exec(dateStr);
  if (!m) return "";
  const y = m[1];
  const mo = String(Number(m[2])).padStart(2, "0");
  const d = String(Number(m[3])).padStart(2, "0");
  return `Ngày ${d} tháng ${mo} năm ${y}`;
}

// ─── Số tiền (nguyên VND) bằng chữ — tối đa hàng tỷ ───
const ONES = ["", "một", "hai", "ba", "bốn", "năm", "sáu", "bảy", "tám", "chín"];
const TENS = ["", "mười", "hai mươi", "ba mươi", "bốn mươi", "năm mươi", "sáu mươi", "bảy mươi", "tám mươi", "chín mươi"];

/** Đọc số 1..99 */
function readTwo(r: number): string {
  const t = Math.floor(r / 10);
  const o = r % 10;
  if (t === 0) return ONES[o];
  let s = t === 1 ? "mười" : TENS[t];
  if (o > 0) {
    if (o === 5) s += " lăm";
    else if (o === 1 && t > 1) s += " mốt";
    else if (o === 4 && t > 1) s += " tư";
    else s += " " + ONES[o];
  }
  return s;
}

/**
 * Đọc nhóm 3 chữ số (0..999).
 * `withHundreds = true` khi có nhóm cao hơn đã được đọc — lúc đó ô trăm rỗng
 * phải đọc thành "không trăm …" (vd: 1.005.000 → "không trăm linh năm nghìn").
 */
function readGroup(g: number, withHundreds: boolean): string {
  if (g === 0) return "";
  const h = Math.floor(g / 100);
  const r = g % 100;
  let s = "";
  if (h > 0) s += ONES[h] + " trăm";
  else if (withHundreds) s += "không trăm";
  if (r > 0) {
    if (s) s += " ";
    if (r < 10) {
      // hàng chục rỗng → "linh X" (trừ nhóm cao nhất chỉ có 1-9)
      s += withHundreds || h > 0 ? "linh " + ONES[r] : ONES[r];
    } else {
      s += readTwo(r);
    }
  }
  return s;
}

/** Số nguyên VND → chữ (không lẻ xu), tối đa hàng tỷ. Vd: 1500000 → "Một triệu năm trăm nghìn đồng chẵn" */
function vndToWords(n: number): string {
  const amount = Math.floor(Math.abs(n));
  if (amount === 0) return "Không đồng";
  const units = ["", "nghìn", "triệu", "tỷ"];
  const groups: number[] = [];
  let rest = amount;
  for (let i = 0; i < 4; i++) {
    groups.push(rest % 1000);
    rest = Math.floor(rest / 1000);
  }
  const parts: string[] = [];
  let seen = false;
  for (let i = 3; i >= 0; i--) {
    const g = groups[i];
    if (g === 0) continue;
    const word = readGroup(g, seen);
    parts.push(i > 0 ? `${word} ${units[i]}` : word);
    seen = true;
  }
  let s = parts.join(" ");
  s += amount % 1000 === 0 ? " đồng chẵn" : " đồng";
  return s.charAt(0).toUpperCase() + s.slice(1);
}

function printNow() {
  try {
    window.print();
  } catch {
    // một số webview từ chối in — bỏ qua
  }
}

let printTimer: number | undefined;

onMounted(async () => {
  try {
    const [voucherRows, config] = await Promise.all([
      api.getVoucher(voucherNo.value),
      api.getBusinessConfig(),
    ]);
    rows.value = voucherRows;
    biz.value = config;
  } catch (e) {
    errorMsg.value = "Không tìm thấy chứng từ";
    const detail =
      typeof e === "string" && e.trim() ? e : `Không tìm thấy chứng từ ${voucherNo.value}`;
    toast.add({ severity: "error", summary: "Không thể in phiếu", detail });
    loading.value = false;
    return;
  }
  loading.value = false;
  await nextTick();
  printTimer = window.setTimeout(() => {
    try {
      window.print();
    } catch {
      // bỏ qua
    }
  }, 400);
});

onBeforeUnmount(() => {
  if (printTimer !== undefined) window.clearTimeout(printTimer);
});
</script>

<template>
  <div>
    <!-- Thanh lệnh (ẩn khi in) -->
    <div class="print-hide mb-4 flex flex-wrap items-center gap-3">
      <Button label="In lại" icon="pi pi-print" @click="printNow" />
      <Button label="Quay lại" icon="pi pi-arrow-left" severity="secondary" @click="router.back()" />
    </div>

    <div v-if="loading" class="print-hide rounded-lg bg-white p-10 text-center text-sm text-gray-500 shadow">
      Đang tải phiếu…
    </div>

    <div v-else-if="errorMsg" class="print-hide rounded-lg bg-white p-10 text-center shadow">
      <p class="mb-3 font-semibold text-red-600">{{ errorMsg }}</p>
      <Button label="Quay lại" icon="pi pi-arrow-left" severity="secondary" @click="router.back()" />
    </div>

    <!-- Vùng phiếu in -->
    <div
      v-else-if="rows.length && first"
      class="print-area mx-auto max-w-[210mm] bg-white px-10 py-6 shadow-xl"
    >
      <!-- Đơn vị + mẫu số -->
      <div class="flex items-start justify-between text-sm">
        <div>
          <p class="font-bold uppercase leading-snug">{{ biz?.name || "HỘ KINH DOANH" }}</p>
          <p class="mt-0.5 text-xs">{{ biz?.address }}</p>
          <p v-if="biz?.tax_code" class="mt-0.5 text-xs">Mã số thuế: {{ biz?.tax_code }}</p>
        </div>
        <div class="whitespace-nowrap text-right text-xs">
          <p class="font-semibold">Mẫu số: {{ formNo }}</p>
          <p class="text-gray-500">(Ban hành theo TT số 133/2016/TT-BTC)</p>
        </div>
      </div>

      <!-- Tiêu đề -->
      <h1 class="mt-6 text-center text-xl font-bold uppercase tracking-wide">{{ title }}</h1>
      <div class="mt-2 flex items-start justify-between text-sm">
        <span></span>
        <div class="space-y-0.5 text-right">
          <p class="font-semibold">Số: {{ first?.voucher_no }}</p>
          <p class="font-medium">{{ postedDateLabel }}</p>
        </div>
      </div>

      <!-- Thông tin đối tượng -->
      <div class="mt-4 space-y-1 text-sm">
        <p v-if="isInbound">
          Họ và tên người giao hàng:
          <span class="font-semibold">{{ first?.supplier_name || "………………………………" }}</span>
        </p>
        <p v-else>
          Họ và tên người nhận hàng:
          <span class="font-semibold">{{ first?.customer_name || "………………………………" }}</span>
        </p>
        <p v-if="isInbound && first?.doc_date">
          Theo hóa đơn số: …………………… ngày {{ formatVnDate(first?.doc_date ?? "") }}
        </p>
        <p v-if="!isInbound">
          Lý do xuất kho:
          <span class="font-semibold">{{ first?.description || "………………………………" }}</span>
        </p>
      </div>

      <!-- Bảng dòng -->
      <table class="mt-4 w-full table-fixed border-collapse text-sm">
        <thead>
          <tr>
            <th class="w-9 border border-black px-1 py-1.5 text-center font-semibold">STT</th>
            <th class="border border-black px-1 py-1.5 text-center font-semibold">Tên, nhãn hiệu, quy cách vật tư</th>
            <th class="w-16 border border-black px-1 py-1.5 text-center font-semibold">Mã</th>
            <th class="w-12 border border-black px-1 py-1.5 text-center font-semibold">ĐVT</th>
            <th class="w-16 border border-black px-1 py-1.5 text-center font-semibold">Số lượng</th>
            <th class="w-24 border border-black px-1 py-1.5 text-center font-semibold">Đơn giá</th>
            <th class="w-28 border border-black px-1 py-1.5 text-center font-semibold">Thành tiền</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="(r, i) in displayRows" :key="i">
            <td class="border border-black px-1 py-1 text-center">{{ i + 1 }}</td>
            <td class="border border-black px-1 py-1 break-words">
              {{ r.product_name || r.product_code }}
              <span class="text-[10px] text-gray-400">({{ r.product_code }})</span>
            </td>
            <td class="border border-black px-1 py-1 text-center">{{ r.product_code }}</td>
            <td class="border border-black px-1 py-1 text-center">{{ r.unit }}</td>
            <td class="border border-black px-1 py-1 text-right">{{ fmt(r.quantity) }}</td>
            <td class="border border-black px-1 py-1 text-right">{{ fmtVnd(r.unit_price) }}</td>
            <td class="border border-black px-1 py-1 text-right">{{ fmtVnd(r.amount > 0 ? r.amount : r.quantity * r.unit_price) }}</td>
          </tr>
        </tbody>
      </table>

      <!-- Tổng tiền -->
      <div class="mt-3 space-y-1 text-sm font-medium">
        <p>
          Cộng thành tiền (bằng số):
          <span class="text-base font-bold">{{ fmt(total) }} đ</span>
        </p>
        <p>Số tiền bằng chữ: <span class="font-semibold">{{ vndToWords(total) }}</span></p>
      </div>

      <!-- Chữ ký -->
      <div class="mt-12">
        <p class="text-right text-sm">{{ postedDateLabel }}</p>
        <div class="mt-2 grid grid-cols-4 gap-3 text-center text-sm">
          <div>
            <p class="font-semibold">Người lập phiếu</p>
            <div class="h-16"></div>
            <p>Họ tên: ………………</p>
          </div>
          <div>
            <p class="font-semibold">Người giao (nhận) hàng</p>
            <div class="h-16"></div>
            <p>Họ tên: ………………</p>
          </div>
          <div>
            <p class="font-semibold">Thủ kho</p>
            <div class="h-16"></div>
            <p>Họ tên: ………………</p>
          </div>
          <div>
            <p class="font-semibold">Kế toán trưởng</p>
            <div class="h-16"></div>
            <p>Họ tên: ………………</p>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style>
/* CSS in — không scoped để áp dụng lên layout app (App.vue) khi in */
@media print {
  @page {
    size: A4 portrait;
    margin: 12mm;
  }
  html,
  body {
    background: #ffffff !important;
  }
  body {
    color: #000 !important;
  }
  /* Ẩn chrome của app */
  aside,
  header {
    display: none !important;
  }
  main > div.flex-1 {
    padding: 0 !important;
    overflow: visible !important;
  }
  /* Ẩn phần tử màn hình (nút, thông báo…) */
  .print-hide {
    display: none !important;
  }
  /* Chỉ hiện vùng phiếu, nền trắng, không shadow */
  .print-area {
    box-shadow: none !important;
    margin: 0 !important;
    max-width: none !important;
    padding: 0 !important;
    width: 100% !important;
  }
  .print-area tr {
    page-break-inside: avoid;
  }
  .print-area .text-gray-400,
  .print-area .text-gray-500 {
    color: #000 !important;
  }
}
</style>