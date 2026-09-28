<script setup lang="ts">
/**
 * Chép hóa đơn sang máy tính tiền bên kia (không có API → chép tay).
 *
 * Bên kia không có danh mục hàng: mỗi lần phát hành phải nhập lại từng dòng
 * vào ô text tự do nhận nhiều dòng. Dialog dựng sẵn khối dán theo đúng bố cục
 * cột + dấu phân cách mà form bên kia nhận (chọn 1 lần, app nhớ cho các hóa
 * đơn sau), kèm kiểm tra trước khi chép và bước đánh dấu "đã chép" — lúc đó app
 * lưu bản chốt để sau này phát hiện hóa đơn bị sửa.
 */
import { api } from "@/db";
import { useAuthStore } from "@/stores/auth";
import { useSettingsStore } from "@/stores/settings";
import Textarea from "primevue/textarea";
// PrimeVue không được auto-import trong dự án — import tường minh.
import SelectButton from "primevue/selectbutton";
import type { Invoice, InvoiceExportPack } from "@/types";
import { fmtVnd } from "@/utils/format";

const visible = defineModel<boolean>("visible", { default: false });
const props = defineProps<{ invoice: Invoice | null }>();
const emit = defineEmits<{ done: [] }>();

const auth = useAuthStore();
const settings = useSettingsStore();
const toast = useToast();
const pack = ref<InvoiceExportPack | null>(null);
const loading = ref(false);
const saving = ref(false);
const tab = ref<"paste" | "text" | "json">("paste");
const showChecks = ref(true);

/** Bố cục cột của 1 dòng dán, theo những gì form bên kia thực nhận. */
const layoutOptions = [
  { value: "full", label: "Tên · ĐVT · SL · Đơn giá · CK · Thành tiền" },
  { value: "no_discount", label: "Tên · ĐVT · SL · Đơn giá · Thành tiền" },
  { value: "no_unit", label: "Tên · SL · Đơn giá · Thành tiền" },
  { value: "amount_only", label: "Tên · SL · Thành tiền" },
];
const sepOptions = [
  { value: ",", label: "Dấu phẩy  ,  (phần mềm POS hay dùng)" },
  { value: "\t", label: "Tab" },
  { value: ";", label: "Dấu chấm phẩy  ;" },
  { value: "|", label: "Dấu gạch đứng  |" },
];

type PasteLayout = "full" | "no_discount" | "no_unit" | "amount_only";
const pasteLayout = ref<PasteLayout>("no_discount");
const pasteSep = ref(",");

const errors = computed(() => pack.value?.checks.filter((c) => c.level === "error") ?? []);
const warns = computed(() => pack.value?.checks.filter((c) => c.level === "warn") ?? []);
const blocked = computed(() => errors.value.length > 0);

const body = computed(() => {
  if (!pack.value) return "";
  if (tab.value === "text") return pack.value.text;
  if (tab.value === "json") return JSON.stringify(pack.value.json, null, 2);
  return pack.value.paste;
});

/** Đọc lựa chọn đã lưu, dựng lại gói chép theo bố cục/dấu phân cách đó. */
async function load() {
  if (!props.invoice?.id) return;
  loading.value = true;
  try {
    const saved = settings.settings;
    if (saved) {
      const layout = saved.invoice_paste_layout;
      const sep = saved.invoice_paste_sep;
      if (layout && layoutOptions.some((o) => o.value === layout))
        pasteLayout.value = layout as PasteLayout;
      if (sep && sepOptions.some((o) => o.value === sep)) pasteSep.value = sep;
    }
    pack.value = await api.invoiceExportPack(props.invoice.id, {
      pasteLayout: pasteLayout.value,
      pasteSep: pasteSep.value,
    });
  } catch (e) {
    pack.value = null;
    toast.add({ severity: "error", summary: "Không dựng được dữ liệu chép", detail: String(e) });
  } finally {
    loading.value = false;
  }
}

/** Đổi bố cục/dấu phân cách → dựng lại khối dán và nhớ lại cho lần sau. */
async function onFormatChange() {
  await load();
  try {
    await settings.save({
      invoice_paste_layout: pasteLayout.value,
      invoice_paste_sep: pasteSep.value,
    });
  } catch {
    // Không nhớ được lựa chọn không ảnh hưởng việc chép — bỏ qua.
  }
}

