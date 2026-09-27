<script setup lang="ts">
/**
 * Thanh lọc danh sách phiếu (Nhập kho / Xuất kho): từ khoá + khoảng ngày.
 *
 * Danh sách phiếu sắp theo ngày giảm dần nên một phiếu lập từ hóa đơn có ngày
 * cũ sẽ nằm giữa danh sách, rất dễ tưởng "không có phiếu". Ô tìm + lọc ngày là
 * đường tìm nhanh thay vì lần theo từng trang.
 *
 * Bộ lọc nằm ở view (mỗi màn một bộ) nên KeepAlive giữ nguyên khi chuyển menu.
 * Mỗi thay đổi đều báo ra `change` để view nạp lại danh sách.
 */
const from = defineModel<Date | null>("from", { default: null });
const to = defineModel<Date | null>("to", { default: null });
const search = defineModel<string>("search", { default: "" });
const emit = defineEmits<{ change: [] }>();

// Gõ từ khoá gọi API sau khoảng trễ ngắn, tránh dội mỗi phím.
let timer: ReturnType<typeof setTimeout> | null = null;
function fire() {
  if (timer) clearTimeout(timer);
  timer = setTimeout(() => emit("change"), 300);
}
function fireNow() {
  if (timer) clearTimeout(timer);
  emit("change");
}

function onSearchInput(value: string | undefined) {
  search.value = value ?? "";
  fire();
}

/** DatePicker có thể phát ra mảng khi bật chọn khoảng — ở đây chỉ dùng 1 ngày. */
function onDateInput(value: unknown): Date | null {
  if (Array.isArray(value)) return (value[0] as Date | null | undefined) ?? null;
  return (value as Date | null | undefined) ?? null;
}

function onFromInput(value: unknown) {
  from.value = onDateInput(value);
  fireNow();
}

function onToInput(value: unknown) {
  to.value = onDateInput(value);
  fireNow();
}

function clear() {
  search.value = "";
  from.value = null;
  to.value = null;
  fireNow();
}

const hasFilter = computed(() => !!search.value.trim() || !!from.value || !!to.value);

// Bỏ qua khi component bị unmount để không phát `change` sau khi rời màn.
onBeforeUnmount(() => timer && clearTimeout(timer));
</script>

<template>
  <div class="flex flex-wrap items-center gap-2">
    <IconField>
      <InputIcon class="pi pi-search" />
      <InputText
        :model-value="search"
        size="small"
        placeholder="Số phiếu, mã hàng, khách, diễn giải…"
        class="w-64"
        @update:model-value="onSearchInput"
      />
    </IconField>
    <DatePicker
      :model-value="from"
      dateFormat="dd/mm/yy"
      placeholder="Từ ngày"
      size="small"
      show-icon
      class="w-36"
      @update:model-value="onFromInput"
    />
    <DatePicker
      :model-value="to"
      dateFormat="dd/mm/yy"
      placeholder="Đến ngày"
      size="small"
      show-icon
      class="w-36"
      @update:model-value="onToInput"
    />
    <Button
      v-if="hasFilter"
      label="Xoá lọc"
      icon="pi pi-filter-slash"
      size="small"
      severity="secondary"
      text
      @click="clear"
    />
    <span v-if="hasFilter" class="text-xs text-gray-500">
      Đang lọc — bấm “Xoá lọc” để xem toàn bộ phiếu
    </span>
  </div>
</template>
