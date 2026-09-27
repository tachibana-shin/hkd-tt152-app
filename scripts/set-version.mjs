// Ghi phiên bản vào các file manifest để bundle khớp với tag của release.
// Dùng ở workflow Release (sau khi semantic-release tính ra phiên bản) và
// chạy được tay: `node scripts/set-version.mjs 0.4.0`
//
// Không cần thư viện ngoài — Node là đủ, chạy được trên cả Windows.
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const version = process.argv[2];
if (!version || !/^\d+\.\d+\.\d+(-[\w.]+)?$/.test(version)) {
  console.error("Cách dùng: node scripts/set-version.mjs <phiên-bản> (vd 0.4.0)");
  process.exit(1);
}

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

/** Thay giá trị của một khoá JSON, giữ nguyên thứ tự và thụt lề 2 space. */
function setJsonKey(file, key, value) {
  const path = join(root, file);
  const text = readFileSync(path, "utf8");
  const pattern = new RegExp(`("${key}"\\s*:\\s*)"[^"]*"`);
  if (!pattern.test(text)) {
    throw new Error(`Không tìm thấy "${key}" trong ${file}`);
  }
  writeFileSync(path, text.replace(pattern, `$1"${value}"`));
  console.log(`${file}: ${key} = ${value}`);
}

// Cargo.toml không phải JSON — thay đúng dòng version trong [package].
function setCargoVersion(value) {
  const path = join(root, "src-tauri", "Cargo.toml");
  const text = readFileSync(path, "utf8");
  // So khớp trước rồi mới thay: nếu version đã đúng, nội dung không đổi nên
  // không so được bằng cách so sánh chuỗi trước/sau.
  if (!/^version = "[^"]*"/m.test(text)) {
    throw new Error("Không tìm thấy version trong src-tauri/Cargo.toml");
  }
  writeFileSync(path, text.replace(/^version = "[^"]*"/m, `version = "${value}"`));
  console.log(`src-tauri/Cargo.toml: version = ${value}`);
}

setJsonKey("package.json", "version", version);
setJsonKey("src-tauri/tauri.conf.json", "version", version);
setCargoVersion(version);
