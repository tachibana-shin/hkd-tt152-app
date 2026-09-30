// Extracts the change list of one release from CHANGELOG.md so the Release body can
// be assembled from it.
//
//   node scripts/release-notes.mjs 0.12.0        # prints the "Thay đổi" section
//   node scripts/release-notes.mjs 0.12.0 --md   # prints the version heading too
//
// Why not awk/sed in the workflow: `@semantic-release/changelog` varies the heading
// level by release type — `# [1.2.0]` for minor/major but `## [1.2.1]` for patch. A
// `^## \[` pattern therefore only matches patch releases, and every minor/major
// release lost its change list. Accepting `#{1,2}` here removes that dependency.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const version = process.argv[2];
if (!version) {
  console.error("Missing version: node scripts/release-notes.mjs <x.y.z>");
  process.exit(2);
}

const file = join(root, "CHANGELOG.md");
let changelog;
try {
  changelog = readFileSync(file, "utf8");
} catch {
  // No CHANGELOG.md yet (first ever release) — treat it as "no changes".
  process.exit(0);
}

/** A version heading: `# [x.y.z](link) (date)` — accepts one or two `#`. */
function isVersionHeading(line, v) {
  return new RegExp(`^#{1,2} \\[${v.replace(/\./g, "\\.")}\\]`).test(line);
}

/** Start of any other version entry — marks the end of the section being read. */
function isAnyVersionHeading(line) {
  return /^#{1,2} \[\d+\.\d+\.\d+[\w.-]*\]/.test(line);
}

const lines = changelog.split(/\r?\n/);
const start = lines.findIndex((l) => isVersionHeading(l, version));
if (start < 0) process.exit(0);

const body = [];
for (let i = start + 1; i < lines.length; i += 1) {
  if (isAnyVersionHeading(lines[i])) break;
  body.push(lines[i]);
}

// Drop leading/trailing blank lines and the runs of blank lines the generator leaves.
const cleaned = body
  .join("\n")
  .replace(/\n{3,}/g, "\n\n")
  .trim();

if (!cleaned) process.exit(0);
console.log(process.argv.includes("--md") ? lines[start] : "### Thay đổi");
console.log();
console.log(cleaned);
