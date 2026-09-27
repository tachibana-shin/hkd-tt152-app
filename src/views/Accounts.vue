<script setup lang="ts">
import { api } from "@/db";
import { useAuthStore } from "@/stores/auth";
import type { Account } from "@/types";
import { fmtVnd } from "@/utils/format";
import { useKeepAliveRefresh } from "@/composables/useKeepAliveRefresh";

const auth = useAuthStore();
const toast = useToast();
const confirm = useConfirm();

const loading = ref(false);
const accounts = ref<Account[]>([]);

const dialog = ref(false);
const editing = ref(false);
const saving = ref(false);
const form = reactive({
  id: undefined as number | undefined,
  code: "",
  name: "",
  opening_debit: 0,
  opening_credit: 0,
});

const totalDebit = computed(() => accounts.value.reduce((s, a) => s + a.opening_debit, 0));
const totalCredit = computed(() => accounts.value.reduce((s, a) => s + a.opening_credit, 0));

async function load() {
  loading.value = true;
  try {
    accounts.value = await api.getAccounts();
  } catch (e) {
    toast.add({
      severity: "error",
      summary: "Lỗi tải danh mục tài khoản",
      detail: String(e),
    });
  } finally {
    loading.value = false;
  }
}

function openCreate() {
  Object.assign(form, {
    id: undefined,
    code: "",
    name: "",
    opening_debit: 0,
    opening_credit: 0,
  });
  editing.value = false;
  dialog.value = true;
}

function openEdit(a: Account) {
  Object.assign(form, {
    id: a.id,
    code: a.code,
    name: a.name,
    opening_debit: a.opening_debit,
    opening_credit: a.opening_credit,
  });
  editing.value = true;
  dialog.value = true;
}

async function save() {
  if (!form.code.trim()) {
    toast.add({
      severity: "warn",
      summary: "Thiếu mã tài khoản",
      detail: "Vui lòng nhập mã tài khoản.",
    });
    return;
  }
  if (!form.name.trim()) {
    toast.add({
      severity: "warn",
      summary: "Thiếu tên tài khoản",
      detail: "Vui lòng nhập tên tài khoản.",
    });
    return;
  }
  saving.value = true;
  try {
    await api.saveAccount({ ...form });
    dialog.value = false;
    toast.add({
      severity: "success",
      summary: editing.value ? "Đã sửa" : "Đã thêm",
      detail: `Tài khoản ${form.code}`,
    });
    await load();
  } catch (e) {
    toast.add({
      severity: "error",
      summary: "Không lưu được",
      detail: String(e),
    });
  } finally {
    saving.value = false;
  }
}

function remove(a: Account) {
  confirm.require({
    message: `Xóa tài khoản ${a.code} — ${a.name}?`,
    header: "Xác nhận xóa",
    icon: "pi pi-exclamation-triangle",
    acceptLabel: "Xóa",
    rejectLabel: "Hủy",
    accept: async () => {
      try {
        await api.deleteAccount(a.id);
        toast.add({
          severity: "success",
          summary: "Đã xóa",
          detail: `Tài khoản ${a.code}`,
        });
        await load();
      } catch (e) {
        toast.add({
          severity: "error",
          summary: "Không xóa được",
          detail: String(e),
        });
      }
    },
  });
}

void load();

// Quay lại màn (KeepAlive giữ state) → nạp lại dữ liệu cho khỏi cũ.
useKeepAliveRefresh(load);
</script>

