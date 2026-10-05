/**
 * Render Markdown → HTML cho ghi chú phát hành (release body) của popup
 * "Có bản cập nhật" và màn Cài đặt.
 *
 * Vì sao tự viết: ghi chú do semantic-release sinh (xem CHANGELOG.md) chỉ dùng
 * vài thứ — tiêu đề `##`, mục `###`, gạch đầu dòng `*`, in đậm, link `[t](u)`
 * — viết đúng chừng đó thì gọn hơn kéo cả thư viện markdown vào app.
 *
 * An toàn: MỌI chữ từ server đều escape trước, link chỉ nhận scheme vô hại
 * (http/https/mailto/#/đường dẫn thuần) nên `javascript:` không lọt được vào
 * `href`. HTML trả về chỉ gồm thẻ do mình tự sinh.
 */

/** Thoát ký tự HTML — nội dung release không bao giờ chèn được thẻ lạ. */
function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

/** Chỉ nhận link vô hại: http(s), mailto, mốc `#…`, hoặc đường dẫn không có `:`. */
function safeHref(raw: string): string | null {
  const url = raw.trim();
  if (!url) return null;
  if (/^(https?:\/\/|mailto:|#)/i.test(url)) return url;
  if (!url.includes(":") && !url.startsWith("//")) return url; // kiểu `v1.0.0...v1.1.0`
  return null;
}

/**
 * Format trong 1 dòng: `` `code` ``, **đậm**, *nghiêng*, `[nhãn](link)`.
 *
 * `code` được "park" trước để các quy tắc sau không đụng vào (dấu `*` trong
 * code vẫn là code), link park sau cùng để nhãn còn format được (`**[a](b)**`).
 * Dấu park dùng ký tự vùng riêng tư `\uE000` (không phải ký tự điều khiển)
 * chỉ do mình sinh ra — thay bằng HTML chính mình vừa build rồi giữ nguyên.
 */
function inline(text: string): string {
  const stash: string[] = [];
  const park = (html: string) => `\uE000${stash.push(html) - 1}\uE000`;

  let out = escapeHtml(text);
  out = out.replace(/`([^`]+)`/g, (_m, code: string) => park(`<code>${code}</code>`));
  out = out.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>");
  out = out.replace(/\*([^*\s][^*]*)\*/g, "<em>$1</em>");
  out = out.replace(/\[([^\]]*)\]\(([^()\s]*)\)/g, (_m, label: string, href: string) => {
    const url = safeHref(href);
    // Link không hợp lệ → giữ mỗi nhãn, không render thẻ <a>.
    return url ? park(`<a href="${url}">${label}</a>`) : label;
  });
  return out.replace(/\uE000(\d+)\uE000/g, (_m, i: string) => stash[Number(i)] ?? "");
}

/** 1 dòng của danh sách: khoảng trắng đầu dòng + dấu đầu dòng + nội dung. */
interface ItemLine {
  indent: number;
  ordered: boolean;
  text: string;
}

/** Cấp danh sách (có thể lồng nhau qua thụt lề 2 khoảng trắng). */
interface ListLevel {
  ordered: boolean;
  items: { text: string; kids: ListLevel[] }[];
}

const ITEM_RE = /^(\s*)([-*+]|\d+[.)])\s+(.*)$/;

/** Gom dòng thành cây danh sách lồng nhau (thụt lề ≥ 2 = danh sách con). */
function buildLevel(lines: ItemLine[], start: number): { level: ListLevel; next: number } {
  const own = Math.floor(lines[start].indent / 2);
  const level: ListLevel = { ordered: lines[start].ordered, items: [] };
  let i = start;
  while (i < lines.length) {
    const depth = Math.floor(lines[i].indent / 2);
    if (depth < own) break; // về cấp cha — dừng cho caller xử lý
    const parent = level.items[level.items.length - 1];
    if (depth > own && parent) {
      // List con phải nằm TRONG <li> vừa mở → gắn vào item cuối của cấp này.
      const sub = buildLevel(lines, i);
      parent.kids.push(sub.level);
      i = sub.next;
      continue;
    }
    level.items.push({ text: lines[i].text, kids: [] });
    i++;
  }
  return { level, next: i };
}

function renderLevel(level: ListLevel): string {
  const tag = level.ordered ? "ol" : "ul";
  const body = level.items
    .map((it) => `<li>${inline(it.text)}${it.kids.map(renderLevel).join("")}</li>`)
    .join("");
  return `<${tag}>${body}</${tag}>`;
}

/** Dòng mở khối mới (dùng để cắt đoạn văn khi gặp cấu trúc khác). */
function isBlockStart(line: string): boolean {
  return /^\s*(#{1,6}\s|>|```|[-*+]\s|\d+[.)]\s|(-{3,}|\*{3,}|_{3,})\s*$)/.test(line);
}

/**
 * Render toàn bộ Markdown sang HTML.
 *
 * Đủ dùng cho ghi chú phát hành: heading `#`–`######`, đoạn văn, danh sách
 * (có lồng), blockquote `>`,_hr `---`, code fence ``` và inline như trên.
 * Ảnh `![alt](url)` KHÔNG hỗ trợ (release sinh tự động không có ảnh) — giữ
 * nguyên chữ.
 */
export function renderMarkdown(md: string): string {
  const lines = (md ?? "").replace(/\r\n?/g, "\n").split("\n");
  const html: string[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];
    if (!line.trim()) {
      i++;
      continue;
    }

    // Code fence ``` … ```
    if (/^\s*```/.test(line)) {
      i++;
      const body: string[] = [];
      while (i < lines.length && !/^\s*```/.test(lines[i])) body.push(lines[i++]);
      if (i < lines.length) i++; // bỏ dòng đóng ```
      html.push(`<pre><code>${escapeHtml(body.join("\n"))}</code></pre>`);
      continue;
    }

    const heading = /^(#{1,6})\s+(.*)$/.exec(line);
    if (heading) {
      const level = heading[1].length;
      html.push(`<h${level}>${inline(heading[2])}</h${level}>`);
      i++;
      continue;
    }

    if (/^\s*(-{3,}|\*{3,}|_{3,})\s*$/.test(line)) {
      html.push("<hr />");
      i++;
      continue;
    }

    if (/^\s*>/.test(line)) {
      const body: string[] = [];
      while (i < lines.length && /^\s*>/.test(lines[i])) {
        body.push(lines[i++].replace(/^\s*>\s?/, ""));
      }
      const paras = body
        .filter((b) => b.trim())
        .map((b) => `<p>${inline(b)}</p>`)
        .join("");
      html.push(`<blockquote>${paras}</blockquote>`);
      continue;
    }

    if (ITEM_RE.test(line)) {
      const items: ItemLine[] = [];
      while (i < lines.length && ITEM_RE.test(lines[i])) {
        const m = ITEM_RE.exec(lines[i])!;
        items.push({
          indent: m[1].replace(/\t/g, "  ").length,
          ordered: /\d/.test(m[2]),
          text: m[3],
        });
        i++;
      }
      html.push(renderLevel(buildLevel(items, 0).level));
      continue;
    }

    // Đoạn văn: gộp tới dòng trống hoặc dòng mở khối khác.
    const para: string[] = [];
    while (i < lines.length && lines[i].trim() && !isBlockStart(lines[i])) para.push(lines[i++]);
    if (para.length) {
      html.push(`<p>${inline(para.join(" "))}</p>`);
    } else {
      // Không thể xảy ra (đã xử lý mọi khối ở trên) nhưng không được lặp vô hạn.
      html.push(`<p>${inline(lines[i])}</p>`);
      i++;
    }
  }

  return html.join("\n");
}
