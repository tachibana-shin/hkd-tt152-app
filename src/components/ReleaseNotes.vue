<script setup lang="ts">
import { renderMarkdown } from "@/utils/markdown";

/**
 * Ghi chú phát hành (release body) — dữ liệu Markdown do semantic-release sinh,
 * render thành HTML rồi style bằng CSS scoped. Dùng chung cho popup "Có bản
 * cập nhật" và khối "Cập nhật ứng dụng" ở màn Cài đặt.
 */
const props = defineProps<{
  /** Nội dung Markdown của release (rỗng / thiếu → báo không có ghi chú). */
  notes?: string | null;
}>();

const html = computed(() => renderMarkdown(props.notes ?? ""));
</script>

<template>
  <div
    v-if="html"
    data-testid="release-notes"
    class="release-notes max-h-72 overflow-y-auto rounded-md border border-gray-200 bg-gray-50 p-3 text-sm leading-relaxed text-gray-700"
    v-html="html"
  />
  <p v-else class="text-sm text-gray-500">Bản này không có ghi chú phát hành.</p>
</template>

<style scoped>
/* Nội dung đến từ v-html nên không mang attribute scoped — style qua :deep(). */
.release-notes :deep(h1),
.release-notes :deep(h2) {
  margin: 0.75rem 0 0.35rem;
  font-size: 0.95rem;
  font-weight: 600;
  color: #1f2937;
}

.release-notes :deep(h1:first-child),
.release-notes :deep(h2:first-child) {
  margin-top: 0;
}

.release-notes :deep(h3),
.release-notes :deep(h4) {
  margin: 0.5rem 0 0.25rem;
  font-size: 0.85rem;
  font-weight: 600;
  color: #374151;
}

.release-notes :deep(p) {
  margin: 0.25rem 0;
}

.release-notes :deep(ul),
.release-notes :deep(ol) {
  margin: 0.25rem 0;
  padding-left: 1.15rem;
}

.release-notes :deep(ul) {
  list-style: disc;
}

.release-notes :deep(ol) {
  list-style: decimal;
}

.release-notes :deep(li) {
  margin: 0.15rem 0;
}

/* Link commit khá dài (URL GitHub) → cắt chữ thay vì làm tràn khung. */
.release-notes :deep(a) {
  color: #4f46e5;
  text-decoration: underline;
  overflow-wrap: anywhere;
}

.release-notes :deep(strong) {
  font-weight: 600;
  color: #111827;
}

.release-notes :deep(code) {
  border-radius: 3px;
  background: #e5e7eb;
  padding: 0 4px;
  font-size: 0.85em;
}

.release-notes :deep(pre) {
  margin: 0.4rem 0;
  overflow-x: auto;
  border-radius: 4px;
  background: #f1f5f9;
  padding: 0.5rem;
}

.release-notes :deep(pre code) {
  background: none;
  padding: 0;
}

.release-notes :deep(hr) {
  margin: 0.6rem 0;
  border: 0;
  border-top: 1px solid #d1d5db;
}

.release-notes :deep(blockquote) {
  margin: 0.4rem 0;
  border-left: 3px solid #d1d5db;
  padding-left: 0.6rem;
  color: #4b5563;
}

.release-notes :deep(img) {
  max-width: 100%;
}
</style>
