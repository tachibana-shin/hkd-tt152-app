<script setup lang="ts">
import { api } from "@/db";
import type { Employee } from "@/types";

const props = defineProps<{
  visible: boolean;
  employee: Employee | null;
  canEdit: boolean;
}>();

const emit = defineEmits<{
  (e: "update:visible", value: boolean): void;
  (e: "saved"): void;
}>();

const toast = useToast();

const iso = (d: Date | null) =>
  d
    ? `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`
    : "";
const parseDate = (s: string) => (s ? new Date(`${s}T00:00:00`) : null);

const saving = ref(false);
const form = reactive({
  id: null as number | null,
  code: "",
  name: "",
  position: "",
  job: "",
  department: "",
  basic_salary: 0,
  allowance_cv: 0,
  allowance_xx: 0,
  allowance_phone: 0,
  bh_salary: 0,
  hired_on: null as Date | null,
  left_on: null as Date | null,
  dependents: 0,
});

function fill() {
  const e = props.employee;
  Object.assign(form, {
    id: e?.id ?? null,
    code: e?.code ?? "",
    name: e?.name ?? "",
    position: e?.position ?? "",
    job: e?.job ?? "",
    department: e?.department ?? "",
    basic_salary: e?.basic_salary ?? 0,
    allowance_cv: e?.allowance_cv ?? 0,
    allowance_xx: e?.allowance_xx ?? 0,
    allowance_phone: e?.allowance_phone ?? 0,
    bh_salary: e?.bh_salary ?? 0,
    hired_on: parseDate(e?.hired_on ?? ""),
    left_on: parseDate(e?.left_on ?? ""),
    dependents: e?.dependents ?? 0,
  });
}

watch(
  () => props.visible,
  async (open) => {
    if (!open) return;
    fill();
    // Thêm mới: tự sinh mã NV tiếp theo (NV001, NV002...) — sửa thì giữ nguyên mã.
    if (props.employee == null) {
      try {
        form.code = await api.nextEmployeeCode();
      } catch {
        // Lỗi thì để trống nhập tay
      }
    }
  },
);

async function save() {
  if (!form.code.trim() || !form.name.trim()) {
    toast.add({
      severity: "warn",
      summary: "Thiếu thông tin",
      detail: "Mã và họ tên nhân viên là bắt buộc",
    });
    return;
  }
  saving.value = true;
  try {
    await api.saveEmployee({
      code: form.code.trim(),
      name: form.name.trim(),
      position: form.position.trim(),
      job: form.job.trim(),
      department: form.department.trim(),
      basic_salary: form.basic_salary,
      allowance_cv: form.allowance_cv,
      allowance_xx: form.allowance_xx,
      allowance_phone: form.allowance_phone,
      bh_salary: form.bh_salary,
      hired_on: iso(form.hired_on),
      left_on: iso(form.left_on),
      dependents: form.dependents,
    });
    toast.add({ severity: "success", summary: "Đã lưu", detail: `Nhân viên ${form.code}` });
    emit("update:visible", false);
    emit("saved");
  } catch (e) {
    toast.add({ severity: "error", summary: "Không lưu được", detail: String(e) });
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <AppDialog
    :visible="visible"
    :header="employee ? 'Sửa nhân viên' : 'Thêm nhân viên'"
    width="max-w-3xl"
    action-label="Lưu"
    :saving="saving"
    :show-action="canEdit"
    @update:visible="emit('update:visible', $event)"
    @action="save"
  >
    <div class="grid grid-cols-3 gap-4 py-2">
      <FormField label="Mã NV" required>
        <InputText v-model="form.code" placeholder="NV001" :disabled="form.id !== null" />
      </FormField>
      <FormField label="Họ và tên" required class="col-span-2">
        <InputText v-model="form.name" placeholder="Họ và tên nhân viên" />
      </FormField>
      <FormField label="Chức vụ">
        <InputText v-model="form.position" placeholder="Nhân viên" />
      </FormField>
      <FormField label="Công việc">
        <InputText v-model="form.job" placeholder="Công việc chính" />
      </FormField>
      <FormField label="Bộ phận">
        <InputText v-model="form.department" placeholder="Bộ phận" />
      </FormField>
      <FormField label="Lương theo HĐ">
        <InputNumber
          v-model="form.basic_salary"
          :min="0"
          mode="currency"
          currency="VND"
          locale="vi-VN"
          class="w-full"
        />
      </FormField>
      <FormField label="PC CV">
        <InputNumber
          v-model="form.allowance_cv"
          :min="0"
          mode="currency"
          currency="VND"
          locale="vi-VN"
          class="w-full"
        />
      </FormField>
      <FormField label="PC XX">
        <InputNumber
          v-model="form.allowance_xx"
          :min="0"
          mode="currency"
          currency="VND"
          locale="vi-VN"
          class="w-full"
        />
      </FormField>
      <FormField label="PC ĐT">
        <InputNumber
          v-model="form.allowance_phone"
          :min="0"
          mode="currency"
          currency="VND"
          locale="vi-VN"
          class="w-full"
        />
      </FormField>
      <FormField label="Lương đóng BH">
        <InputNumber
          v-model="form.bh_salary"
          :min="0"
          mode="currency"
          currency="VND"
          locale="vi-VN"
          class="w-full"
        />
      </FormField>
      <FormField label="Số người phụ thuộc">
        <InputNumber v-model="form.dependents" :min="0" :maxFractionDigits="0" class="w-full" />
      </FormField>
      <FormField label="Ngày vào">
        <DatePicker v-model="form.hired_on" dateFormat="dd/mm/yy" showClear class="w-full" />
      </FormField>
      <FormField label="Ngày nghỉ">
        <DatePicker v-model="form.left_on" dateFormat="dd/mm/yy" showClear class="w-full" />
      </FormField>
    </div>
  </AppDialog>
</template>
