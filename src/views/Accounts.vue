<script setup lang="ts">
import { api } from "@/db";
import type { Account } from "@/types";
import { fmtInt as fmt } from "@/utils/format";

const toast = useToast();

const loading = ref(false);
const accounts = ref<Account[]>([]);

const totalDebit = computed(() =>
  accounts.value.reduce((s, a) => s + a.opening_debit, 0),
);
const totalCredit = computed(() =>
  accounts.value.reduce((s, a) => s + a.opening_credit, 0),
);

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

onMounted(load);
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
              'Khởi tạo theo bộ mẫu Excel HKD (sheet DMTK); số dư đầu kỳ từ sheet So Du.'
            "
          />
        </div>
      </template>
      <template #end>
        <Button
          label="Tải lại"
          icon="pi pi-refresh"
          outlined
          :loading="loading"
          @click="load"
        />
      </template>
    </Toolbar>

    <Card>
      <template #content>
        <AppDataTable :value="accounts" :loading="loading" stripedRows>
          <Column field="code" header="Mã TK" />
          <Column field="name" header="Tên tài khoản" />
          <Column field="opening_debit" header="Dư Nợ đầu kỳ" align="right">
            <template #body="{ data }">{{ fmt(data.opening_debit) }}</template>
          </Column>
          <Column field="opening_credit" header="Dư Có đầu kỳ" align="right">
            <template #body="{ data }">{{ fmt(data.opening_credit) }}</template>
          </Column>
          <template #empty
            ><EmptyState text="Chưa có tài khoản." icon="pi pi-book"
          /></template>
        </AppDataTable>
        <div
          v-if="accounts.length"
          class="mt-4 flex items-center justify-end gap-6 text-sm border-t pt-3"
        >
          <span>
            <span class="text-gray-500">Tổng Dư Nợ:</span>
            <b class="text-primary-600 ml-1">{{ fmt(totalDebit) }}</b>
          </span>
          <span class="text-gray-400">•</span>
          <span>
            <span class="text-gray-500">Tổng Dư Có:</span>
            <b class="text-primary-600 ml-1">{{ fmt(totalCredit) }}</b>
          </span>
        </div>
      </template>
    </Card>
  </div>
</template>
