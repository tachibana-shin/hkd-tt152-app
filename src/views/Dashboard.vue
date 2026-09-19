<script setup lang="ts">
import { ref, onMounted } from "vue";
import type { RevenueExpenseRow } from "@/types";
import { api } from "@/db";
import { useAuthStore } from "@/stores/auth";
import { fmtVnd as fmt } from "@/utils/format";

const auth = useAuthStore();
const businessName = ref("...");
const revExp = ref<RevenueExpenseRow | null>(null);

onMounted(async () => {
  businessName.value = await api.getBusinessInfo();
  revExp.value = await api.getRevenueExpense("2026-01-01", "2026-12-31");
});

const revenueNet = () => (revExp.value?.revenue_up ?? 0) - (revExp.value?.revenue_down ?? 0);
const expenseNet = () => (revExp.value?.expense_up ?? 0) - (revExp.value?.expense_down ?? 0);
const profitLoss = () => revenueNet() - expenseNet();
</script>

<template>
  <div class="space-y-6">
    <!-- Welcome card -->
    <div class="bg-white rounded-xl shadow-sm border border-gray-200 p-6">
      <h3 class="text-lg font-semibold text-gray-800 mb-2 flex items-center gap-2">
        <i class="pi pi-chart-pie text-blue-500"></i> Tổng quan
      </h3>
      <p class="text-gray-500">
        Xin chào, <span class="font-medium text-gray-700">{{ auth.currentUser?.display_name || "bạn" }}</span>
      </p>
      <p class="text-gray-500 mt-1 flex items-center gap-1.5">
        <i class="pi pi-shop text-gray-400"></i>
        HKD: <span class="font-medium text-gray-700">{{ businessName }}</span>
      </p>
    </div>

    <!-- Summary cards -->
    <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
      <div class="bg-white rounded-xl shadow-sm border border-gray-200 p-5">
        <p class="text-sm text-gray-500 mb-1 flex items-center gap-1.5">
          <i class="pi pi-arrow-up-right text-green-600"></i> Doanh thu ròng
        </p>
        <p class="text-2xl font-bold text-green-600">{{ fmt(revenueNet()) }}</p>
      </div>
      <div class="bg-white rounded-xl shadow-sm border border-gray-200 p-5">
        <p class="text-sm text-gray-500 mb-1 flex items-center gap-1.5">
          <i class="pi pi-arrow-down-right text-orange-500"></i> Chi phí ròng
        </p>
        <p class="text-2xl font-bold text-orange-500">{{ fmt(expenseNet()) }}</p>
      </div>
      <div class="bg-white rounded-xl shadow-sm border border-gray-200 p-5">
        <p class="text-sm text-gray-500 mb-1 flex items-center gap-1.5">
          <i class="pi pi-chart-line" :class="profitLoss() >= 0 ? 'text-green-600' : 'text-red-600'"></i>
          Lãi / Lỗ
        </p>
        <p class="text-2xl font-bold" :class="profitLoss() >= 0 ? 'text-green-600' : 'text-red-600'">
          {{ fmt(profitLoss()) }}
        </p>
      </div>
    </div>

    <!-- Quick links -->
    <div class="grid grid-cols-2 md:grid-cols-4 gap-3">
      <router-link to="/inbound" class="bg-white rounded-xl shadow-sm border border-gray-200 p-4 hover:shadow-md transition text-center">
        <i class="pi pi-arrow-down text-2xl text-blue-500"></i>
        <p class="mt-2 text-sm font-medium">Nhập kho</p>
      </router-link>
      <router-link to="/outbound" class="bg-white rounded-xl shadow-sm border border-gray-200 p-4 hover:shadow-md transition text-center">
        <i class="pi pi-arrow-up text-2xl text-orange-500"></i>
        <p class="mt-2 text-sm font-medium">Xuất kho</p>
      </router-link>
      <router-link to="/invoices" class="bg-white rounded-xl shadow-sm border border-gray-200 p-4 hover:shadow-md transition text-center">
        <i class="pi pi-file text-2xl text-purple-500"></i>
        <p class="mt-2 text-sm font-medium">Hóa đơn</p>
      </router-link>
      <router-link to="/inventory" class="bg-white rounded-xl shadow-sm border border-gray-200 p-4 hover:shadow-md transition text-center">
        <i class="pi pi-inbox text-2xl text-teal-500"></i>
        <p class="mt-2 text-sm font-medium">Tồn kho</p>
      </router-link>
    </div>
  </div>
</template>
