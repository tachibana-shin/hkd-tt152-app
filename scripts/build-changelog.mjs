// Sinh CHANGELOG.md cho các bản phát hành ĐÃ CÓ, đúng định dạng
// @semantic-release/release-notes-generator sẽ dùng cho các bản sau.
//
//   node scripts/build-changelog.mjs            # ghi CHANGELOG.md
//   node scripts/build-changelog.mjs --print    # chỉ in ra, không ghi
//
// Vì sao cần: CHANGELOG.md chỉ được tạo ở lần chạy semantic-release đầu tiên,
// nên nó sẽ trống trắng và mất luôn lịch sử các bản đã đóng gói trước đó.
// Script này dựng lại từ chính commit + tag trong git, không bịa nội dung.
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

import { generateNotes } from "@semantic-release/release-notes-generator";
import { analyzeCommits } from "@semantic-release/commit-analyzer";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const config = JSON.parse(readFileSync(join(root, ".releaserc.json"), "utf8"));

const pluginConfig = (name) => {
  const found = config.plugins.find((p) => (Array.isArray(p) ? p[0] : p) === name);
  return Array.isArray(found) ? found[1] : undefined;
};

const git = (...args) =>
  execFileSync("git", args, { cwd: root, encoding: "utf8" }).trimEnd();

const repositoryUrl = git("config", "--get", "remote.origin.url")
  .replace(/^git@github\.com:/, "https://github.com/")
  .replace(/\.git$/, "");

/**
 * Commit trong khoảng (from, to] theo thứ tự cũ → mới.
 *
 * Đúng cấu trúc semantic-release đưa cho plugin: mỗi phần tử có `message`/
 * `longMessage` ở cấp ngoài (commit-anitizer đọc `message`), kèm `hash` và
 * `commit` là object git-js.
 */
function commitsBetween(from, to) {
  const range = from ? `${from}..${to}` : to;
  const hashes = git("log", "--reverse", "--format=%H", range)
    .split("\n")
    .filter(Boolean);
  return hashes.map((hash) => {
    const message = git("log", "-1", "--format=%B", hash).trimEnd();
    return { message, longMessage: message, hash, commit: { message, longMessage: message, hash } };
  });
}

/** Ngày của tag theo định dạng YYYY-MM-DD. */
const tagDate = (tag) => git("log", "-1", "--format=%cs", tag);

/**
 * Dọn hai điểm mà generator tạo ra từ dữ liệu thô:
 *
 * 1. Tiêu đề: bản đầu tiên không có tag trước nên generator sinh `# 0.3.0`
 *    (h1, không link). Chuẩn hoá mọi mục về `## [x.y.z](link) (ngày)` cho đồng
 *    bộ với các bản sau (chúng luôn có tag trước).
 * 2. Tham chiếu issue giả: commit có thể nhắc `#body` trong phần mô tả (ví dụ
 *    "ô có template #body"), parser coi đó là issue và thêm ", closes [#body]".
 *    Issue thật trên GitHub luôn là số, nên bỏ tham chiếu không chứa chữ số.
 */
function tidy(notes, version, tag, previousTag) {
  const link = previousTag
    ? `${repositoryUrl}/compare/${previousTag}...${tag}`
    : `${repositoryUrl}/tags/${tag}`;
  return notes
    .replace(/^#{1,3} .*$/m, `## [${version}](${link}) (${tagDate(tag)})`)
    .replace(/, (?:closes|close|fixes|fix|resolves|resolve) \[#\D+\]\([^)]*\)/gi, "");
}

// Đọc đúng cấu hình trong .releaserc.json: cùng preset, cùng releaseRules,
// cùng presetConfig.types → kết quả khớp bản semantic-release sẽ sinh.
const notesConfig = pluginConfig("@semantic-release/release-notes-generator");
const analyzerConfig = pluginConfig("@semantic-release/commit-analyzer");

const tags = git("tag", "--list", "v*", "--sort=v:refname")
  .split("\n")
  .map((t) => t.trim())
  .filter((t) => /^v\d+\.\d+\.\d+/.test(t));
if (tags.length === 0) {
  console.error("Chưa có tag v* nào — không có gì để dựng changelog.");
  process.exit(1);
}

const sections = [];
for (const [index, tag] of tags.entries()) {
  const version = tag.slice(1);
  const previousTag = index === 0 ? null : tags[index - 1];
  // Commit của chính tag đó đã nằm trong khoảng trước đó, nên tag trước là mốc.
  const commits = commitsBetween(previousTag, tag);
  // Bản đầu tiên không có bản trước → generator tự sinh link `/tags/vX.Y.Z`
  // thay vì link compare, giống hệt lúc semantic-release chạy lần đầu.
  const lastRelease = previousTag
    ? { gitTag: previousTag, gitHead: undefined, version: previousTag.slice(1) }
    : { gitTag: undefined, gitHead: undefined, version: "0.0.0" };
  const { type } = await analyzeCommits(analyzerConfig, {
    commits,
    cwd: root,
    logger: { log() {}, error() {} },
    options: { repositoryUrl, lastRelease },
  });
  // `notes` ĐÃ bao gồm dòng tiêu đề → dùng nguyên văn, không tự thêm heading.
  const notes = await generateNotes(notesConfig, {
    commits,
    lastRelease,
    nextRelease: { gitTag: tag, gitHead: undefined, type, version },
    options: { repositoryUrl },
    cwd: root,
    logger: { log() {}, error() {} },
  });
  sections.push(tidy(notes.trim(), version, tag, previousTag));
}

const header = [
  "# Changelog",
  "",
  "Nhật ký thay đổi của HKD Kế Toán. Mỗi mục tương ứng một bản phát hành;",
  "phiên bản được [semantic-release](https://github.com/semantic-release/semantic-release)",
  "tính tự động từ commit trước đó (`feat` → minor, `fix` → patch, kèm `!` → major).",
  "",
  sections.join("\n\n"),
  "",
].join("\n");

if (process.argv.includes("--print")) {
  console.log(header);
} else {
  writeFileSync(join(root, "CHANGELOG.md"), header, "utf8");
  console.log(`CHANGELOG.md: ${tags.length} mục (${tags.join(", ")})`);
}
