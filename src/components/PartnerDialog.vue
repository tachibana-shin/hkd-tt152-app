<script setup lang="ts">
// PartnerDialog — dialog chuẩn thêm / sửa kho, khách hàng, nhà cung cấp.
// Dùng chung cho màn Danh mục đối tác & kho, và mọi nơi cần thêm nhanh đối tác
// (phiếu nhập kho, phiếu xuất…) để tận dụng: tự sinh mã, tra cứu MST.
import { useCatalogStore } from "@/stores/catalog";
import { api } from "@/db";
import type { TaxInfo, TaxResult } from "@/types";

type Kind = "warehouse" | "customer" | "supplier";

const props = withDefaults(
  defineProps<{
    visible: boolean;
    kind: Kind;
    /** Đang sửa đối tác có sẵn (giữ nguyên mã; điền dữ liệu từ `initial`). */
    editing?: boolean;
    initial?: {
      code: string;
      name: string;
      tax_code?: string;
      address?: string;
      phone?: string;
    };
    showAction?: boolean;
  }>(),
  { editing: false, initial: undefined, showAction: true },
);

const emit = defineEmits<{
  (e: "update:visible", value: boolean): void;
  (e: "saved", code: string): void;
}>();

const catalog = useCatalogStore();
const toast = useToast();

const saving = ref(false);
const form = reactive({
  code: "",
  name: "",
  tax_code: "",
  address: "",
  phone: "",
});

// ─── Tra cứu MST (masothue) ───
const lookupLoading = ref(false);
const lookupResults = ref<TaxResult[]>([]);
const lookupSelectedUrl = ref("");

const dialogTitle = computed(() => {
  const base = props.editing ? "Sửa" : "Thêm";
  switch (props.kind) {
    case "warehouse":
      return `${base} kho`;
    case "customer":
      return `${base} khách hàng`;
    default:
      return `${base} nhà cung cấp`;
  }
});

const kindLabel = computed(() => {
  switch (props.kind) {
    case "warehouse":
      return "kho";
    case "customer":
      return "khách hàng";
    case "supplier":
      return "nhà cung cấp";
  }
});

const codePlaceholder = computed(() => {
  switch (props.kind) {
    case "warehouse":
      return "KHO01";
    case "customer":
      return "KH001";
    case "supplier":
      return "NCC001";
  }
});

function resetLookup() {
  lookupLoading.value = false;
  lookupResults.value = [];
  lookupSelectedUrl.value = "";
}

watch(
  () => props.visible,
  async (open) => {
    if (!open) return;
    saving.value = false;
    if (props.editing && props.initial) {
      Object.assign(form, {
        code: props.initial.code,
        name: props.initial.name,
        tax_code: props.initial.tax_code ?? "",
        address: props.initial.address ?? "",
        phone: props.initial.phone ?? "",
      });
    } else {
      Object.assign(form, { code: "", name: "", tax_code: "", address: "", phone: "" });
    }
    resetLookup();
    // Thêm mới: tự sinh mã (KHO001 / KH001 / NCC001…) — vẫn sửa tay được.
    if (!props.editing) {
      try {
        form.code =
          props.kind === "warehouse"
            ? await api.nextWarehouseCode()
            : props.kind === "customer"
              ? await api.nextCustomerCode()
              : await api.nextSupplierCode();
      } catch {
        /* giữ trống — người dùng tự nhập */
      }
    }
  },
);

/** Điền thông tin tra cứu được vào form (chỉ ghi đè khi có giá trị). */
function applyTaxInfo(info: TaxInfo) {
  const t = (v?: string) => (v ?? "").trim();
  const name = t(info.name);
  const addr = t(info.address);
  const phone = t(info.phone);
  if (name) form.name = name;
  if (addr) form.address = addr;
  if (phone) form.phone = phone;
}

