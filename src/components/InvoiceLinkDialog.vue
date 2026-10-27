<script setup lang="ts">
/**
 * Ghi nhận số HĐĐT sau khi phát hành ở máy tính tiền bên kia: nhập ký hiệu + số
 * HĐ → hóa đơn chuyển sang "Đã phát hành". Dùng chung cho màn Hóa đơn và màn
 * Chờ xuất HĐĐT.
 */
import { api } from "@/db";
import { useAuthStore } from "@/stores/auth";
import { useBusinessStore } from "@/stores/business";
import { useCatalogStore } from "@/stores/catalog";
import { useInvoiceStore } from "@/stores/invoice";
import type { Invoice, InvoiceStockScan, InvoiceStockShortage } from "@/types";
import { fmtDec, parseIsoDate, toIsoDate } from "@/utils/format";

const visible = defineModel<boolean>("visible", { default: false });
const props = defineProps<{ invoice: Invoice | null }>();
const emit = defineEmits<{ done: [] }>();

const auth = useAuthStore();
const business = useBusinessStore();
const catalog = useCatalogStore();
const invoiceStore = useInvoiceStore();
const toast = useToast();
const linking = ref(false);
const form = reactive({ hddtNo: "", hddtSymbol: "", hddtDate: new Date() });

// ─── Soi tồn trước khi ghi số: liệt kê hàng thiếu + tạo nhanh lô sản xuất ───
const scan = ref<InvoiceStockScan | null>(null);
const checkingStock = ref(false);
const lotDialog = ref(false);
const lotSuggest = ref<{ code: string; quantity: number; warehouseCode: string } | null>(null);
const shortages = computed(() =>
  (scan.value?.items ?? []).filter((it) => it.kind === "product" || it.kind === "material"),
);

/** Hàng thiếu chưa xuất kho mới bị chặn ghi số — có phiếu xuất là kho đã trừ. */
async function checkStock(): Promise<InvoiceStockScan | null> {
  const inv = props.invoice;
  if (!inv?.id || (inv.voucher_no ?? "").trim()) {
    scan.value = null;
    return null;
  }
  checkingStock.value = true;
  try {
    const map = await api.invoiceShortageMap([inv.id]);
    scan.value = map[String(inv.id)] ?? null;
    return scan.value;
  } catch {
    // Soi lỗi thì để cửa chặn lúc lưu báo — không làm hỏng thao tác ghi số.
    scan.value = null;
    return null;
  } finally {
    checkingStock.value = false;
  }
}

/** Mở form tạo lô đúng phần còn thiếu của 1 thành phẩm. */
function openLot(item: InvoiceStockShortage) {
  if (item.kind !== "product" || !catalog.hasBom(item.code)) return;
  lotSuggest.value = {
    code: item.code,
    quantity: Math.max(0, Math.round((item.required - item.available) * 1e6) / 1e6),
    warehouseCode: item.warehouse_code,
  };
  lotDialog.value = true;
}

/** Lô đã tạo → soi lại tồn ngay để danh sách thiếu cập nhật tại chỗ. */
async function onLotDone() {
  await checkStock();
  if (scan.value?.level === "ok") {
    toast.add({
      severity: "success",
      summary: "Đã đủ tồn kho",
      detail: "Có thể bấm Lưu để ghi số HĐĐT.",
    });
  }
}

/// Hóa đơn lập ở màn Hóa đơn chưa có phiếu xuất → tồn chưa trừ, doanh thu chưa
/// vào sổ (tức là chưa tính vào tờ khai thuế). Mặc định lập phiếu ngay khi ghi
/// nhận số HĐĐT; bỏ chọn thì nhập số phiếu đã lập sẵn ở màn Xuất kho.
const makeOutbound = ref(true);
const outboundNo = ref("");

async function prefill() {
  if (!business.config) await business.load();
  form.hddtNo = props.invoice?.e_invoice_no ?? "";
  // Ký hiệu đã ghi trên hóa đơn → dùng lại; chưa có thì lấy ký hiệu của hộ trong hồ
  // sơ (hộp thoại này tự cập nhật trở lại hồ sơ sau mỗi lần liên kết).
  form.hddtSymbol = props.invoice?.e_invoice_symbol || business.config?.hddt_symbol || "";
  form.hddtDate = parseIsoDate(props.invoice?.e_invoice_date) ?? new Date();
  // Đã có phiếu xuất thì không cần hỏi tạo nữa.
  makeOutbound.value = !props.invoice?.voucher_no;
  outboundNo.value = props.invoice?.voucher_no ?? "";
}

