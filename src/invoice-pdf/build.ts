/**
 * Dựng HTML hoàn chỉnh của hóa đơn điện tử để xem trước và in ra PDF.
 *
 * `render-invoice-html.ts` chỉ ra HTML có placeholder (`/capture.ts`,
 * `TIMES.TTF`, `viewinvoice-bg.jpg`…); chỗ này thay chúng bằng nội dung thật:
 * script sinh QR + tách trang, 4 font Times và 2 ảnh — tất cả nhúng base64 nên
 * HTML tự chứa, mở được qua `blob:` URL và Chrome in được mà không cần mạng.
 *
 * Lần dựng đầu tiên phải tải 4,5MB font từ bundle nên `Promise` được nhớ lại;
 * kết quả theo từng hóa đơn cũng được cache (mỗi bản ~5MB nên chỉ giữ 3 bản
 * gần nhất).
 */
import bgUrl from "./assets/viewinvoice-bg.jpg?url";
import signUrl from "./assets/sign-check.jpg?url";
import timesBdUrl from "./assets/TIMESBD.TTF?url";
import timesBiUrl from "./assets/TIMESBI.TTF?url";
import timesSiUrl from "./assets/TIMESI.TTF?url";
import timesUrl from "./assets/TIMES.TTF?url";
import type { InvoiceData } from "./types";

/** Số HTML đã dựng giữ trong RAM — mỗi bản ~5MB nên chỉ giữ vài bản gần nhất. */
const MAX_CACHED = 3;
const htmlCache = new Map<string, string>();

interface Assets {
  capture: string;
  times: string;
  timesBd: string;
  timesBi: string;
  timesSi: string;
  bg: string;
  sign: string;
}

let assetsPromise: Promise<Assets> | undefined;

/** Nạp 1 file từ bundle rồi mã hoá base64 thành data URL (`fetch` là async). */
async function toDataUrl(url: string, mime: string): Promise<string> {
  const res = await fetch(url);
  if (!res.ok) {
    throw new Error(`Không tải được tài nguyên in PDF (${res.status}): ${url}`);
  }
  const bytes = new Uint8Array(await res.arrayBuffer());
  let binary = "";
  // Chunk 32KB để không vượt giới hạn tham số của `String.fromCharCode`.
  const CHUNK = 0x8000;
  for (let i = 0; i < bytes.length; i += CHUNK) {
    binary += String.fromCharCode(...bytes.subarray(i, i + CHUNK));
  }
  return `data:${mime};base64,${btoa(binary)}`;
}

function loadAssets(): Promise<Assets> {
  assetsPromise ??= (async () => {
    const [times, timesBd, timesBi, timesSi, bg, sign] = await Promise.all([
      toDataUrl(timesUrl, "font/ttf"),
      toDataUrl(timesBdUrl, "font/ttf"),
      toDataUrl(timesBiUrl, "font/ttf"),
      toDataUrl(timesSiUrl, "font/ttf"),
      toDataUrl(bgUrl, "image/jpeg"),
      toDataUrl(signUrl, "image/jpeg"),
    ]);
    const { default: capture } = await import("./assets/capture.js?raw");
    return { capture, times, timesBd, timesBi, timesSi, bg, sign };
  })();
  return assetsPromise;
}

/**
 * Dựng HTML tự chứa cho 1 hóa đơn. Trả về cùng chuỗi nếu `data.id` đã dựng
 * trước đó.
 */
export async function buildInvoiceHtml(data: InvoiceData): Promise<string> {
  const key = data.id ?? "";
  const cached = htmlCache.get(key);
  if (cached) return cached;

  const [assets, { renderInvoiceHtml }] = await Promise.all([
    loadAssets(),
    import("./render-invoice-html"),
  ]);

  // Dùng hàm thay vì chuỗi: nội dung script/base64 chứa `$` và
  // `String.replace` sẽ hiểu `$&`/`$$` là mẫu đặc biệt nếu truyền chuỗi.
  const html = renderInvoiceHtml(data)
    .replace(
      '<script type="module" src="/capture.ts">',
      () => `<script type="module">${assets.capture}</script>`,
    )
    .replace(/"?viewinvoice-bg\.jpg"?/g, () => `"${assets.bg}"`)
    .replace(/"?sign-check\.jpg"?/g, () => `"${assets.sign}"`)
    // Font dài trước. Bản gốc của tra-cuu-hd để 2 dòng sau cùng trỏ cả 2 vào
    // `TIMESI.TTF` nên `TIMESBI.TTF` (đậm-nghiêng) không bao giờ được nhúng.
    .replace(/"?TIMESBI\.TTF"?/g, () => `"${assets.timesBi}"`)
    .replace(/"?TIMESBD\.TTF"?/g, () => `"${assets.timesBd}"`)
    .replace(/"?TIMESI\.TTF"?/g, () => `"${assets.timesSi}"`)
    .replace(/"?TIMES\.TTF"?/g, () => `"${assets.times}"`);

  if (key) {
    if (htmlCache.size >= MAX_CACHED) {
      const oldest = htmlCache.keys().next().value;
      if (oldest !== undefined) htmlCache.delete(oldest);
    }
    htmlCache.set(key, html);
  }
  return html;
}
