import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import tailwindcss from "@tailwindcss/vite";
import AutoImport from "unplugin-auto-import/vite";
import Components from "unplugin-vue-components/vite";
import Icons from "unplugin-icons/vite";
import IconsResolver from "unplugin-icons/resolver";
import { fileURLToPath, URL } from "node:url";
import process from "node:process";

const host = process.env.TAURI_DEV_HOST;

// PrimeVue component dùng trong app. Resolver có sẵn của unplugin-vue-components chưa cập nhật
// cho PrimeVue v5 (thiếu DatePicker/Select/Toast/ConfirmDialog/Tabs…), nên khai báo tường minh
// để auto-import theo nhu cầu thay vì đăng ký toàn cục trong main.ts.
const PRIMEVUE_COMPONENTS = [
  "AutoComplete", "Avatar", "Button", "Card", "Checkbox", "Column", "ConfirmDialog",
  "DataTable", "DatePicker", "Dialog", "Divider", "FileUpload", "IconField",
  "InputIcon", "InputNumber", "InputText", "ProgressSpinner", "Select",
  "Tab", "TabList", "TabPanel", "TabPanels", "Tabs", "Tag", "Toast",
  "ToggleSwitch", "Toolbar",
];

const PrimeVueLocalResolver = () => ({
  type: "component" as const,
  resolve: (name: string) =>
    PRIMEVUE_COMPONENTS.includes(name) ? { from: `primevue/${name.toLowerCase()}` } : undefined,
});

export default defineConfig(() => ({
  plugins: [
    vue(),
    tailwindcss(),

    // Auto-import Vue/Router/Pinia APIs + invoke (generates src/auto-imports.d.ts)
    AutoImport({
      imports: [
        "vue",
        "vue-router",
        "pinia",
        { "@tauri-apps/api/core": ["invoke"] },
        { from: "primevue/usetoast", imports: ["useToast"] },
        { from: "primevue/useconfirm", imports: ["useConfirm"] },
      ],
      dts: "src/auto-imports.d.ts",
      vueTemplate: true,
    }),

    // Auto-register local components in src/components + PrimeVue + Iconify icons (<i-mdi-home />)
    Components({
      dirs: ["src/components"],
      extensions: ["vue"],
      dts: "src/components.d.ts",
      resolvers: [IconsResolver({ prefix: "i" }), PrimeVueLocalResolver()],
    }),

    Icons({ compiler: "vue3", autoInstall: true }),
  ],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  // Vite chỉ quét deps tĩnh trong entry .ts; import PrimeVue nằm trong .vue nên không được
  // pre-bundle từ lúc khởi động → phát hiện muộn khi chạy và reload trang giữa chừng. Khai báo sẵn.
  optimizeDeps: {
    include: [
      "@primeuix/themes/aura",
      "primevue/config",
      "primevue/toastservice",
      "primevue/confirmationservice",
      "primevue/tooltip",
      "primevue/usetoast",
      "primevue/useconfirm",
      ...PRIMEVUE_COMPONENTS.map((name) => `primevue/${name.toLowerCase()}`),
    ],
  },
  // Khi dùng app qua trình duyệt (Chrome) ở dev — chính app desktop chạy web server
  // local 127.0.0.1:45731, proxy /api qua đó.
  proxy: {
    "/api": { target: "http://127.0.0.1:45731", changeOrigin: true },
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? { protocol: "ws", host, port: 1421 }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
}));