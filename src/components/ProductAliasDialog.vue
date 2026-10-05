<script setup lang="ts">
/**
 * Thêm "tên khác" (alias) cho 1 mặt hàng — tách ra khỏi bảng dòng để popup hóa
 * đơn dùng lại: vừa mở từ icon tag trên từng dòng (truyền sẵn `product-code`),
 * vừa mở từ nút ở header bảng "Mặt hàng" (chưa biết hàng nào → có ô chọn).
 *
 * Lưu xong bắn `saved(mã hàng, danh sách tên mới)` — người gọi tự toast để nói
 * rõ tên đó có gắn được vào dòng hóa đơn (F5 in đúng tên) hay chỉ lưu danh mục.
 */
import { useCatalogStore } from "@/stores/catalog";

const props = defineProps<{
  /** Mã hàng đang thêm tên khác — bỏ trống thì dialog thêm ô chọn sản phẩm. */
  productCode?: string;
}>();
const emit = defineEmits<{ saved: [code: string, added: string[]] }>();
const visible = defineModel<boolean>("visible", { default: false });

const catalog = useCatalogStore();
const toast = useToast();

const pickedCode = ref("");
const aliasText = ref("");
const saving = ref(false);

/** Mã hàng cần gán: hàng của dòng đang mở, hoặc hàng vừa chọn ở ô dropdown. */
const code = computed(() => props.productCode || pickedCode.value);
const product = computed(() => catalog.productByCode(code.value));

const header = computed(() =>
  product.value ? `Thêm tên khác — ${product.value.name}` : "Thêm tên khác",
);

watch(visible, (open) => {
  if (!open) return;
  aliasText.value = "";
  // Gọi từ header: không kèm hàng nào → dọn lựa chọn của lần mở trước.
  if (!props.productCode) pickedCode.value = "";
});

async function save() {
  const wanted = aliasText.value
    .split(",")
    .map((a) => a.trim())
    .filter(Boolean);
  if (!code.value || !wanted.length) {
    toast.add({
      severity: "warn",
      summary: "Thiếu thông tin",
      detail: props.productCode
        ? "Gõ tên khác muốn gán cho mặt hàng này"
        : "Chọn mặt hàng và gõ tên khác muốn gán",
    });
    return;
  }
  saving.value = true;
  try {
    // Bắt số/ tên TRƯỚC khi đóng — dialog header tự dọn ô chọn khi tắt.
    const target = code.value;
    const name = product.value?.name ?? target;
    // Thêm (giữ tên cũ), nạp lại danh mục — backend bỏ tên trùng tên chính/mã.
    const added = await catalog.addProductAliases(target, wanted);
    visible.value = false;
    // Có tên mới → người gọi báo kết quả (kèm việc gắn vào dòng hay chỉ danh mục).
    if (added[0]) {
      emit("saved", target, added);
      return;
    }
    // Không thêm được gì (đều đã là tên chính / tên khác) → dialog tự báo.
    toast.add({
      severity: "warn",
      summary: "Tên này không cần thêm",
      detail: `${wanted[0]} đã là tên chính hoặc tên khác của ${name}`,
    });
  } catch (e) {
    toast.add({ severity: "error", summary: "Lỗi", detail: String(e) });
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <AppDialog
    v-model:visible="visible"
    :header="header"
    width="max-w-md"
    action-label="Lưu"
    :saving="saving"
    @action="save"
  >
    <div class="flex flex-col gap-3 py-2">
      <!-- Mở từ header bảng: chọn hàng trước rồi mới gõ tên khác. -->
      <FormField v-if="!productCode" label="Mặt hàng" required>
        <ProductSelect v-model="pickedCode" size="small" />
      </FormField>
      <p v-if="product" class="text-sm text-gray-600">
        Mặt hàng:
        <b>{{ product.name }}</b>
        <span class="text-gray-400">({{ product.code }})</span>
      </p>
      <FormField label="Tên khác" required>
        <InputText
          v-model="aliasText"
          size="small"
          placeholder="Tapioca, Bột mì"
          data-testid="alias-input"
          @keyup.enter="save"
        />
      </FormField>
      <p class="text-xs text-gray-400">
        Gõ 1 tên hoặc nhiều tên cách nhau bằng phẩy. Mỗi tên chỉ gán cho 1 sản phẩm; dòng hóa đơn
        giữ ngay tên vừa thêm.
      </p>
    </div>
  </AppDialog>
</template>
