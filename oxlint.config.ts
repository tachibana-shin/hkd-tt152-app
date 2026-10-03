import { defineConfig } from "oxlint";

export default defineConfig({
  ignorePatterns: [
    "src-tauri/**",
    // Gói Rust cùng workspace — không có mã JS/TS để lint.
    "packages/**",
    "src/auto-imports.d.ts",
    "src/components.d.ts",
  ],
});
