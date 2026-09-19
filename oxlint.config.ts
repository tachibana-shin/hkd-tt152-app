import { defineConfig } from "oxlint";

export default defineConfig({
  ignorePatterns: ["src-tauri/**", "src/auto-imports.d.ts", "sr/components.d.ts"],
});
