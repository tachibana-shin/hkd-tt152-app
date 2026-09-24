<script setup lang="ts">
import { api } from "@/db";
import type { AttendanceWorkDay, Employee, PayrollRow } from "@/types";
import { useAuthStore } from "@/stores/auth";
import { fmtInt as fmt, fmtVnd } from "@/utils/format";

const auth = useAuthStore();
const toast = useToast();

// ─── Danh sách nhân viên ───
const employees = ref<Employee[]>([]);
const empLoading = ref(false);
const empDialog = ref(false);
const empEditing = ref<Employee | null>(null);

async function loadEmployees() {
  empLoading.value = true;
  try {
    employees.value = await api.getEmployees();
  } catch (e) {
    toast.add({
      severity: "error",
      summary: "Không tải được nhân viên",
      detail: String(e),
    });
  } finally {
    empLoading.value = false;
  }
}

function openEmpCreate() {
  empEditing.value = null;
  empDialog.value = true;
}

function openEmpEdit(row: Employee) {
  empEditing.value = row;
  empDialog.value = true;
}

// ─── Bảng lương tháng ───
const now = new Date();
const curMonth = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}`;
const periods = ref<string[]>([]);
const selectedPeriod = ref(curMonth);
const payrollRows = ref<PayrollRow[]>([]);
const payrollLoading = ref(false);
const savingPayroll = ref(false);
const attLoading = ref(false);

async function loadPeriods() {
  try {
    periods.value = await api.getPayrollPeriods();
  } catch (e) {
    toast.add({
      severity: "error",
      summary: "Không tải được kỳ lương",
      detail: String(e),
    });
  }
}

async function buildPayroll() {
  if (!selectedPeriod.value) {
    toast.add({
      severity: "warn",
      summary: "Chưa chọn kỳ",
      detail: "Chọn kỳ lương trước khi lập bảng",
    });
    return;
  }
  payrollLoading.value = true;
  try {
    // Kỳ đã có dữ liệu → tải bảng lương đã lưu thay vì sinh mới
    if (periods.value.includes(selectedPeriod.value)) {
      payrollRows.value = await api.getPayroll(selectedPeriod.value);
      return;
    }
    const active = employees.value.filter((e) => !e.left_on);
    if (!active.length) {
      toast.add({
        severity: "warn",
        summary: "Chưa có nhân viên",
        detail: "Thêm nhân viên trước khi lập bảng lương",
      });
      payrollRows.value = [];
      return;
    }
    payrollRows.value = active.map((e) => ({
      period: selectedPeriod.value,
      employee_code: e.code,
      employee_name: e.name,
      department: e.department,
      work_days: 26,
      bonus: 0,
      advance: 0,
      pit_amount: 0,
      gross_salary: 0,
      bh_employer: 0,
      bh_employee: 0,
      net_pay: 0,
      note: "",
    }));
  } catch (e) {
    toast.add({
      severity: "error",
      summary: "Không lập được bảng lương",
      detail: String(e),
    });
  } finally {
    payrollLoading.value = false;
  }
}

async function savePayrollTable() {
  if (!selectedPeriod.value) {
    toast.add({
      severity: "warn",
      summary: "Chưa chọn kỳ",
      detail: "Chọn kỳ lương trước khi lưu",
    });
    return;
  }
  if (!payrollRows.value.length) {
    toast.add({
      severity: "warn",
      summary: "Bảng lương trống",
      detail: "Nhấn 'Lập bảng lương' trước khi lưu",
    });
    return;
  }
  savingPayroll.value = true;
  try {
    const res = await api.savePayroll(
      selectedPeriod.value,
      payrollRows.value.map((r) => ({
        employee_code: r.employee_code,
        work_days: r.work_days,
        bonus: r.bonus,
        advance: r.advance,
        pit_amount: r.pit_amount,
        note: r.note,
      })),
    );
    let detail = res;
    try {
      const j = JSON.parse(res) as { rows?: number; total_net?: number };
      if (j.total_net !== undefined)
        detail = `${j.rows ?? 0} NV • Tổng thực lĩnh ${fmt(j.total_net)} đ`;
    } catch {
      /* res không phải JSON — dùng nguyên văn làm detail */
    }
    toast.add({ severity: "success", summary: "Đã lưu bảng lương", detail });
    payrollRows.value = await api.getPayroll(selectedPeriod.value);
    await loadPeriods();
  } catch (e) {
    toast.add({
      severity: "error",
      summary: "Không lưu được bảng lương",
      detail: String(e),
    });
  } finally {
    savingPayroll.value = false;
  }
}

// Nạp số công đã chấm theo ngày (màn Chấm công) vào bảng lương
async function useAttendanceWorkDays() {
  if (!selectedPeriod.value) {
    toast.add({
      severity: "warn",
      summary: "Chưa chọn kỳ",
      detail: "Chọn kỳ lương trước khi nạp công",
    });
    return;
  }
  if (!payrollRows.value.length) {
    toast.add({
      severity: "warn",
      summary: "Bảng lương trống",
      detail: "Hãy 'Lập bảng lương' trước rồi nạp công.",
    });
    return;
  }
  attLoading.value = true;
  try {
    const workDays: AttendanceWorkDay[] = await api.getAttendanceWorkDays(selectedPeriod.value);
    // Kỳ chưa có Ô nào được chấm (bảng chấm công còn trống) → báo để sang
    // màn Chấm công nhập. Lưu ý: backend giờ trả cả người 0 công, nên không
    // dùng workDays.length để nhận biết "chưa có dữ liệu".
    const att = await api.getAttendance(selectedPeriod.value);
    if (!att.length) {
      toast.add({
        severity: "info",
        summary: "Chưa có dữ liệu chấm công",
        detail: "Chưa có dữ liệu chấm công cho kỳ này — sang màn 'Chấm công' để nhập.",
      });
      return;
    }
    const byCode = new Map(workDays.map((w) => [w.employee_code, w.work_days]));
    let applied = 0;
    for (const row of payrollRows.value) {
      const days = byCode.get(row.employee_code);
      if (days !== undefined) {
        row.work_days = days;
        applied += 1;
      }
    }
    toast.add({
      severity: "success",
      summary: "Đã nạp công",
      detail: `Đã nạp công cho ${applied} nhân viên (chấm công kỳ ${selectedPeriod.value})`,
    });
  } catch (e) {
    toast.add({
      severity: "error",
      summary: "Không nạp được công từ chấm công",
      detail: String(e),
    });
  } finally {
    attLoading.value = false;
  }
}

loadEmployees();
loadPeriods();
</script>

<template>
  <div class="space-y-4">
    <Toolbar>
      <template #start>
        <div class="flex items-center gap-2">
          <i-mdi-cash-multiple class="text-xl text-primary-500" />
          <h3 class="font-semibold">Nhân sự & Bảng lương</h3>
        </div>
      </template>
    </Toolbar>

    <!-- Danh sách nhân viên (sheet Nhan Vien) -->
    <SectionCard>
      <template #icon><i class="pi pi-users text-primary-500" /></template>
      <template #title>
        <span>Danh sách nhân viên</span>
        <span class="text-xs text-gray-400">({{ employees.length }})</span>
      </template>
      <template #actions>
        <Button
          v-if="auth.canAccounting"
          label="Thêm nhân viên"
          icon="pi pi-plus"
          size="small"
          @click="openEmpCreate"
        />
      </template>
      <AppDataTable
        :value="employees"
        :loading="empLoading"
        stripedRows
        paginator
        :rows="10"
        actions-width="70"
      >
        <Column field="code" header="Mã" />
        <Column field="name" header="Họ và tên" />
        <Column field="position" header="Chức vụ">
          <template #body="{ data }">{{ data.position || "—" }}</template>
        </Column>
        <Column field="department" header="Bộ phận">
          <template #body="{ data }">{{ data.department || "—" }}</template>
        </Column>
        <Column field="basic_salary" header="Lương HĐ" align="right">
          <template #body="{ data }">{{ fmtVnd(data.basic_salary) }}</template>
        </Column>
        <Column field="allowance_cv" header="PC CV" align="right">
          <template #body="{ data }">{{ fmtVnd(data.allowance_cv) }}</template>
        </Column>
        <Column field="allowance_xx" header="PC XX" align="right">
          <template #body="{ data }">{{ fmtVnd(data.allowance_xx) }}</template>
        </Column>
        <Column field="allowance_phone" header="PC ĐT" align="right">
          <template #body="{ data }">{{ fmtVnd(data.allowance_phone) }}</template>
        </Column>
        <Column field="bh_salary" header="Lương đóng BH" align="right">
          <template #body="{ data }">{{ fmtVnd(data.bh_salary) }}</template>
        </Column>
        <template #actions="{ data }">
          <Button
            v-if="auth.canAccounting"
            icon="pi pi-pencil"
            text
            rounded
            size="small"
            @click="openEmpEdit(data)"
          />
        </template>
        <template #empty><EmptyState text="Chưa có nhân viên." icon="pi pi-users" /></template>
      </AppDataTable>
    </SectionCard>

    <!-- Bảng lương tháng (sheet Bang Luong) -->
    <SectionCard
      title="Bảng lương tháng"
      tooltip="Chi lương tự ghi phiếu chi (Nợ 642/Có 111). Chấm công ở màn Chấm công rồi bấm Lấy công từ chấm công."
    >
      <template #icon><i class="pi pi-money-bill text-primary-500" /></template>
      <template #actions>
        <Tag
          v-if="selectedPeriod && periods.includes(selectedPeriod)"
          value="Kỳ đã lập"
          severity="success"
        />
      </template>
      <div class="flex flex-wrap items-end gap-3">
        <div>
          <label class="mb-1 block text-xs text-gray-500">Kỳ lương</label>
          <Select
            v-model="selectedPeriod"
            :options="periods"
            placeholder="Chọn kỳ lương (YYYY-MM)"
            editable
            class="w-44"
          />
        </div>
        <Button
          v-if="auth.canAccounting"
          label="Lập bảng lương"
          icon="pi pi-file-edit"
          :loading="payrollLoading"
          @click="buildPayroll"
        />
        <Button
          v-if="auth.canAccounting"
          label="Lấy công từ chấm công"
          icon="pi pi-user-check"
          severity="secondary"
          :loading="attLoading"
          :disabled="!payrollRows.length || payrollLoading"
          @click="useAttendanceWorkDays"
        />
        <Button
          v-if="auth.canAccounting"
          label="Lưu bảng lương"
          icon="pi pi-save"
          severity="success"
          :loading="savingPayroll"
          :disabled="!payrollRows.length || payrollLoading"
          @click="savePayrollTable"
        />
        <span v-if="periods.includes(selectedPeriod)" class="text-xs text-gray-400">
          Kỳ này đã có bảng lương.
        </span>
      </div>

      <PayrollTable
        :rows="payrollRows"
        :employees="employees"
        :loading="payrollLoading"
        :can-edit="auth.canAccounting"
        :has-periods="periods.length > 0"
      />
    </SectionCard>

    <!-- Dialog nhân viên -->
    <EmployeeDialog
      v-model:visible="empDialog"
      :employee="empEditing"
      :can-edit="auth.canAccounting"
      @saved="loadEmployees"
    />
  </div>
</template>
