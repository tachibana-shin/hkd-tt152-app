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
 * Ngưỡng doanh thu năm dùng làm mức trừ TNCN — chỉ để hiển thị cho người dùng đọc
 * trước khi bấm lưu; số tiền thực tế luôn lấy từ Cài đặt (một nguồn duy nhất).
 *
 * Trước đây hộp thoại này có bảng phân bổ mức trừ theo từng nhóm ngành. Đã bỏ:
 * mức trừ vốn là số tiền **một lần trong năm** và được khấu dần theo doanh thu
 * lũy kế các kỳ trước + kỳ này, không phải chọn từng nhóm ngành. Chia theo tỷ lệ
 * doanh thu (app tự làm) là cách duy nhất, và tổng các kỳ luôn bằng đúng một mức
 * trừ — không thể "vượt" hay "thiếu" như bảng nhập tay.
 */
const exemptThreshold = ref(1_000_000_000);

/** Nạp ngưỡng đang áp dụng từ Cài đặt. */
async function loadThreshold() {
  try {
    exemptThreshold.value = (await api.getAppSettings()).tax_threshold_exempt;
  } catch {
    exemptThreshold.value = 1_000_000_000;
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
    void loadThreshold();
  },
);

async function save() {
  saving.value = true;
  try {
    await api.saveAppSettings({
      tax_period: form.period,
      tax_method: form.method,
      // Đồng bộ công tắc "Khấu trừ GTGT đầu vào": theo lợi nhuận → được khấu trừ
      // (mặc định BẬT), theo doanh thu → không khấu trừ (mặc định TẮT).
      vat_deduct: form.method === "profit" ? "1" : "0",
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

      <!-- Mức trừ TNCN: app tự khấu dần theo doanh thu lũy kế, không cần chọn tay -->
      <div
        class="rounded-lg border border-gray-200 bg-surface-50 p-3 text-xs text-gray-600 space-y-1"
      >
        <p class="font-semibold text-gray-700">
          Mức trừ doanh thu cho thuế TNCN: {{ fmtVnd(exemptThreshold) }} đ/năm
        </p>
        <p>
          Luật TNCN Điều 7 khoản 3 điểm a: <b>doanh thu tính thuế = phần doanh thu vượt mức trừ</b>
          {{ fmtVnd(exemptThreshold) }}. App tự khấu dần: mỗi kỳ trừ tiếp phần mức trừ chưa dùng của
          các kỳ trước (theo tỷ lệ doanh thu của các nhóm ngành), nên tổng cả năm luôn bằng đúng một
          mức trừ và không bao giờ trừ quá doanh thu thật được.
        </p>
        <p>
          Số còn lại của mức trừ và số đã trừ trong kỳ hiển thị ở màn
          <b>Kế toán HKD → Tờ khai thuế</b>. Muốn đổi mức trừ: màn <b>Cài đặt</b>.
        </p>
      </div>

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
