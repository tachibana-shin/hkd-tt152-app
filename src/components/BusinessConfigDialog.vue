<script setup lang="ts">
// Dialog cấu hình hộ kinh doanh.
import { useBusinessStore } from "@/stores/business";
import { useAuthStore } from "@/stores/auth";
import type { BusinessConfig } from "@/types";

const props = defineProps<{
  visible: boolean;
  config: BusinessConfig | null;
  /** Chế độ bắt buộc (cập nhật thông tin HKD ngay khi đăng nhập): không đóng
   *  được cho tới khi lưu thành công — ẩn nút Hủy, không có nút X, không Esc. */
  required?: boolean;
}>();
const emit = defineEmits<{
  (e: "update:visible", value: boolean): void;
  (e: "saved"): void;
}>();

const business = useBusinessStore();
const auth = useAuthStore();
const toast = useToast();
const saving = ref(false);

const form = reactive({
  name: "",
  short_name: "",
  tax_code: "",
  address: "",
  ownership: "Tư nhân",
  province: "",
  tax_code_issued_on: "",
  phone: "",
  email: "",
});

watch(
  () => props.visible,
  (open) => {
    const c = props.config;
    if (!open || !c) return;
    Object.assign(form, {
      name: c.name,
      short_name: c.short_name,
      tax_code: c.tax_code,
      address: c.address,
      ownership: c.ownership,
      province: c.province,
      tax_code_issued_on: c.tax_code_issued_on,
      phone: c.phone,
      email: c.email,
    });
  },
);

async function save() {
  // Tên và MST là bắt buộc (nhất là chế độ bắt buộc sau đăng nhập).
  if (!form.name.trim() || !form.tax_code.trim()) {
    toast.add({
      severity: "warn",
      summary: "Thiếu thông tin bắt buộc",
      detail: "Tên hộ kinh doanh và mã số thuế không được để trống",
      life: 3000,
    });
    return;
  }
  saving.value = true;
  try {
    await business.save({ ...form });
    toast.add({ severity: "success", summary: "Đã lưu thông tin hộ kinh doanh" });
    emit("update:visible", false);
    emit("saved");
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
    header="Cấu hình hộ kinh doanh"
    width="max-w-2xl"
    action-label="Lưu cấu hình"
    :saving="saving"
    :show-action="auth.isAdmin"
    :cancel-label="required ? null : undefined"
    :closable="!required"
    :close-on-escape="!required"
    @update:visible="emit('update:visible', $event)"
    @action="save"
  >
    <div class="grid grid-cols-2 gap-4 py-2">
      <FormField label="Tên hộ kinh doanh" required class="col-span-2">
        <InputText v-model="form.name" placeholder="Ví dụ: Hộ kinh doanh Nguyễn Văn A" />
      </FormField>
      <FormField label="Tên viết tắt">
        <InputText v-model="form.short_name" />
      </FormField>
      <FormField label="Mã số thuế" required>
        <InputText v-model="form.tax_code" placeholder="VD: 0123456789" />
      </FormField>
      <FormField label="Địa chỉ" class="col-span-2">
        <InputText v-model="form.address" />
      </FormField>
      <FormField label="Loại hình">
        <InputText v-model="form.ownership" />
      </FormField>
      <FormField label="Tỉnh/TP">
        <InputText v-model="form.province" />
      </FormField>
      <FormField label="Nơi cấp MST">
        <InputText v-model="form.tax_code_issued_on" />
      </FormField>
      <FormField label="Số điện thoại">
        <InputText v-model="form.phone" />
      </FormField>
      <FormField label="Email" class="col-span-2">
        <InputText v-model="form.email" />
      </FormField>
    </div>
  </AppDialog>
</template>
