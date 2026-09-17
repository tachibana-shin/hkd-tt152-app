<script setup lang="ts">
// Bảng nhập dòng mặt hàng dùng chung cho phiếu nhập / phiếu xuất.
import { useCatalogStore } from "@/stores/catalog";
import { fmtInt as fmt } from "@/utils/format";
import type { IndustryGroup, Product } from "@/types";

interface LineItem {
  product_code: string;
  quantity: number;
  unit_price: number;
  industry_code?: string;
}

const props = withDefaults(
  defineProps<{
    items: LineItem[];
    products: Product[];
    industryGroups?: IndustryGroup[];
    canEdit?: boolean;
    showIndustry?: boolean;
    showAmount?: boolean;
    priceField?: "cost_price" | "sale_price";
    info?: string;
    totalLabel?: string;
  }>(),
  {
    canEdit: true,
    showIndustry: false,
    showAmount: false,
    priceField: "cost_price",
    industryGroups: () => [],
    totalLabel: "Tổng tiền:",
  },
);

const emit = defineEmits<{
  (e: "add"): void;
  (e: "remove", index: number): void;
}>();

const catalog = useCatalogStore();

function onProductPick(index: number) {
  const p = catalog.productByCode(props.items[index].product_code);
  if (!p) return;
  props.items[index].unit_price = props.priceField === "sale_price" ? p.sale_price : p.cost_price;
}

const total = computed(() => props.items.reduce((s, it) => s + it.quantity * it.unit_price, 0));
</script>

<template>
  <div class="mt-4 flex items-center justify-between">
    <h4 class="flex items-center gap-1.5 text-sm font-semibold">
      Mặt hàng
      <i
        v-if="info"
        class="pi pi-info-circle cursor-help text-xs text-gray-400"
        v-tooltip="info"
        aria-hidden="true"
      />
    </h4>
    <Button v-if="canEdit" label="Thêm dòng" icon="pi pi-plus" size="small" text @click="emit('add')" />
  </div>

  <DataTable :value="items" class="mt-2">
    <Column header="Sản phẩm" style="min-width: 200px">
      <template #body="{ index }">
        <Select
          v-model="items[index].product_code"
          :options="products"
          optionLabel="name"
          optionValue="code"
          filter
          showClear
          class="w-full"
          @change="onProductPick(index)"
        />
      </template>
    </Column>
    <Column header="SL" :style="showIndustry ? 'width: 100px' : 'width: 110px'">
      <template #body="{ index }">
        <InputNumber v-model="items[index].quantity" :min="0" class="w-full" />
      </template>
    </Column>
    <Column header="Đơn giá" :style="showIndustry ? 'width: 140px' : 'width: 150px'">
      <template #body="{ index }">
        <InputNumber
          v-model="items[index].unit_price"
          :min="0"
          mode="currency"
          currency="VND"
          locale="vi-VN"
          class="w-full"
        />
      </template>
    </Column>
    <Column v-if="showIndustry" header="Nhóm ngành" style="width: 170px">
      <template #body="{ index }">
        <Select
          v-model="items[index].industry_code"
          :options="industryGroups"
          optionLabel="name"
          optionValue="code"
          filter
          class="w-full"
        />
      </template>
    </Column>
    <Column v-if="showAmount" header="Thành tiền" style="width: 140px" align="right">
      <template #body="{ index }">
        <span class="font-medium">{{ fmt(items[index].quantity * items[index].unit_price) }}</span>
      </template>
    </Column>
    <Column header="" style="width: 60px">
      <template #body="{ index }">
        <Button
          v-if="canEdit"
          icon="pi pi-times"
          text
          rounded
          size="small"
          severity="danger"
          @click="emit('remove', index)"
        />
      </template>
    </Column>
    <template #empty><EmptyState text="Chưa có dòng nào" icon="pi pi-list" /></template>
  </DataTable>

  <div class="mt-4 flex items-center justify-end gap-3">
    <span class="text-sm text-gray-500">{{ totalLabel }}</span>
    <span class="text-lg font-bold text-primary-600">{{ fmt(total) }} đ</span>
  </div>
</template>
