// Ghi phiên bản vào MỌI file manifest để bundle khớp với tag của release.
// Dùng ở workflow Release (prepare của semantic-release, sau khi nó tính ra
// phiên bản) và chạy được tay: `node scripts/set-version.mjs 0.6.0`.
//
// `--check` (không tham số) chỉ kiểm tra: 4 file phải cùng một phiên bản, và
// phiên bản đó phải khớp với tag v* mới nhất trên remote. Dùng trong CI để bắt
// trường hợp ai đó sửa tay một file rồi quên file còn lại.
//
// Không cần thư viện ngoài — Node là đủ, chạy được trên cả Windows.
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const CARGO = join(root, "src-tauri", "Cargo.toml");
const LOCK = join(root, "src-tauri", "Cargo.lock");

/** Các file JSON chứa khoá "version" cùng nội dung phiên bản của crate Rust. */
const JSON_FILES = ["package.json", "src-tauri/tauri.conf.json"];

const arg = process.argv[2];
const check = arg === "--check";
const version = check ? null : arg;

if (!check && (!version || !/^\d+\.\d+\.\d+(-[\w.]+)?$/.test(version))) {
  console.error("Cách dùng: node scripts/set-version.mjs <phiên-bản> (vd 0.6.0)");
  console.error("         node scripts/set-version.mjs --check");
  process.exit(1);
}

const read = (file) => readFileSync(join(root, file), "utf8");

/** Tên crate trong [package] của Cargo.toml — dùng để tìm đúng khối trong Cargo.lock. */
function crateName() {
  const text = readFileSync(CARGO, "utf8");
  const pkg = text.match(/^\[package\][\s\S]*?^name\s*=\s*"([^"]+)"/m);
  if (!pkg) throw new Error("Không tìm thấy [package].name trong src-tauri/Cargo.toml");
  return pkg[1];
}

/** Phiên bản đang ghi trong một file JSON. */
function jsonVersion(file) {
  const text = read(file);
  const m = text.match(/"version"\s*:\s*"([^"]*)"/);
  if (!m) throw new Error(`Không tìm thấy "version" trong ${file}`);
  return m[1];
}

/** Phiên bản trong khối [[package]] của crate trong Cargo.lock. */
function lockVersion() {
  const text = readFileSync(LOCK, "utf8");
  const block = text.match(
    new RegExp(`\\[\\[package\\]\\]\\nname = "${crateName()}"\\nversion = "([^"]*)"`),
  );
  if (!block) throw new Error(`Không tìm thấy gói ${crateName()} trong src-tauri/Cargo.lock`);
  return block[1];
}

/** Phiên bản trong [package].version của Cargo.toml. */
function cargoVersion() {
  const text = readFileSync(CARGO, "utf8");
  const m = text.match(/^\[package\][\s\S]*?^version\s*=\s*"([^"]*)"/m);
  if (!m) throw new Error("Không tìm thấy version trong src-tauri/Cargo.toml");
  return m[1];
}

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
  const text = readFileSync(CARGO, "utf8");
  // So khớp trước rồi mới thay: nếu version đã đúng, nội dung không đổi nên
  // không so được bằng cách so sánh chuỗi trước/sau.
  if (!/^version = "[^"]*"/m.test(text)) {
    throw new Error("Không tìm thấy version trong src-tauri/Cargo.toml");
  }
  writeFileSync(CARGO, text.replace(/^version = "[^"]*"/m, `version = "${value}"`));
  console.log(`src-tauri/Cargo.toml: version = ${value}`);
}

// Cargo.lock: sửa version của chính crate này (dependency thì cargo tự quản).
// Không sửa thì `cargo build --locked` và Tauri CLI sẽ thấy lockfile lệch.
function setLockVersion(value) {
  const text = readFileSync(LOCK, "utf8");
  const pattern = new RegExp(
    `(\\[\\[package\\]\\]\\nname = "${crateName()}"\\nversion = ")[^"]*(")`,
  );
  if (!pattern.test(text)) {
    throw new Error(`Không tìm thấy gói ${crateName()} trong src-tauri/Cargo.lock`);
  }
  writeFileSync(LOCK, text.replace(pattern, `$1${value}$2`));
  console.log(`src-tauri/Cargo.lock: ${crateName()} = ${value}`);
}

if (check) {
  const found = [
    ...JSON_FILES.map((f) => [f, jsonVersion(f)]),
    ["src-tauri/Cargo.toml", cargoVersion()],
    ["src-tauri/Cargo.lock", lockVersion()],
  ];
  const versions = new Set(found.map(([, v]) => v));
  if (versions.size > 1) {
    console.error("::error::Phiên bản không khớp giữa các file manifest:");
    for (const [file, v] of found) console.error(`  ${file}: ${v}`);
    process.exit(1);
  }
  const [only] = [...versions];
  console.log(`Phiên bản trong repo: ${only}`);

  // Nếu đọc được remote thì so với tag v* mới nhất — tag là nguồn sự thật của
  // bản phát hành. Không có remote/tag thì bỏ qua (ví dụ checkout sâu).
  let tag = null;
  try {
    tag = execFileSync("git", ["ls-remote", "--tags", "origin", "v*"], {
      cwd: root,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    })
      .split("\n")
      .map((l) => l.replace(/^.*refs\/tags\/v/, "").replace(/\^\{\}$/, ""))
      .filter((t) => /^\d+\.\d+\.\d+/.test(t))
      .sort((a, b) => a.localeCompare(b, undefined, { numeric: true }))
      .pop();
  } catch {
    /* không có git hoặc không có remote — bỏ qua bước so với tag */
  }
  if (tag) {
    if (tag !== only) {
      console.error(
        `::error::Tag mới nhất trên remote là v${tag} nhưng repo đang ghi ${only}. ` +
          "Chạy `node scripts/set-version.mjs " + tag + "` rồi commit.",
      );
      process.exit(1);
    }
    console.log(`Khớp tag v${tag} trên remote.`);
  }
  process.exit(0);
}

for (const file of JSON_FILES) setJsonKey(file, "version", version);
setCargoVersion(version);
setLockVersion(version);
