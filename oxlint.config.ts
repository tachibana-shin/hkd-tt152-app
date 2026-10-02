import { defineConfig } from "oxlint";

export default defineConfig({
  ignorePatterns: ["src-tauri/**", "src/auto-imports.d.ts", "src/components.d.ts"],
});
