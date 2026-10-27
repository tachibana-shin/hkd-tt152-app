<script setup lang="ts">
/**
 * Tạo lô thành phẩm từ định mức vật tư — tách từ màn Lô sản xuất để popup hóa
 * đơn dùng lại (tạo nhanh 1 lô khi đang thiếu hàng rồi gắn luôn vào dòng).
 *
 * 1 lô = 1 phiếu nhập kho loại `production`: chọn thành phẩm + sản lượng, app
 * hiện trước thành phần = định mức × sản lượng (kèm tồn kho của kho đang chọn).
 * Giá trị nhập kho lấy theo đơn giá KHAI trên từng dòng định mức (kể cả dịch
 * vụ); giá vốn FIFO chỉ hiện để đối chiếu. Lưu gọi `save_inbound` với
 * `inbound_type: production` + `autoBom: true` → backend chụp snapshot thành
 * phần, trừ tồn NVL và tự sinh phiếu xuất NVL (Nợ 154 / Có 152) trong CÙNG
 * transaction; thiếu tồn thì cả lô không được lưu.
 *
 * Mở dialog là tự reset (giữ `product-code` nếu người gọi chỉ định sẵn thành
 * phẩm) rồi nạp số phiếu lô + lô tồn + định mức. Lưu xong bắn `done` để màn
 * cha nạp lại (danh sách lô / tồn kho / danh mục).
 */
import { useAuthStore } from "@/stores/auth";
import { useCatalogStore } from "@/stores/catalog";
import { useStockStore } from "@/stores/stock";
import { api } from "@/db";
import { fmtDec, fmtVnd, toIsoDate } from "@/utils/format";
import type { StockLot } from "@/types";

const props = defineProps<{
  /** Thành phẩm gợi ý sẵn khi mở (popup hóa đơn gắn hàng vừa tạo xong). */
  productCode?: string;
  /**
   * Sản lượng điền sẵn — popup hóa đơn gợi ý phần còn THIẾU khi chọn hàng
   * chế tạo mà kho không đủ, người dùng vẫn sửa được trước khi tạo lô.
   */
  quantity?: number;
  /** Kho nhập điền sẵn — lấy kho xuất trên dòng hóa đơn để nhập về đúng chỗ. */
  warehouseCode?: string;
}>();
const emit = defineEmits<{
  done: [payload: { voucherNo: string; productCode: string }];
}>();
const visible = defineModel<boolean>("visible", { default: false });

const auth = useAuthStore();
const catalog = useCatalogStore();
const stockStore = useStockStore();
const { warehouses } = storeToRefs(catalog);
const toast = useToast();

const saving = ref(false);

const form = reactive({
  product_code: "",
  quantity: 1,
  posting_date: new Date(),
  warehouse_code: "",
  voucher_no: "",
  description: "Nhập kho thành phẩm — lô sản xuất",
});

// Lô tồn kho (soi tồn + tính giá thành FIFO trước khi lưu). Định mức đọc trực
// tiếp từ catalog.boms — nạp lại mỗi lần mở (xem watch dưới) để ô Thành phẩm
// chỉ nhận hàng CHẾ TẠO và bản xem trước theo định mức hiện hành.
const stockLots = ref<StockLot[]>([]);

watch(
  visible,
  async (open) => {
    if (!open) return;
    // Kho điền sẵn phải là kho có thật (dòng hóa đơn có thể để trống/rơi kho
    // đã xoá) — sai thì rơi về kho đầu tiên như cũ.
    const presetWh = warehouses.value.some((w) => w.code === props.warehouseCode)
      ? props.warehouseCode!
      : "";
    Object.assign(form, {
      product_code: props.productCode ?? "",
      quantity: props.quantity && props.quantity > 0 ? props.quantity : 1,
      posting_date: new Date(),
      warehouse_code: presetWh || warehouses.value[0]?.code || "",
      voucher_no: "",
      description: "Nhập kho thành phẩm — lô sản xuất",
    });
    try {
      // Số lô do backend sinh (PN0001…) — không đoán từ danh sách đang lọc.
      // Định mức nạp LẠI mỗi lần mở (force): ô Thành phẩm chỉ nhận hàng có định
      // mức, và định mức có thể vừa được đổi ở màn Sản phẩm.
      const [voucherNo, lots] = await Promise.all([
        api.nextVoucherNo("PN"),
        api.getStockLots(""),
        catalog.loadBoms(true),
      ]);
      form.voucher_no = voucherNo;
      stockLots.value = lots;
    } catch (e) {
      toast.add({ severity: "error", summary: "Không mở được hộp thoại", detail: String(e) });
    }
  },
  { immediate: true },
);