async function runLookup() {
  const mst = form.tax_code.trim();
  if (!mst) {
    toast.add({
      severity: "warn",
      summary: "Thiếu MST",
      detail: "Nhập mã số thuế trước khi tra cứu",
    });
    return;
  }
  lookupLoading.value = true;
  lookupResults.value = [];
  lookupSelectedUrl.value = "";
  try {
    const outcome = await api.lookupTaxCode(mst);
    if (outcome.kind === "single") {
      applyTaxInfo(outcome.info);
      toast.add({
        severity: "success",
        summary: "Đã tra cứu",
        detail: `Tìm thấy: ${outcome.info.name ?? outcome.info.tax_code ?? mst}`,
      });
    } else if (outcome.kind === "multiple") {
      lookupResults.value = outcome.results;
      toast.add({
        severity: "info",
        summary: "Nhiều kết quả",
        detail: `Có ${outcome.results.length} kết quả — chọn đúng bên dưới`,
      });
    } else {
      toast.add({
        severity: "warn",
        summary: "Không tìm thấy",
        detail: outcome.message,
      });
    }
  } catch (e) {
    toast.add({ severity: "error", summary: "Lỗi tra cứu", detail: String(e) });
  } finally {
    lookupLoading.value = false;
  }
}

async function applyLookupResult(url: string) {
  if (!url) return;
  try {
    const info = await api.lookupTaxDetail(url);
    applyTaxInfo(info);
    lookupSelectedUrl.value = url;
    toast.add({
      severity: "success",
      summary: "Đã điền thông tin",
      detail: info.name ?? "Xong",
    });
  } catch (e) {
    toast.add({ severity: "error", summary: "Lỗi lấy chi tiết", detail: String(e) });
  }
}

async function save() {
  if (!form.code || !form.name) {
    toast.add({
      severity: "warn",
      summary: "Thiếu thông tin",
      detail: "Mã và tên là bắt buộc",
    });
    return;
  }
  saving.value = true;
  try {
    if (props.kind === "warehouse") {
      await catalog.saveWarehouse(form.code, form.name);
    } else if (props.kind === "customer") {
      await catalog.saveCustomer({
        code: form.code,
        name: form.name,
        tax_code: form.tax_code,
        address: form.address,
        phone: form.phone,
      });
    } else {
      await catalog.saveSupplier({
        code: form.code,
        name: form.name,
        tax_code: form.tax_code,
        address: form.address,
        phone: form.phone,
      });
    }
    toast.add({
      severity: "success",
      summary: "Đã lưu",
      detail: `Đã lưu ${kindLabel.value} ${form.code}`,
    });
    emit("update:visible", false);
    emit("saved", form.code);
  } catch (e) {
    toast.add({ severity: "error", summary: "Lỗi lưu", detail: String(e) });
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <AppDialog
    :visible="visible"
    :header="dialogTitle"
    width="max-w-xl"
    action-label="Lưu"
    :saving="saving"
    :show-action="showAction"
    @update:visible="emit('update:visible', $event)"
    @action="save"
  >
    <div class="grid grid-cols-2 gap-4 py-2">
      <FormField :label="kind === 'warehouse' ? 'Mã kho' : 'Mã'" required>
        <InputText v-model="form.code" :placeholder="codePlaceholder" :disabled="editing" />
      </FormField>
      <FormField :label="kind === 'warehouse' ? 'Tên kho' : 'Tên'" required>
        <InputText v-model="form.name" placeholder="Tên hiển thị" />
      </FormField>

      <template v-if="kind !== 'warehouse'">
        <FormField label="MST">
          <div class="flex gap-2">
            <InputText v-model="form.tax_code" placeholder="Mã số thuế" class="min-w-0 flex-1" />
            <Button
              label="Tra cứu"
              icon="pi pi-search"
              severity="secondary"
              class="shrink-0 whitespace-nowrap"
              :loading="lookupLoading"
              :disabled="!form.tax_code.trim()"
              @click="runLookup"
            />
          </div>
        </FormField>
        <FormField v-if="lookupResults.length" label="Kết quả tra cứu" class="col-span-2">
          <Select
            v-model="lookupSelectedUrl"
            :options="lookupResults"
            option-label="name"
            option-value="url"
            placeholder="Có nhiều kết quả — chọn đúng hộ / doanh nghiệp…"
            class="w-full"
            @change="applyLookupResult($event.value)"
          />
        </FormField>
        <FormField label="Điện thoại">
          <InputText v-model="form.phone" placeholder="Số điện thoại" />
        </FormField>
        <FormField label="Địa chỉ" class="col-span-2">
          <InputText v-model="form.address" placeholder="Địa chỉ" />
        </FormField>
      </template>
    </div>
  </AppDialog>
</template>
