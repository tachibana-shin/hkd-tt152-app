<script setup lang="ts">
/**
 * Tra cứu hóa đơn trên cổng HĐĐT (mục menu cùng cấp "HĐĐT").
 *
 * Ma trận 2 tầng của cổng quyết định endpoint:
 *   Hóa đơn ra (bán ra)  × {HĐ điện tử → query,    máy tính tiền → sco-query}
 *   Hóa đơn vào (mua vào) × {HĐ điện tử → query,    máy tính tiền → sco-query}
 * Cấu hình tài khoản cổng + captcha nằm ở màn HĐĐT.
 */
import { api } from "@/db";
import { usePortalSession } from "@/composables/usePortalSession";
import type { HddtInvoiceDirection, HddtInvoiceKind, HddtInvoiceRow } from "@/types";
import { HDDT_KHMSHDON, HDDT_TTHAI, HDDT_TTXLY, HDDT_TTXLY_ALL } from "@/types";

const toast = useToast();
const portal = usePortalSession();
const { statusLoaded, loggingIn, loggedIn } = portal;

function toastError(summary: string, e: unknown) {
  toast.add({ severity: "error", summary, detail: String(e), life: 5000 });
}

// ─── Tra cứu hóa đơn (trang `/tra-cuu/tra-cuu-hoa-don` của cổng) ───
// Cổng có 2 tầng chọn, mỗi tầng quyết định endpoint:
//   Hóa đơn ra (bán ra)  × {HĐĐT thường → query,    máy tính tiền → sco-query}
//   Hóa đơn vào (mua vào) × {HĐĐT thường → query,    máy tính tiền → sco-query}
const searchLoading = ref(false);
const searchRows = ref<HddtInvoiceRow[]>([]);
const searchTotal = ref(0);
const searchTime = ref(0);
const searchMsg = ref("");
/** Ngày lập từ → dd/MM/yyyy (mặc định: tháng trước + 1 ngày, như cổng). */
const searchFrom = ref<Date | null>(null);
/** Ngày lập đến → dd/MM/yyyy (mặc định: hôm nay, như cổng). */
const searchTo = ref<Date | null>(null);
const searchDirection = ref<HddtInvoiceDirection>("sold");
/** Mặc định "máy tính tiền" (sco-query) — đây là loại HĐ phổ biến của HKD. */
const searchKind = ref<HddtInvoiceKind>("cash-register");
const searchTthai = ref("0");
const searchTtxly = ref(HDDT_TTXLY_ALL);
const searchKhmshdon = ref("");
const searchKhhdon = ref("");
const searchShdon = ref("");
const searchNbmst = ref("");
const searchNmcmnd = ref("");
const searchUnhiem = ref(false);
/** Cursor phân trang (keySet) — stack các state đã đi qua. */
const pageStates = ref<(string | null)[]>([null]);
const pageIdx = ref(0);
const pageSize = ref(15);

const tthaiOptions = Object.entries(HDDT_TTHAI).map(([value, label]) => ({
  label,
  value,
}));
const ttxlyOptions = [
  { label: "Tất cả", value: HDDT_TTXLY_ALL },
  ...Object.entries(HDDT_TTXLY).map(([value, label]) => ({ label, value })),
];
const khmshdonOptions = Object.entries(HDDT_KHMSHDON).map(([value, label]) => ({
  label,
  value,
}));

/** Nhãn tab lớn (khớp chữ trên cổng). */
const directionOptions = [
  { label: "Hóa đơn ra (bán ra)", value: "sold" as const },
  { label: "Hóa đơn vào (mua vào)", value: "purchase" as const },
];
/** Nhãn tab nhỏ (loại hóa đơn) — cổng gọi "Hóa đơn điện tử" là hóa đơn thường. */
const kindOptions = [
  { label: "Hóa đơn điện tử", value: "regular" as const },
  { label: "Hóa đơn máy tính tiền", value: "cash-register" as const },
];
/** MST đối tác: người mua khi tra HĐ ra, người bán khi tra HĐ vào. */
const nbmstLabel = computed(() =>
  searchDirection.value === "sold" ? "MST người mua" : "MST người bán",
);

function defaultRange() {
  const to = new Date();
  const from = new Date();
  from.setMonth(from.getMonth() - 1);
  from.setDate(from.getDate() + 1); // cổng: subtract(1,"month").add(1,"days")
  return { from, to };
}