/** 1 dòng NVL của bản xem trước: định mức × sản lượng, tồn kho, giá vốn FIFO. */
type PreviewLine = {
  code: string;
  name: string;
  unit: string;
  /** Dịch vụ (nhân công…) — có giá trị nhưng không có tồn kho để trừ. */
  isService: boolean;
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
  /** Đơn giá khai trên dòng định mức (VND). */
  salePrice: number;
  /** Thành tiền theo giá khai = SL cần × đơn giá khai. */
  saleAmount: number;
};

const preview = computed<PreviewLine[]>(() => {
  const qty = form.quantity > 0 ? form.quantity : 0;
  return catalog.boms
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
      if (b.material_is_service) {
        return {
          code: b.material_code,
          name: b.material_name,
          unit: b.material_unit,
          isService: true,
          perUnit: b.quantity,
          need,
          onHand: 0,
          shortage: 0,
          unitCost: 0,
          cost: 0,
          salePrice: b.unit_price,
          saleAmount: Math.round(need * b.unit_price * 1e6) / 1e6,
        };
      }
      // Trừ FIFO đúng như backend khi lưu để giá vốn ước tính không lệch.
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
        isService: false,
        perUnit: b.quantity,
        need,
        onHand,
        shortage: Math.max(0, Math.round((need - onHand) * 1e6) / 1e6),
        unitCost: need > 0 ? cost / need : 0,
        cost,
        salePrice: b.unit_price,
        saleAmount: Math.round(need * b.unit_price * 1e6) / 1e6,
      };
    });
});

/** Giá vốn FIFO thực tế của NVL (dịch vụ không có tồn kho nên = 0). */
const totalCost = computed(() => preview.value.reduce((s, l) => s + l.cost, 0));
/** Σ định mức × đơn giá đã khai — giá trị lô theo bảng định mức. */
const totalDeclared = computed(() => preview.value.reduce((s, l) => s + l.saleAmount, 0));
const shortages = computed(() => preview.value.filter((l) => l.shortage > 0));
/** Phải có ít nhất 1 NVL thật (có tồn kho) mới tạo được lô. */
const hasBom = computed(() => preview.value.some((l) => !l.isService));

/**
 * Đơn giá thành phẩm ghi trên phiếu nhập = tổng giá trị theo đơn giá KHAI ÷
 * sản lượng (đúng giá trị cột "Giá trị nhập"). Chưa khai giá dòng nào thì rơi
 * về giá vốn FIFO để không làm mất giá trị lô.
 */
const unitPrice = computed(() => {
  if (!(form.quantity > 0)) return 0;
  if (totalDeclared.value > 0) return totalDeclared.value / form.quantity;
  return totalCost.value / form.quantity;
});

