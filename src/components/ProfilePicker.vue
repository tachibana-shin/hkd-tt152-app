<script setup lang="ts">
/**
 * Màn hình chọn hồ sơ lúc khởi động (kiểu Google Chrome profile picker):
 * hiện khi máy có NHIỀU hồ sơ HKD và chưa đăng nhập — hỏi người dùng vào hồ
 * sơ nào thay vì tự chọn hồ sơ gần nhất. Chọn hồ sơ nào thì vào hồ sơ đó;
 * hồ sơ có bật tự động đăng nhập thì vào thẳng app, không qua màn đăng nhập.
 */
import type { Profile } from "@/types";

defineProps<{
  profiles: Profile[];
  activeKey?: string;
}>();

const emit = defineEmits<{ select: [key: string] }>();

const AVATAR_COLORS = [
  "bg-sky-500",
  "bg-emerald-500",
  "bg-amber-500",
  "bg-rose-500",
  "bg-violet-500",
  "bg-teal-500",
  "bg-orange-500",
  "bg-indigo-500",
];
</script>

<template>
  <div
    class="flex h-screen items-center justify-center overflow-auto bg-gradient-to-br from-gray-900 via-gray-800 to-primary-900"
  >
    <div class="text-center px-6 py-10">
      <div class="text-white mb-8">
        <img src="/app-icon.png" alt="" class="mx-auto h-16 w-16" />
        <h1 class="text-2xl font-bold mt-3 tracking-wide">HKD Kế Toán</h1>
        <p class="text-sm text-gray-400 mt-1">Chọn hồ sơ để tiếp tục</p>
      </div>

      <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-5 justify-items-center">
        <button
          v-for="(p, i) in profiles"
          :key="p.key"
          type="button"
          @click="emit('select', p.key)"
          class="w-44 rounded-2xl bg-white/95 hover:bg-white shadow-xl p-5 flex flex-col items-center gap-3 transition-transform duration-150 hover:scale-105 hover:-translate-y-0.5 focus:outline-none focus:ring-2 focus:ring-primary-400"
        >
          <div
            class="w-16 h-16 rounded-full flex items-center justify-center text-2xl font-bold text-white"
            :class="AVATAR_COLORS[i % AVATAR_COLORS.length]"
          >
            {{ (p.name || "?").slice(0, 1).toUpperCase() }}
          </div>
          <div class="min-w-0">
            <p class="font-semibold text-gray-800 text-sm truncate">
              {{ p.name }}
            </p>
            <p class="text-xs text-gray-400 mt-0.5">
              {{ p.active ? "Đang mở • nhấn để đăng nhập" : "Nhấn để mở" }}
            </p>
          </div>
        </button>
      </div>

      <p class="text-xs text-gray-400 mt-8">
        Mỗi hồ sơ là một hộ kinh doanh riêng với dữ liệu độc lập (hoá đơn, kho, lương, thuế, người
        dùng)
      </p>
    </div>
  </div>
</template>
