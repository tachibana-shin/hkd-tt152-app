import { defineConfig } from "oxfmt";

export default defineConfig({
  ignorePatterns: [
    "src-tauri/**",
    "src/auto-imports.d.ts",
    "src/components.d.ts",
    // Do semantic-release sinh ra, không sửa tay: ép format sẽ làm CI đỏ ở mọi
    // commit sau khi changelog được cập nhật.
    "CHANGELOG.md",
  ],
});