function fmtDateVN(d: Date | null) {
  if (!d) return "";
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${day}/${m}/${y}`;
}

/** Trạng thái hóa đơn → nhãn (rỗng nếu không biết mã). */
function tthaiLabel(row: HddtInvoiceRow) {
  return row.tthai != null ? (HDDT_TTHAI[Number(row.tthai)] ?? String(row.tthai)) : "—";
}

/** Trạng thái xử lý → nhãn (rỗng nếu không biết mã). */
function ttxlyLabel(row: HddtInvoiceRow) {
  return row.ttxly != null ? (HDDT_TTXLY[Number(row.ttxly)] ?? String(row.ttxly)) : "—";
}

/** Mẫu số hóa đơn → nhãn (dùng khi API không kèm tên loại). */
function khmshdonLabel(row: HddtInvoiceRow) {
  if (row.khmshdon == null) return "—";
  return HDDT_KHMSHDON[Number(row.khmshdon)] ?? String(row.khmshdon);
}

// Cổng hiển thị cột "đối tác" khác nhau theo hướng tra cứu:
//   HĐ ra  → cột "Mã số thuế" = bên bán (nbmst), "Thông tin hóa đơn" = người mua.
//   HĐ vào → cột "Mã số thuế" = bên mua (nmmst), "Thông tin người bán" = người bán.
const isPurchase = computed(() => searchDirection.value === "purchase");
const partyHeader = computed(() =>
  isPurchase.value ? "Thông tin người bán" : "Thông tin hóa đơn",
);
const partyMstLabel = computed(() => (isPurchase.value ? "MST người bán" : "MST người mua"));
const partyNameLabel = computed(() => (isPurchase.value ? "Tên người bán" : "Tên người mua"));
/** MST hiển thị ở cột thứ hai: người mua (ra) / người bán (vào). */
function partyMst(row: HddtInvoiceRow) {
  return (isPurchase.value ? row.nbmst : row.nmmst) || "—";
}
function partyName(row: HddtInvoiceRow) {
  if (isPurchase.value) return row.nbten || "—";
  return row.nmten || row.nmtnmua || "—";
}
/** Cột "Mã số thuế": HĐ ra hiện bên bán, HĐ vào hiện bên mua. */
function mstColumn(row: HddtInvoiceRow) {
  return (isPurchase.value ? row.nmmst : row.nbmst) || "—";
}

/** Ngày lập (`tdlap`, ISO) → dd/MM/yyyy. */
function fmtTdlap(v: unknown) {
  if (v == null || v === "") return "—";
  const d = new Date(String(v));
  if (Number.isNaN(d.getTime())) return String(v);
  return d.toLocaleDateString("vi-VN");
}

function fmtMoney(v: unknown) {
  if (v === null || v === undefined || v === "") return "—";
  const n = typeof v === "number" ? v : Number(v);
  if (Number.isNaN(n)) return String(v);
  return n.toLocaleString("vi-VN");
}

async function runSearch(state: string | null) {
  searchLoading.value = true;
  searchMsg.value = "";
  try {
    const res = await api.hddtListInvoices({
      direction: searchDirection.value,
      kind: searchKind.value,
      from: fmtDateVN(searchFrom.value),
      to: fmtDateVN(searchTo.value),
      tthai: searchTthai.value || undefined,
      ttxly: searchTtxly.value || undefined,
      khmshdon: searchKhmshdon.value || undefined,
      khhdon: searchKhhdon.value.trim() || undefined,
      shdon: searchShdon.value.trim() || undefined,
      nbmst: searchNbmst.value.trim() || undefined,
      nmcmnd: searchNmcmnd.value.trim() || undefined,
      unhiem: searchUnhiem.value,
      size: pageSize.value,
      state,
    });
    searchRows.value = res.datas ?? [];
    searchTotal.value = res.total ?? 0;
    searchTime.value = res.time ?? 0;
    searchMsg.value = `Có ${searchTotal.value.toLocaleString("vi-VN")} kết quả (đã tải ${
      searchRows.value.length
    })`;
    return res.state;
  } catch (e) {
    toastError("Lỗi tra cứu hóa đơn", e);
    return null;
  } finally {
    searchLoading.value = false;
  }
}

/** Cổng chỉ nhận khoảng ngày ≤ 1 tháng (HTTP 400 nếu vượt) — chặn sớm ở UI. */
const dateRangeTooLong = computed(() => {
  const from = searchFrom.value;
  const to = searchTo.value;
  if (!from || !to) return false;
  return (to.getTime() - from.getTime()) / 86_400_000 > 31;
});

/** Bấm "Tìm kiếm" → bắt đầu từ trang đầu. */
async function onSearchSubmit() {
  if (dateRangeTooLong.value) {
    toastError(
      "Khoảng ngày không hợp lệ",
      "Cổng HĐĐT chỉ cho tra cứu tối đa 1 tháng. Vui lòng thu hẹp khoảng từ ngày.",
    );
    return;
  }
  pageStates.value = [null];
  pageIdx.value = 0;
  const st = await runSearch(null);
  if (st) pageStates.value = [null, st];
}

/** Đổi tab (vào/ra hoặc loại hóa đơn) → bỏ kết quả của endpoint cũ. */
function resetSearchResults() {
  searchRows.value = [];
  searchTotal.value = 0;
  searchMsg.value = "";
  pageStates.value = [null];
  pageIdx.value = 0;
}

/** PrimeVue Tabs phát `update:value` (string | number) — ép về union của ta. */
function onDirectionChange(value: string | number) {
  searchDirection.value = value as HddtInvoiceDirection;
  resetSearchResults();
  // Cổng mặc định "Kết quả kiểm tra" = "Đã cấp mã hóa đơn" (5) ở tab hóa đơn vào.
  if (searchDirection.value === "purchase") {
    searchTtxly.value = "5";
  }
}

function onKindChange(value: string | number) {
  searchKind.value = value as HddtInvoiceKind;
  resetSearchResults();
}

/** Trang sau — dùng cursor `state` của trang hiện tại (keySet như cổng). */
async function onNextPage() {
  const curState = pageStates.value[pageIdx.value] ?? null;
  if (pageIdx.value < pageStates.value.length - 1) {
    // đã có cursor phía trước → quay lại
    pageIdx.value += 1;
    await runSearch(pageStates.value[pageIdx.value]);
    return;
  }
  const st = await runSearch(curState);
  if (st) {
    pageStates.value = [...pageStates.value.slice(0, pageIdx.value + 1), st];
    pageIdx.value += 1;
  }
}

/** Trang trước — quay lại cursor đã lưu. */
async function onPrevPage() {
  if (pageIdx.value <= 0) return;
  pageIdx.value -= 1;
  await runSearch(pageStates.value[pageIdx.value]);
}

function onResetSearch() {
  const { from, to } = defaultRange();
  searchFrom.value = from;
  searchTo.value = to;
  searchTthai.value = "0";
  searchTtxly.value = HDDT_TTXLY_ALL;
  searchKhmshdon.value = "";
  searchKhhdon.value = "";
  searchShdon.value = "";
  searchNbmst.value = "";
  searchNmcmnd.value = "";
  searchUnhiem.value = false;
}

/** Có trang sau không: pageIdx chưa tới cuối stack, hoặc chưa lấy hết. */
const canNextPage = computed(() => {
  if (searchLoading.value) return false;
  const lastState = pageStates.value[pageStates.value.length - 1];
  if (pageIdx.value < pageStates.value.length - 1) return true;
  // Trang hiện tại còn cursor → còn dữ liệu phía sau
  return !!lastState;
});
const canPrevPage = computed(() => pageIdx.value > 0 && !searchLoading.value);

onMounted(() => {
  // Range mặc định như cổng: tháng trước → hôm nay.
  const { from, to } = defaultRange();
  searchFrom.value = from;
  searchTo.value = to;
  void portal.loadPortalStatus();
});
</script>

<template>
  <div class="space-y-4">
    <Toolbar>
      <template #start>
        <div class="flex items-center gap-2">
          <i class="pi pi-search text-xl text-primary-500" />
          <h3 class="font-semibold">Tra cứu hóa đơn HĐĐT</h3>
        </div>
      </template>
      <template #end>
        <Button
          v-if="statusLoaded && !loggedIn"
          label="Đăng nhập cổng HĐĐT"
          icon="pi pi-sign-in"
          size="small"
          :loading="loggingIn"
          @click="portal.loginPortal"
        />
      </template>
    </Toolbar>

    <SectionCard>
      <p v-if="statusLoaded && !loggedIn" class="mb-3 text-sm text-orange-600">
        <i class="pi pi-exclamation-triangle mr-1" />Chưa đăng nhập cổng HĐĐT nên chưa tra cứu được.
        Bấm <b>Đăng nhập cổng HĐĐT</b> ở góc trên bên phải; nếu cổng bắt nhập captcha thì mở màn
        <b>HĐĐT</b> để nhập tay.
      </p>

      <!-- Tầng 1: hóa đơn ra (bán ra) / hóa đơn vào (mua vào) — như cổng -->
      <Tabs :value="searchDirection" class="mb-3" @update:value="onDirectionChange">
        <TabList>
          <Tab v-for="d in directionOptions" :key="d.value" :value="d.value" class="text-sm">
            {{ d.label }}
          </Tab>
        </TabList>
      </Tabs>
      <!-- Tầng 2: hóa đơn điện tử (thường) / hóa đơn máy tính tiền -->
      <Tabs :value="searchKind" class="mb-3" @update:value="onKindChange">
        <TabList>
          <Tab v-for="k in kindOptions" :key="k.value" :value="k.value" class="text-sm">
            {{ k.label }}
          </Tab>
        </TabList>
      </Tabs>

      <div class="mb-3 flex flex-wrap items-end gap-3">
        <div>
          <label class="mb-1 block text-xs text-gray-500">Từ ngày lập</label>
          <DatePicker v-model="searchFrom" dateFormat="dd/mm/yy" class="w-40" />
        </div>
        <div>
          <label class="mb-1 block text-xs text-gray-500">Đến ngày lập</label>
          <DatePicker v-model="searchTo" dateFormat="dd/mm/yy" class="w-40" />
        </div>
        <div>
          <label class="mb-1 block text-xs text-gray-500">{{ nbmstLabel }}</label>
          <InputText v-model="searchNbmst" class="w-40" placeholder="MST" />
        </div>
        <div>
          <label class="mb-1 block text-xs text-gray-500">CCCD người mua</label>
          <InputText v-model="searchNmcmnd" class="w-36" placeholder="Số CCCD" />
        </div>
        <div>
          <label class="mb-1 block text-xs text-gray-500">Trạng thái hóa đơn</label>
          <Select
            v-model="searchTthai"
            :options="tthaiOptions"
            option-label="label"
            option-value="value"
            class="w-44"
          />
        </div>
        <div>
          <label class="mb-1 block text-xs text-gray-500">Kết quả kiểm tra</label>
          <Select
            v-model="searchTtxly"
            :options="ttxlyOptions"
            option-label="label"
            option-value="value"
            class="w-48"
          />
        </div>
        <div>
          <label class="mb-1 block text-xs text-gray-500">Ký hiệu mẫu số HĐ</label>
          <Select
            v-model="searchKhmshdon"
            :options="khmshdonOptions"
            option-label="label"
            option-value="value"
            class="w-56"
            showClear
            placeholder="Chọn"
          />
        </div>
        <div>
          <label class="mb-1 block text-xs text-gray-500">Số hóa đơn</label>
          <InputText v-model="searchShdon" class="w-32" placeholder="VD: 821" />
        </div>
        <div>
          <label class="mb-1 block text-xs text-gray-500">Ký hiệu hóa đơn</label>
          <InputText v-model="searchKhhdon" class="w-40" placeholder="VD: C26THN" />
        </div>
        <div class="pb-1">
          <label class="mb-1 flex items-center gap-2 text-xs text-gray-500">
            <Checkbox v-model="searchUnhiem" binary />
            Hóa đơn ủy nhiệm
          </label>
        </div>
        <Button
          label="Tìm kiếm"
          icon="pi pi-search"
          :loading="searchLoading"
          @click="onSearchSubmit"
        />
        <Button
          label="Bỏ tìm kiếm"
          icon="pi pi-times"
          severity="secondary"
          text
          @click="onResetSearch"
        />
      </div>

      <div class="mb-2 flex flex-wrap items-center gap-3 text-sm">
        <span v-if="dateRangeTooLong" class="text-orange-600">
          <i class="pi pi-exclamation-triangle mr-1" />Cổng HĐĐT chỉ cho tra cứu tối đa 1 tháng —
          vui lòng thu hẹp khoảng từ ngày.
        </span>
        <span v-else-if="searchMsg" class="text-gray-600">
          <i class="pi pi-info-circle mr-1" />{{ searchMsg }}
        </span>
        <span v-else class="text-gray-400">
          Chọn loại hóa đơn rồi bấm "Tìm kiếm" để tra trên cổng HĐĐT (khoảng ngày mặc định: tháng
          trước → hôm nay).
        </span>
        <div class="ml-auto flex items-center gap-2">
          <Button
            icon="pi pi-chevron-left"
            size="small"
            severity="secondary"
            :disabled="!canPrevPage"
            @click="onPrevPage"
          />
          <span class="text-xs text-gray-500">
            Trang {{ pageIdx + 1 }} / {{ Math.max(1, Math.ceil(searchTotal / pageSize)) }}
          </span>
          <Button
            icon="pi pi-chevron-right"
            size="small"
            severity="secondary"
            :disabled="!canNextPage"
            @click="onNextPage"
          />
        </div>
      </div>

      <AppDataTable
        :value="searchRows"
        :loading="searchLoading"
        stripedRows
        scrollable
        scrollHeight="480px"
        size="small"
        class="mt-1"
      >
        <Column header="STT" :style="{ width: '3rem' }">
          <template #body="{ index }">{{
            searchRows.length === 0 ? "" : pageIdx * pageSize + index + 1
          }}</template>
        </Column>
        <Column field="nbmst" header="Mã số thuế">
          <template #body="{ data }">{{ mstColumn(data) }}</template>
        </Column>
        <Column header="Mẫu số">
          <template #body="{ data }">{{ khmshdonLabel(data) }}</template>
        </Column>
        <Column field="khhdon" header="Ký hiệu HĐ">
          <template #body="{ data }">{{ data.khhdon || "—" }}</template>
        </Column>
        <Column field="shdon" header="Số HĐ">
          <template #body="{ data }">{{ data.shdon ?? "—" }}</template>
        </Column>
        <Column header="Ngày lập" :style="{ width: '7rem' }">
          <template #body="{ data }">{{ fmtTdlap(data.tdlap) }}</template>
        </Column>
        <Column :header="partyHeader">
          <template #body="{ data }">
            <div class="text-xs leading-5">
              <div>
                <b>{{ partyMstLabel }}:</b> {{ partyMst(data) }}
              </div>
              <div>
                <b>{{ partyNameLabel }}:</b> {{ partyName(data) }}
              </div>
              <div v-if="data.nmcccd"><b>CCCD:</b> {{ data.nmcccd }}</div>
            </div>
          </template>
        </Column>
        <Column header="Tổng tiền chưa thuế" align="right">
          <template #body="{ data }">{{ fmtMoney(data.tgtcthue) }}</template>
        </Column>
        <Column header="Tổng thuế" align="right">
          <template #body="{ data }">{{ fmtMoney(data.tgtthue) }}</template>
        </Column>
        <Column header="Tổng thanh toán" align="right">
          <template #body="{ data }">
            {{ fmtMoney(data.tgtttbso)
            }}<span v-if="data.dvtte" class="text-xs text-gray-500"> {{ data.dvtte }}</span>
          </template>
        </Column>
        <Column header="Trạng thái HĐ">
          <template #body="{ data }">{{ tthaiLabel(data) }}</template>
        </Column>
        <Column header="Kết quả xử lý">
          <template #body="{ data }">
            <Tag
              v-if="data.ttxly == 3"
              :value="ttxlyLabel(data)"
              severity="danger"
              :title="String(data.ldo ?? '')"
            />
            <Tag v-else-if="data.ttxly == 5" :value="ttxlyLabel(data)" severity="success" />
            <Tag v-else :value="ttxlyLabel(data)" severity="secondary" />
          </template>
        </Column>
      </AppDataTable>
      <p v-if="searchTime" class="mt-2 text-xs text-gray-400">
        <i class="pi pi-clock mr-1" />Thời gian truy vấn cổng: {{ searchTime }} ms
      </p>
    </SectionCard>
  </div>
</template>
