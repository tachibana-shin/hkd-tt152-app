import { defineConfig } from "oxfmt";

export default defineConfig({
  ignorePatterns: [
    "src-tauri/**",
    // Gói Rust cùng workspace (Cargo.toml ở gốc) — không phải nguồn frontend;
    // `template.html` trong đó là asset nhúng bằng include_str! và format lại
    // sẽ đổi khoảng trắng mà htmltopdf đang render.
    "packages/**",
    // Bộ cache truy vấn sqlx do `cargo sqlx prepare` sinh ra (nằm ở workspace
    // root): format lại sẽ làm `--check` đỏ ngay ở lần chạy sau.
    ".sqlx/**",
    "src/auto-imports.d.ts",
    "src/components.d.ts",
    // Do semantic-release sinh ra, không sửa tay: ép format sẽ làm CI đỏ ở mọi
    // commit sau khi changelog được cập nhật.
    "CHANGELOG.md",
  ],
});