watch(
  [visible, () => props.invoice?.id],
  ([open, id], [wasOpen, wasId]) => {
    if (!open || (wasOpen && id === wasId)) return;
    scan.value = null;
    void prefill();
    void catalog.loadBoms();
    void checkStock();
  },
  { immediate: true },
);

async function save() {
  if (!props.invoice?.id) return;
  const hddtNo = form.hddtNo.trim();
  if (!hddtNo) {
    toast.add({
      severity: "warn",
      summary: "Thiếu Số HĐĐT",
      detail: "Vui lòng nhập Số hóa đơn điện tử trước khi lưu liên kết",
    });
    return;
  }
  // Không lập phiếu mới thì bắt buộc có số phiếu xuất đã lập để sổ khớp hóa đơn.
  if (!props.invoice?.voucher_no && !makeOutbound.value && !outboundNo.value.trim()) {
    toast.add({
      severity: "warn",
      summary: "Thiếu số phiếu xuất",
      detail:
        "Hãy để app lập phiếu xuất, hoặc nhập số phiếu đã lập ở màn Xuất kho — hóa đơn không có phiếu xuất thì doanh thu không vào tờ khai thuế.",
    });
    return;
  }
  // Chốt chặn trước khi gọi backend: thiếu thành phẩm thì chưa ghi số được —
  // hiện luôn danh sách hàng thiếu kèm nút tạo lô (issue #9).
  const current = await checkStock();
  if (current && current.level !== "ok") {
    toast.add({
      severity: "warn",
      summary: "Còn thiếu tồn kho",
      detail: "Tạo lô sản xuất cho các dòng thiếu ngay trong hộp thoại này rồi bấm Lưu lại.",
      life: 7000,
    });
    return;
  }
  linking.value = true;
  try {
    const res = await api.linkHddt({
      invoiceId: props.invoice.id,
      hddtNo,
      hddtSymbol: form.hddtSymbol.trim(),
      hddtDate: toIsoDate(form.hddtDate),
    });
    const syncDetail = res.date_synced
      ? ` Ngày hóa đơn đã cập nhật theo HĐĐT: ${res.old_date} → ${res.hddt_date}.`
      : "";
    let pxNo = props.invoice?.voucher_no ?? "";
    if (!pxNo && makeOutbound.value && props.invoice?.id) {
      try {
        pxNo = await invoiceStore.createOutbound(props.invoice.id);
      } catch (e) {
        // Hóa đơn đã phát hành nhưng chưa vào sổ — báo rõ và lâu, kèm cách xử lý.
        toast.add({
          severity: "error",
          summary: "Đã ghi nhận HĐĐT nhưng chưa lập được phiếu xuất",
          detail: `${String(e)} — hóa đơn ${props.invoice?.number ?? ""} đang hiện "⚠ chưa có" ở cột Phiếu xuất. Xử lý xong bấm nút 🧾 ngay trên dòng hóa đơn để lập phiếu.`,
          life: 20_000,
        });
        visible.value = false;
        emit("done");
        return;
      }
    } else if (!pxNo) {
      pxNo = outboundNo.value.trim();
    }
    toast.add({
      severity: res.warning ? "warn" : "success",
      summary: "Đã liên kết HĐĐT",
      detail:
        `Hóa đơn ${props.invoice.number} chuyển sang trạng thái Đã phát hành.` +
        `${syncDetail}${pxNo ? ` Đã lập phiếu xuất ${pxNo}.` : ""}`,
    });
    if (res.warning) {
      toast.add({ severity: "warn", summary: "Lưu ý ngày", detail: res.warning, life: 9000 });
    }
    // Ký hiệu vừa dùng đã được lưu vào hồ sơ → nạp lại để hóa đơn sau mặc định
    // đúng ký hiệu mới (đổi mẫu số theo năm chỉ cần gõ ở đây một lần).
    await business.load();
    visible.value = false;
    emit("done");
  } catch (e) {
    toast.add({ severity: "error", summary: "Liên kết HĐĐT thất bại", detail: String(e) });
  } finally {
    linking.value = false;
  }
}
</script>