watch(
  [visible, () => props.invoice?.id],
  ([open, id], [wasOpen, wasId]) => {
    // Chỉ nạp lại khi mở dialog hoặc đổi hóa đơn đích.
    if (open && (!wasOpen || id !== wasId)) void load();
  },
  { immediate: true },
);

async function copy() {
  try {
    await navigator.clipboard.writeText(body.value);
    toast.add({
      severity: "success",
      summary: "Đã chép vào clipboard",
      detail:
        tab.value === "paste"
          ? `Dán vào ô dòng hàng bên kia (tách cột bằng ${pasteSep.value === "\t" ? "Tab" : `"${pasteSep.value}"`})`
          : "Dán vào ô nhập của dịch vụ",
    });
  } catch (e) {
    toast.add({ severity: "error", summary: "Không chép được", detail: String(e) });
  }
}

async function markExported() {
  if (!props.invoice?.id) return;
  saving.value = true;
  try {
    pack.value = await api.invoiceMarkExported(props.invoice.id, {
      pasteLayout: pasteLayout.value,
      pasteSep: pasteSep.value,
    });
    toast.add({
      severity: "success",
      summary: "Đã ghi nhận bản chốt",
      detail:
        "Hóa đơn chuyển sang 'Đã chép — chờ phát hành'. Nếu bạn sửa hóa đơn sau đó, " +
        "app sẽ báo lệch để bạn sửa lại bên kia.",
    });
    visible.value = false;
    emit("done");
  } catch (e) {
    toast.add({ severity: "error", summary: "Không ghi nhận được", detail: String(e) });
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <AppDialog
    v-model:visible="visible"
    :header="`Chép hóa đơn ${invoice?.number ?? ''} sang dịch vụ khác`"
    width="max-w-5xl"
    cancel-label="Đóng"
    action-label="Đã chép sang bên kia"
    action-icon="pi pi-check"
    :saving="saving"
    :show-action="auth.canAccounting"
    :action-disabled="blocked || loading"
    @action="markExported"
  >
    <div v-if="loading" class="flex justify-center py-8">
      <ProgressSpinner />
    </div>

    <div v-else-if="pack" class="space-y-3">
      <!-- Cảnh báo đã sửa sau khi chép: quan trọng nhất, đưa lên đầu -->
      <Message
        v-if="pack.edited_after_export"
        severity="warn"
        :closable="false"
        icon="pi pi-exclamation-triangle"
      >
        <b>Hóa đơn đã bị sửa sau lần chép gần nhất.</b>
        Số tiền hoặc dòng hàng đã khác với bản chốt → bạn cần cập nhật lại bên kia (thường là hủy HĐ
        cũ và phát hành HĐ thay thế).
      </Message>

      <Message v-if="pack.export_count > 0" severity="info" :closable="false">
        Đã chép <b>{{ pack.export_count }}</b> lần. Lần chép gần nhất: bản chốt dùng để đối chiếu.
      </Message>

      <!-- Kiểm tra trước khi chép -->
      <div v-if="errors.length || warns.length">
        <button
          type="button"
          class="flex w-full items-center gap-1 text-sm font-medium text-gray-600"
          @click="showChecks = !showChecks"
        >
          <i :class="showChecks ? 'pi pi-chevron-down' : 'pi pi-chevron-right'" class="text-xs" />
          Kiểm tra trước khi chép ({{ errors.length }} lỗi · {{ warns.length }} cảnh báo)
        </button>
        <ul v-if="showChecks" class="mt-2 space-y-1 text-sm">
          <li
            v-for="(c, i) in pack.checks.filter((c) => c.level !== 'ok')"
            :key="i"
            class="flex items-start gap-2"
            :class="c.level === 'error' ? 'text-red-600' : 'text-amber-700'"
          >
            <i
              :class="c.level === 'error' ? 'pi pi-times-circle' : 'pi pi-exclamation-triangle'"
              class="mt-0.5 text-xs"
            />
            <span>{{ c.message }}</span>
          </li>
        </ul>
      </div>

      <!-- Thông tin người mua -->
      <div
        class="grid grid-cols-1 sm:grid-cols-2 gap-3 rounded border border-gray-200 p-3 text-sm md:grid-cols-4"
      >
        <div>
          <div class="text-gray-500">Người mua</div>
          <div class="font-medium">{{ pack.header.customer || "—" }}</div>
        </div>
        <div>
          <div class="text-gray-500">MST</div>
          <div class="font-medium">{{ pack.header.customer_tax_code || "—" }}</div>
        </div>
        <div>
          <div class="text-gray-500">Ngày</div>
          <div class="font-medium">{{ pack.header.date_vn }}</div>
        </div>
        <div>
          <div class="text-gray-500">Tổng tiền (khách trả)</div>
          <div class="font-semibold">{{ fmtVnd(pack.header.total) }}</div>
        </div>
      </div>

      <!-- Dòng hàng -->
      <div class="overflow-x-auto rounded border border-gray-200">
        <table class="w-full text-sm">
          <thead class="bg-gray-50 text-xs text-gray-500">
            <tr>
              <th class="w-8 px-2 py-1 text-right">#</th>
              <th class="px-2 py-1 text-left">Tên hàng</th>
              <th class="w-14 px-2 py-1 text-left">ĐVT</th>
              <th class="w-20 px-2 py-1 text-right">SL</th>
              <th class="w-28 px-2 py-1 text-right">Đơn giá</th>
              <th class="w-24 px-2 py-1 text-right">Chiết khấu</th>
              <th class="w-28 px-2 py-1 text-right">Thành tiền</th>
              <th
                class="px-2 py-1 text-left"
                title="Cơ sở tính thuế cuối kỳ — không có trên hóa đơn"
              >
                Thuế suất (kê cuối kỳ)
              </th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="l in pack.lines" :key="l.stt" class="border-t border-gray-100 align-top">
              <td class="px-2 py-1 text-right text-gray-400">{{ l.stt }}</td>
              <td class="px-2 py-1">
                {{ l.product_name }}
                <div v-for="w in l.warnings" :key="w" class="text-xs text-amber-700" :title="w">
                  ⚠ {{ w }}
                </div>
              </td>
              <td class="px-2 py-1">{{ l.unit }}</td>
              <td class="px-2 py-1 text-right">{{ l.quantity }}</td>
              <td class="px-2 py-1 text-right">{{ fmtVnd(l.unit_price) }}</td>
              <td class="px-2 py-1 text-right">{{ l.discount ? fmtVnd(l.discount) : "—" }}</td>
              <td class="px-2 py-1 text-right font-medium">{{ fmtVnd(l.amount) }}</td>
              <td class="px-2 py-1 text-xs">{{ l.vat_label }}</td>
            </tr>
          </tbody>
        </table>
      </div>

      <!-- Bố cục + dấu phân cách: chọn đúng với form máy tính tiền đang dùng -->
      <div class="grid grid-cols-1 gap-3 md:grid-cols-2">
        <FormField label="Cột của 1 dòng dán" input-id="paste-layout">
          <Select
            :id="'paste-layout'"
            v-model="pasteLayout"
            :options="layoutOptions"
            option-label="label"
            option-value="value"
            size="small"
            class="w-full"
            @change="onFormatChange"
          />
        </FormField>
        <FormField label="Dấu tách cột" input-id="paste-sep">
          <Select
            :id="'paste-sep'"
            v-model="pasteSep"
            :options="sepOptions"
            option-label="label"
            option-value="value"
            size="small"
            class="w-full"
            @change="onFormatChange"
          />
        </FormField>
      </div>

      <!-- Khối chép -->
      <div>
        <div class="mb-1 flex flex-wrap items-center gap-2">
          <SelectButton
            v-model="tab"
            :options="[
              { label: 'Dán dòng hàng', value: 'paste' },
              { label: 'Text đầy đủ', value: 'text' },
              { label: 'JSON', value: 'json' },
            ]"
            option-label="label"
            option-value="value"
            :allow-empty="false"
            size="small"
          />
          <Button
            label="Chép"
            icon="pi pi-copy"
            size="small"
            :outlined="true"
            severity="secondary"
            :disabled="blocked"
            @click="copy"
          />
          <span class="text-xs text-gray-500">
            {{
              tab === "paste"
                ? `Dán vào ô dòng hàng bên kia — cột: ${pack.paste_columns.join(" · ")}`
                : tab === "text"
                  ? "Dán vào ô nhập tự do / khung mô tả."
                  : "Khi bên kia có ô nhập JSON."
            }}
          </span>
        </div>
        <Textarea :model-value="body" readonly rows="8" class="w-full font-mono text-xs" />
        <p class="mt-1 text-xs text-gray-500">
          Hóa đơn bán không tách thuế: tổng tiền là số tiền khách trả, thuế tính cuối kỳ theo tỷ lệ
          % × doanh thu. Số trong khối dán viết không dấu phân cách nghìn để máy đọc đúng.
        </p>
      </div>
    </div>
  </AppDialog>
</template>
