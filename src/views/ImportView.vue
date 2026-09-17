<script setup lang="ts">
import { api } from "@/db";
import type { ImportResult } from "@/types";
import { useAuthStore } from "@/stores/auth";
import { type ImportItem, readNhapLieuSheet } from "@/utils/importExcel";

const toast = useToast();
const confirm = useConfirm();
const auth = useAuthStore();

const file = ref<File | null>(null);
const fileName = ref("");
const reading = ref(false);
const importing = ref(false);
const imported = ref(false);

/** Số dòng "candidate" (có Số phiếu hoặc Ngày ghi sổ) — dùng cho badge "Đã đọc" */
const candCount = ref(0);
/** Số dòng candidate bị bỏ qua (nhãn/tổng, ngày không hợp lệ) */
const skipCount = ref(0);
/** Dữ liệu thô của sheet NHAP LIEU (JS thuần) */
const rawRows = ref<unknown[][]>([]);
/** Dữ liệu đã map sang schema import_nhap_lieu */
const parsedRows = ref<ImportItem[]>([]);

function onFileSelect(e: { files: File[] }) {
  const f = e.files?.[0];
  if (!f) return;
  file.value = f;
  fileName.value = f.name;
  // xoá dữ liệu cũ khi chọn file mới
  rawRows.value = [];
  parsedRows.value = [];
  candCount.value = 0;
  skipCount.value = 0;
  imported.value = false;
}

async function readFile() {
  if (!file.value) {
    toast.add({ severity: "warn", summary: "Chưa chọn file", detail: "Hãy chọn file Excel mẫu trước khi đọc." });
    return;
  }
  reading.value = true;
  try {
    const res = await readNhapLieuSheet(file.value);
    if (!res) {
      toast.add({
        severity: "error",
        summary: "Không tìm thấy sheet 'NHAP LIEU'",
        detail: "File này không chứa sheet dữ liệu 'NHAP LIEU'. Vui lòng chọn đúng file mẫu.",
      });
      return;
    }
    rawRows.value = res.rows;
    parsedRows.value = res.items;
    candCount.value = res.candidates;
    skipCount.value = res.skipped;
    imported.value = false;

    toast.add({
      severity: "success",
      summary: "Đã đọc file",
      detail: `Sheet "${res.sheetName}": nhận diện ${res.items.length} dòng (bỏ qua ${res.skipped} dòng).`,
    });
  } catch (e) {
    toast.add({ severity: "error", summary: "Lỗi đọc file", detail: String(e) });
  } finally {
    reading.value = false;
  }
}

function confirmImport() {
  if (!parsedRows.value.length) {
    toast.add({
      severity: "warn",
      summary: "Chưa có dữ liệu để nhập",
      detail: "Hãy đọc file Excel và kiểm tra dữ liệu nhận diện được trước khi nhập.",
    });
    return;
  }
  confirm.require({
    message: `Sẽ ghi ${parsedRows.value.length} bút toán vào sổ nhật ký chung. Tiếp tục?`,
    header: "Xác nhận nhập liệu",
    icon: "pi pi-exclamation-triangle",
    acceptLabel: "Nhập dữ liệu",
    acceptClass: "p-button-danger",
    accept: async () => {
      importing.value = true;
      try {
        const res: ImportResult = await api.importNhapLieu(parsedRows.value);
        if (res.ok) {
          toast.add({
            severity: "success",
            summary: "Đã nhập dữ liệu vào sổ",
            detail: `Nhập thành công: ${res.imported} dòng • Bỏ qua: ${res.skipped} dòng.`,
          });
        } else {
          toast.add({
            severity: "warn",
            summary: "Nhập có dòng bị bỏ qua",
            detail: `Thành công: ${res.imported} • Bỏ qua: ${res.skipped}.`,
          });
        }
        imported.value = true;
        parsedRows.value = [];
      } catch (e) {
        toast.add({ severity: "error", summary: "Lỗi nhập liệu", detail: String(e) });
      } finally {
        importing.value = false;
      }
    },
  });
}