<template>
  <AppDialog
    v-model:visible="visible"
    :header="`Liên kết hóa đơn điện tử (HĐĐT) — ${invoice?.number ?? ''}`"
    width="max-w-lg"
    cancel-label="Đóng"
    action-label="Lưu"
    :saving="linking"
    :show-action="auth.canAccounting"
    @action="save"
  >
    <p class="text-sm text-gray-600">
      Hóa đơn {{ invoice?.number }} — {{ invoice?.customer }}. Sau khi lưu, chuyển sang trạng thái
      Đã liên kết HĐĐT.
    </p>

    <!-- Soi tồn ngay khi mở hộp thoại: liệt kê hàng thiếu + tạo nhanh lô sản xuất,
         giống popup sửa hóa đơn nháp (issue #9). -->
    <div v-if="checkingStock" class="mt-2 flex items-center gap-2 text-xs text-gray-500">
      <i class="pi pi-spin pi-spinner" /> Đang soi tồn kho…
    </div>
    <div
      v-else-if="scan && scan.level !== 'ok'"
      data-testid="link-shortage"
      class="mt-2 space-y-2 rounded border border-amber-300 bg-amber-50 p-3"
    >
      <div class="text-sm font-semibold text-amber-800">
        <i class="pi pi-exclamation-triangle mr-1" /> Thiếu tồn kho — chưa ghi số được
      </div>
      <div
        v-for="it in shortages"
        :key="`${it.kind}-${it.code}-${it.warehouse_code}`"
        class="flex flex-wrap items-center justify-between gap-2 rounded bg-white/70 px-2 py-1 text-sm"
      >
        <span>
          <b>{{ it.name }}</b> ({{ it.code }})
          <span class="text-xs text-gray-600">
            — cần {{ fmtDec(it.required) }}, tồn {{ fmtDec(it.available) }}, thiếu
            <b class="text-red-600">{{ fmtDec(it.required - it.available) }}</b>
            <template v-if="it.warehouse_code"> · kho {{ it.warehouse_code }}</template>
          </span>
        </span>
        <Button
          v-if="it.kind === 'product' && catalog.hasBom(it.code)"
          label="Tạo lô"
          icon="pi pi-factory"
          size="small"
          outlined
          :disabled="!auth.canStock"
          @click="openLot(it)"
        />
        <span v-else-if="it.kind === 'material'" class="text-xs text-red-600">
          Thiếu NVL — nhập thêm vật tư rồi tạo lô
        </span>
      </div>
      <p class="text-xs text-amber-700">
        Hàng có định mức: bấm <b>Tạo lô</b> để sản xuất bù ngay. Lô nhập vào đúng kho trên dòng hóa
        đơn; tạo xong danh sách này tự cập nhật.
      </p>
    </div>
    <div class="grid grid-cols-1 gap-3 py-2">
      <FormField label="Số HĐĐT" required input-id="link-hddt-no">
        <InputText
          :id="'link-hddt-no'"
          v-model="form.hddtNo"
          placeholder="Nhập số hóa đơn điện tử"
          class="w-full"
        />
      </FormField>
      <FormField label="Ký hiệu HĐĐT" input-id="link-hddt-symbol">
        <InputText
          :id="'link-hddt-symbol'"
          v-model="form.hddtSymbol"
          placeholder="Nhập ký hiệu (VD: 1C24TT152)"
          class="w-full"
        />
      </FormField>
      <div v-if="!invoice?.voucher_no" class="col-span-2 rounded border border-gray-200 p-3">
        <label class="flex items-start gap-2 text-sm">
          <Checkbox v-model="makeOutbound" binary class="mt-0.5" />
          <span>
            <b>Lập phiếu xuất cho hóa đơn này</b>
            <span class="block text-xs text-gray-500">
              Trừ tồn kho và ghi doanh thu (Nợ 131 / Có 511) — không có phiếu xuất thì doanh thu
              không vào tờ khai thuế.
            </span>
          </span>
        </label>
        <FormField v-if="!makeOutbound" label="Số phiếu xuất đã lập" required class="mt-2">
          <InputText :id="'link-px-no'" v-model="outboundNo" placeholder="PX001" class="w-full" />
        </FormField>
      </div>
      <FormField label="Ngày HĐĐT" input-id="link-hddt-date">
        <DatePicker
          :input-id="'link-hddt-date'"
          v-model="form.hddtDate"
          dateFormat="dd/mm/yy"
          class="w-full"
        />
      </FormField>
    </div>
  </AppDialog>

  <ProductionLotDialog
    v-model:visible="lotDialog"
    :product-code="lotSuggest?.code"
    :quantity="lotSuggest?.quantity"
    :warehouse-code="lotSuggest?.warehouseCode"
    @done="onLotDone"
  />
</template>
