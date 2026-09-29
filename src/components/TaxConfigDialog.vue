<script setup lang="ts">
// Cấu hình kê khai thuế của hộ kinh doanh theo quy định mới nhất:
//   - Kỳ khai thuế: theo quý / theo tháng / theo năm / theo từng lần phát sinh
//   - Phương pháp tính thuế TNCN: theo doanh thu (tỷ lệ ngành) hoặc theo lợi nhuận (15/17/20%)
// Căn cứ: NĐ 68/2026/NĐ-CP, NĐ 141/2026/NĐ-CP (ngưỡng miễn thuế 1 tỷ, áp dụng từ 01/01/2026).
import { api } from "@/db";
import { useAuthStore } from "@/stores/auth";
import { fmtVnd } from "@/utils/format";

const props = defineProps<{
  visible: boolean;
  period: string; // year | quarter | month | per_occurrence
  method: string; // revenue | profit
  /** Nhóm hộ hiện hành (1–4) — dùng gợi ý kỳ khai hợp lý. */
  group?: number | null;
}>();
const emit = defineEmits<{
  (e: "update:visible", value: boolean): void;
  (e: "saved", payload: { period: string; method: string }): void;
}>();

const auth = useAuthStore();
const toast = useToast();
const saving = ref(false);

const periodOptions = [
  { label: "Theo quý — nhóm 2, 3 (DT > 1 tỷ – 50 tỷ)", value: "quarter" },
  { label: "Theo tháng — nhóm 4 (DT > 50 tỷ)", value: "month" },
  { label: "Theo năm — nhóm 1 (DT ≤ 1 tỷ, thông báo doanh thu)", value: "year" },
  {
    label: "Theo từng lần phát sinh — chỉ dành cho THUẾ KHÁC (Điều 8 khoản 2)",
    value: "per_occurrence",
  },
];
const methodOptions = [
  { label: "Theo doanh thu × tỷ lệ ngành (không xác định được chi phí)", value: "revenue" },
  { label: "Theo lợi nhuận × 15%/17%/20% (xác định được chi phí)", value: "profit" },
];

const form = reactive({
  period: props.period,
  method: props.method,
});

/**
 * Phân bổ mức trừ ngưỡng 01 tỷ/năm cho từng nhóm ngành (NĐ 68/2026 Điều 4 khoản 3).
 *
 * Luật TNCN Điều 7 khoản 3 điểm a: doanh thu tính thuế = phần doanh thu VƯỢT trên
 * mức 01 tỷ. Khi có nhiều nhóm ngành áp mức thuế suất khác nhau, hộ được tự chọn
 * nhóm nào được trừ theo phương án có lợi nhất, tổng không vượt quá 01 tỷ/năm.
 * Để trống = app chia theo tỷ lệ doanh thu (mặc định hợp lý khi các nhóm cùng
 * tỷ lệ %), và tự bỏ trống ô nào không muốn trừ.
 */
const allocRows = ref<
  { industry_code: string; industry_name: string; revenue: number; alloc: number | null }[]
>([]);
const allocTotal = computed(() => allocRows.value.reduce((sum, r) => sum + (r.alloc ?? 0), 0));
const EXEMPT_THRESHOLD = 1_000_000_000;

/** Nạp doanh thu năm theo nhóm ngành + mức trừ đang lưu trong app_setting. */
async function loadAlloc(year: number) {
  try {
    const [rows, settings] = await Promise.all([
      api.getTaxDeclaration(year, "year", 0),
      api.getAppSettings(),
    ]);
    const saved: Record<string, number> = (() => {
      try {
        const raw = settings.pit_exempt_alloc?.trim();
        return raw ? (JSON.parse(raw) as Record<string, number>) : {};
      } catch {
        return {};
      }
    })();
    allocRows.value = rows
      .filter((r) => r.revenue_up - r.revenue_down > 0)
      .map((r) => ({
        industry_code: r.industry_code,
        industry_name: r.industry_name,
        revenue: r.revenue_up - r.revenue_down,
        alloc: saved[r.industry_code] ?? null,
      }));
  } catch {
    allocRows.value = [];
  }
}

