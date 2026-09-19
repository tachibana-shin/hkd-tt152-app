import { defineConfig } from "oxfmt";

export default defineConfig({
  ignorePatterns: ["src-tauri/**", "src/auto-imports.d.ts", "sr/components.d.ts"],
});