function reset() {
  file.value = null;
  fileName.value = "";
  rawRows.value = [];
  parsedRows.value = [];
  candCount.value = 0;
  skipCount.value = 0;
  imported.value = false;
}
</script>

<template>
  <div class="space-y-4">
    <Toolbar>
      <template #start>
        <div class="flex items-center gap-3 flex-wrap">
          <div class="flex items-center gap-2">
            <i-mdi-file-import class="text-xl text-rose-500" />
            <h3 class="font-semibold">Nhập liệu Excel (NHAP LIEU)</h3>
          </div>
          <Tag value="File mẫu: Phan Mem Ke toan Excel_HKD - TT152.xlsx" severity="secondary" />
        </div>
      </template>
      <template #end>
        <Button
          label="Làm lại"
          icon="pi pi-replay"
          severity="secondary"
          :disabled="!rawRows.length && !fileName"
          @click="reset"
        />
      </template>
    </Toolbar>

    <!-- Trạng thái rỗng: hướng dẫn -->
    <Card v-if="!rawRows.length">
      <template #content>
        <div class="flex items-start gap-4">
          <i-mdi-information-outline class="text-3xl text-sky-500 shrink-0" />
          <ol class="list-decimal list-inside text-sm text-gray-600 space-y-1">
            <li>Chọn file Excel mẫu (sheet NHAP LIEU).</li>
            <li>Xem trước dữ liệu nhận diện.</li>
            <li>Bấm Nhập để ghi vào sổ nhật ký.</li>
          </ol>
        </div>
      </template>
    </Card>

    <!-- Card 1: Chọn file -->
    <SectionCard
      title="Chọn file"
      tooltip="File cần có sheet NHAP LIEU; bỏ qua dòng tiêu đề/trống; chỉ nhận dòng có Số phiếu hoặc Ngày ghi sổ."
    >
      <template #icon><i-mdi-file-excel class="text-emerald-500" /></template>
      <div class="flex flex-wrap items-center gap-3">
        <FileUpload
          mode="basic"
          accept=".xlsx,.xls"
          chooseLabel="Chọn file"
          customUpload
          :auto="false"
          @select="onFileSelect"
        />
        <Button
          label="Đọc file"
          icon="pi pi-file-excel"
          :loading="reading"
          @click="readFile"
        />
        <span v-if="fileName" class="text-sm text-gray-600 flex items-center gap-1">
          <i-mdi-file-excel class="text-emerald-600" />
          {{ fileName }}
        </span>
      </div>
    </SectionCard>

    <!-- Card 2: Xem trước -->
    <SectionCard v-if="rawRows.length && !imported" title="Xem trước">
      <template #icon><i-mdi-table-eye class="text-sky-500" /></template>
      <ImportPreview :rows="parsedRows" :cand-count="candCount" :skip-count="skipCount" />
    </SectionCard>

    <!-- Card 3: Nhập vào sổ -->
    <SectionCard v-if="rawRows.length || imported" title="Nhập vào sổ" class="border-emerald-200">
      <template #icon><i-mdi-database-import class="text-emerald-500" /></template>
      <div v-if="imported" class="flex flex-wrap items-center gap-3">
        <i-mdi-check-circle class="text-2xl text-emerald-500" />
        <span class="text-sm text-gray-600">
          Đã nhập vào sổ nhật ký. Bấm <b>Làm lại</b> để nhập file khác.
        </span>
      </div>
      <div v-else class="flex flex-wrap items-center gap-3">
        <Button
          v-if="auth.canAccounting"
          :label="parsedRows.length ? `Nhập ${parsedRows.length} dòng vào phần mềm` : 'Nhập dữ liệu vào phần mềm'"
          icon="pi pi-database"
          :loading="importing"
          @click="confirmImport"
        />
        <Button label="Làm lại" icon="pi pi-replay" severity="secondary" @click="reset" />
      </div>
    </SectionCard>
  </div>
</template>