/** Kỳ khai hợp lý theo nhóm hộ (NĐ 68/2026): 1 → năm, 2–3 → quý, 4 → tháng. */
const suggestedPeriod = computed(() => {
  switch (props.group) {
    case 1:
      return "year";
    case 4:
      return "month";
    case 2:
    case 3:
      return "quarter";
    default:
      return "";
  }
});
const suggestedLabel = computed(
  () => periodOptions.find((o) => o.value === suggestedPeriod.value)?.label ?? "",
);

watch(
  () => props.visible,
  (open) => {
    if (!open) return;
    form.method = props.method;
    // Mở hộp thoại thì chọn sẵn kỳ hợp lý theo nhóm hộ; người dùng vẫn đổi
    // được, và chỉ ghi lại khi bấm "Lưu cấu hình".
    form.period = suggestedPeriod.value || props.period;
    void loadAlloc(new Date().getFullYear());
  },
);

async function save() {
  if (allocTotal.value > EXEMPT_THRESHOLD) {
    toast.add({
      severity: "warn",
      summary: "Tổng mức trừ vượt ngưỡng",
      detail: `Tổng ${fmtVnd(allocTotal.value)} đ > ${fmtVnd(EXEMPT_THRESHOLD)} đ/năm — phần vượt sẽ không được tính trừ.`,
    });
  }
  saving.value = true;
  try {
    await api.saveAppSettings({
      tax_period: form.period,
      tax_method: form.method,
      // Đồng bộ công tắc "Khấu trừ GTGT đầu vào": theo lợi nhuận → được khấu trừ
      // (mặc định BẬT), theo doanh thu → không khấu trừ (mặc định TẮT).
      vat_deduct: form.method === "profit" ? "1" : "0",
      // Mức trừ ngưỡng theo nhóm ngành (bỏ ô trống = không trừ nhóm đó).
      pit_exempt_alloc: JSON.stringify(
        Object.fromEntries(
          allocRows.value
            .filter((r) => r.alloc && r.alloc > 0)
            .map((r) => [r.industry_code, r.alloc as number]),
        ),
      ),
    });
    toast.add({
      severity: "success",
      summary: "Đã lưu cấu hình thuế",
      detail: "Áp dụng ngay cho tờ khai và tổng hợp thuế",
    });
    emit("update:visible", false);
    emit("saved", { period: form.period, method: form.method });
  } catch (e) {
    toast.add({ severity: "error", summary: "Lỗi", detail: String(e) });
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <AppDialog
    :visible="visible"
    header="Cấu hình kê khai thuế"
    width="max-w-3xl"
    action-label="Lưu cấu hình"
    :saving="saving"
    :show-action="auth.canAccounting"
    @update:visible="emit('update:visible', $event)"
    @action="save"
  >
    <div class="space-y-4 py-2">
      <FormField label="Kỳ khai thuế của hộ" required>
        <div data-testid="tax-period" class="w-full">
          <Select
            v-model="form.period"
            :options="periodOptions"
            optionLabel="label"
            optionValue="value"
            class="w-full"
            aria-label="Kỳ khai thuế của hộ"
          />
        </div>
        <p v-if="suggestedLabel" class="mt-1 text-xs text-gray-500">
          Gợi ý cho <b>Nhóm {{ group }}</b> của hộ: {{ suggestedLabel }}.
        </p>
      </FormField>
      <FormField label="Phương pháp tính thuế TNCN" required>
        <div data-testid="tax-method" class="w-full">
          <Select
            v-model="form.method"
            :options="methodOptions"
            optionLabel="label"
            optionValue="value"
            class="w-full"
            aria-label="Phương pháp tính thuế TNCN"
          />
        </div>
      </FormField>

      <!-- Phân bổ mức trừ ngưỡng 01 tỷ cho từng nhóm ngành (Điều 4 khoản 3) -->
      <FormField label="Phân bổ mức trừ ngưỡng 01 tỷ/năm theo nhóm ngành">
        <p class="mb-2 text-xs text-gray-500">
          Luật TNCN Điều 7 khoản 3 điểm a: doanh thu tính thuế là
          <b>phần doanh thu vượt trên</b> mức 01 tỷ. Khi có nhiều nhóm ngành áp mức thuế suất khác
          nhau, hộ được tự chọn nhóm nào được trừ theo phương án có lợi nhất — tổng mức trừ không
          vượt quá {{ fmtVnd(EXEMPT_THRESHOLD) }} đ/năm. Để trống = app chia theo tỷ lệ doanh thu.
        </p>
        <div v-if="allocRows.length" class="rounded border border-gray-200">
          <div
            class="grid grid-cols-[1fr_9rem_11rem] gap-2 border-b bg-surface-50 px-2 py-1 text-xs font-medium text-gray-500"
          >
            <span>Nhóm ngành nghề</span>
            <span class="text-right">Doanh thu năm</span>
            <span class="text-right">Mức trừ chọn</span>
          </div>
          <div
            v-for="r in allocRows"
            :key="r.industry_code"
            class="grid grid-cols-[1fr_9rem_11rem] items-center gap-2 border-b px-2 py-1 text-sm last:border-b-0"
          >
            <span>{{ r.industry_name }}</span>
            <span class="text-right text-gray-600">{{ fmtVnd(r.revenue) }}</span>
            <InputNumber
              v-model="r.alloc"
              :min="0"
              :max="EXEMPT_THRESHOLD"
              :max-fraction-digits="0"
              placeholder="Chia theo tỷ lệ"
              show-buttons
              button-layout="horizontal"
              class="w-full"
            />
          </div>
          <div
            class="flex justify-between bg-surface-50 px-2 py-1 text-xs font-medium"
            :class="allocTotal > EXEMPT_THRESHOLD ? 'text-rose-600' : 'text-gray-600'"
          >
            <span>Tổng mức trừ</span>
            <span>{{ fmtVnd(allocTotal) }} / {{ fmtVnd(EXEMPT_THRESHOLD) }} đ</span>
          </div>
        </div>
        <p v-else class="text-xs text-gray-400">Chưa có doanh thu trong năm để phân bổ.</p>
      </FormField>

      <div class="rounded-lg bg-surface-50 p-3 text-xs text-gray-600 space-y-1">
        <p class="font-semibold text-gray-700">
          Các nhóm hộ kinh doanh từ 01/01/2026 (NĐ 68/2026/NĐ-CP + NĐ 141/2026/NĐ-CP):
        </p>
        <p>
          • <b>Nhóm 1</b> — doanh thu ≤ 1 tỷ/năm:
          <b class="text-emerald-600">miễn thuế GTGT + TNCN</b>, chỉ thông báo doanh thu năm (hạn
          31/01 năm sau).
        </p>
        <p>
          • <b>Nhóm 2</b> — doanh thu &gt; 1 tỷ – 3 tỷ: thuế GTGT = doanh thu × tỷ lệ ngành
          (1%/2%/3%/5%); thuế TNCN = doanh thu × tỷ lệ ngành (0,5%–5%) <i>hoặc</i> lợi nhuận × 15%.
        </p>
        <p>
          • <b>Nhóm 3</b> — doanh thu &gt; 3 tỷ – 50 tỷ: GTGT theo tỷ lệ ngành; TNCN = lợi nhuận ×
          <b>17%</b>.
        </p>
        <p>
          • <b>Nhóm 4</b> — doanh thu &gt; 50 tỷ: GTGT theo tỷ lệ ngành; TNCN = lợi nhuận ×
          <b>20%</b>, kỳ khai theo tháng.
        </p>
        <p>
          • <b>Khấu trừ thuế GTGT đầu vào</b>: hộ theo <b>lợi nhuận</b> được khấu trừ GTGT đầu vào
          (nhập kho tách TK 133, giá nhập chưa thuế); hộ theo <b>doanh thu</b> không được khấu trừ —
          giá nhập gồm thuế. Công tắc nằm ở màn cấu hình hộ kinh doanh (mặc định tắt).
        </p>
        <p class="pt-1 text-gray-400">
          Ứng dụng tự xếp nhóm hộ theo tổng doanh thu cả năm tính từ dữ liệu kế toán. Nhóm 3/4 luôn
          tính TNCN theo lợi nhuận bất kể lựa chọn bên trên.
        </p>
      </div>
    </div>
  </AppDialog>
</template>