<template>
  <div class="space-y-4">
    <Toolbar>
      <template #start>
        <div class="flex items-center gap-2">
          <i-mdi-bank class="text-xl text-primary-500" />
          <h3 class="font-semibold">Danh mục tài khoản (DMTK)</h3>
          <i
            class="pi pi-info-circle text-xs text-gray-400 cursor-help"
            v-tooltip="
              'Bảng mã tài khoản kế toán dùng trong hạch toán (111, 131, 511, 632...). Thêm/sửa/xóa được; số dư đầu kỳ nhập khi bắt đầu năm.'
            "
          />
        </div>
      </template>
      <template #end>
        <Button
          v-if="auth.canAccounting"
          label="Thêm tài khoản"
          icon="pi pi-plus"
          class="mr-2"
          @click="openCreate"
        />
        <Button label="Tải lại" icon="pi pi-refresh" outlined :loading="loading" @click="load" />
      </template>
    </Toolbar>

    <Card>
      <template #content>
        <AppDataTable :value="accounts" :loading="loading" stripedRows>
          <Column field="code" header="Mã TK" style="width: 120px" />
          <Column field="name" header="Tên tài khoản" />
          <Column field="opening_debit" header="Dư Nợ đầu kỳ" align="right">
            <template #body="{ data }">{{ fmtVnd(data.opening_debit) }}</template>
          </Column>
          <Column field="opening_credit" header="Dư Có đầu kỳ" align="right">
            <template #body="{ data }">{{ fmtVnd(data.opening_credit) }}</template>
          </Column>
          <Column v-if="auth.canAccounting" header="" align="center" style="width: 120px">
            <template #body="{ data }">
              <div class="flex justify-center gap-1">
                <Button
                  icon="pi pi-pencil"
                  text
                  rounded
                  size="small"
                  v-tooltip="'Sửa'"
                  @click="openEdit(data)"
                />
                <Button
                  icon="pi pi-trash"
                  text
                  rounded
                  size="small"
                  severity="danger"
                  v-tooltip="'Xóa'"
                  @click="remove(data)"
                />
              </div>
            </template>
          </Column>
          <template #empty><EmptyState text="Chưa có tài khoản." icon="pi pi-book" /></template>
        </AppDataTable>
        <div
          v-if="accounts.length"
          class="mt-4 flex items-center justify-end gap-6 text-sm border-t pt-3"
        >
          <span>
            <span class="text-gray-500">Tổng Dư Nợ:</span>
            <b class="text-primary-600 ml-1">{{ fmtVnd(totalDebit) }}</b>
          </span>
          <span class="text-gray-400">•</span>
          <span>
            <span class="text-gray-500">Tổng Dư Có:</span>
            <b class="text-primary-600 ml-1">{{ fmtVnd(totalCredit) }}</b>
          </span>
        </div>
      </template>
    </Card>

    <AppDialog
      v-model:visible="dialog"
      :header="editing ? 'Sửa tài khoản' : 'Thêm tài khoản'"
      width="max-w-xl"
      action-label="Lưu"
      :saving="saving"
      :show-action="auth.canAccounting"
      @action="save"
    >
      <div class="grid grid-cols-2 gap-4 py-2">
        <FormField label="Mã tài khoản" required>
          <InputText
            size="small"
            v-model="form.code"
            placeholder="VD: 112, 131, 511..."
            :disabled="editing"
          />
        </FormField>
        <FormField label="Tên tài khoản" required>
          <InputText size="small" v-model="form.name" placeholder="VD: Tiền gửi ngân hàng" />
        </FormField>
        <FormField label="Dư Nợ đầu kỳ">
          <InputNumber
            v-model="form.opening_debit"
            :min="0"
            mode="currency"
            currency="VND"
            locale="vi-VN"
            size="small"
            class="w-full"
          />
        </FormField>
        <FormField label="Dư Có đầu kỳ">
          <InputNumber
            v-model="form.opening_credit"
            :min="0"
            mode="currency"
            currency="VND"
            locale="vi-VN"
            size="small"
            class="w-full"
          />
        </FormField>
        <p class="col-span-2 text-xs text-gray-400">
          Mã tài khoản dùng chung với hạch toán (phiếu thu/chi, xuất kho...). Sửa mã không cho phép
          — nhập lại tài khoản mới nếu cần đổi mã.
        </p>
      </div>
    </AppDialog>
  </div>
</template>