/** Giá trị nhập kho thực tế sẽ ghi: theo giá khai, chưa khai thì theo giá vốn FIFO. */
const lotValue = computed(() => (totalDeclared.value > 0 ? totalDeclared.value : totalCost.value));

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
    const created = { voucherNo: form.voucher_no, productCode: form.product_code };
    toast.add({
      severity: "success",
      summary: `Đã tạo lô ${created.voucherNo}`,
      detail: `Giá trị nhập kho: ${fmtVnd(res.total)}${
        res.material_voucher ? ` · Đã tạo phiếu xuất NVL ${res.material_voucher}` : ""
      }`,
    });
    visible.value = false;
    emit("done", created);
  } catch (e) {
    toast.add({ severity: "error", summary: "Không tạo được lô", detail: String(e) });
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <AppDialog
    v-model:visible="visible"
    header="Tạo lô sản xuất"
    width="max-w-5xl"
    action-label="Tạo lô"
    :saving="saving"
    :show-action="auth.canStock"
    @action="save"
  >
    <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4 py-2">
      <FormField label="Thành phẩm">
        <div data-testid="lot-fg">
          <ProductSelect v-model="form.product_code" size="small" only-manufactured />
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
        <DatePicker v-model="form.posting_date" dateFormat="dd/mm/yy" size="small" class="w-full" />
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
        <span class="text-sm font-semibold">Thành phần theo định mức</span>
        <span class="text-xs text-gray-500">
          Giá trị nhập:
          <b class="text-emerald-700">{{ fmtVnd(lotValue) }}</b>
          · Giá vốn FIFO:
          <b class="text-gray-700">{{ fmtVnd(totalCost) }}</b>
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
        <Column field="name" header="Vật tư / dịch vụ" style="min-width: 14rem">
          <template #body="{ data }">
            {{ data.name }} ({{ data.code }})
            <Tag v-if="data.isService" value="Dịch vụ" severity="warn" class="ml-1" />
          </template>
        </Column>
        <Column field="perUnit" header="Định mức/SP" align="right">
          <template #body="{ data }">{{ fmtDec(data.perUnit) }} {{ data.unit }}</template>
        </Column>
        <Column field="need" header="SL cần" align="right">
          <template #body="{ data }">{{ fmtDec(data.need) }}</template>
        </Column>
        <Column field="onHand" header="Tồn kho" align="right">
          <template #body="{ data }">
            <span
              v-if="data.isService"
              class="text-gray-400"
              v-tooltip="'Dịch vụ không theo dõi tồn kho — không trừ kho'"
              >—</span
            >
            <span v-else :class="data.shortage > 0 ? 'text-red-600' : 'text-emerald-700'">
              {{ fmtDec(data.onHand) }}
            </span>
          </template>
        </Column>
        <Column field="unitCost" header="Giá vốn FIFO" align="right">
          <template #body="{ data }">
            <span v-if="data.isService" class="text-gray-400">—</span>
            <span v-else>{{ fmtVnd(data.unitCost) }}</span>
          </template>
        </Column>
        <Column field="cost" header="Tiền vốn NVL" align="right">
          <template #body="{ data }">
            <span v-if="data.isService" class="text-gray-400">—</span>
            <b v-else>{{ fmtVnd(data.cost) }}</b>
          </template>
        </Column>
        <Column field="salePrice" header="Đơn giá khai" align="right">
          <template #body="{ data }">{{ fmtVnd(data.salePrice) }}</template>
        </Column>
        <Column field="saleAmount" header="Thành tiền khai" align="right">
          <template #body="{ data }"
            ><b>{{ fmtVnd(data.saleAmount) }}</b></template
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
          Thiếu tồn kho: {{ shortages.map((l) => l.name).join(", ") }} — nhập thêm vật tư rồi tạo lô
          lại (lưu khi thiếu tồn sẽ bị backend từ chối).
        </span>
      </Message>
      <p v-else class="mt-2 text-xs text-gray-400">
        Lưu lô: nhập thành phẩm (Nợ 155 / Có 154) theo đơn giá KHAI trên từng thành phần định mức;
        NVL thật được tự xuất theo FIFO (Nợ 154 / Có 152), dịch vụ không trừ kho. Định mức được chụp
        lại vào lô nên sửa định mức sau này không làm đổi thành phần lô đã tạo. Lô là chứng từ kho
        chính thức, độc lập với hoá đơn bán — xoá hoá đơn nháp không xoá lô.
      </p>
    </div>
  </AppDialog>
</template>
