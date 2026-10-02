import { defineConfig } from "oxlint";

export default defineConfig({
  ignorePatterns: [
    "src-tauri/**",
    "src/auto-imports.d.ts",
    "src/components.d.ts",
    // Tài nguyên nhúng của hóa đơn (font/ảnh nhị phân + bundle capture.js đã
    // minify) — không phải code do ta viết.
    "src/invoice-pdf/assets/**",
  ],
});
