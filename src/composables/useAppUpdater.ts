import { getVersion } from "@tauri-apps/api/app";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

/**
 * Cập nhật OTA — bản cài từ Release của chính repo này.
 *
 * App chạy được ở HAI dạng: cửa sổ Tauri (có updater) và web server nội bộ
 * mở bằng Chrome (chỉ để dùng nhanh, không có updater). Vì vậy mọi lệnh gọi
 * đều bọc trong `inTauri` — bản web không có lệnh updater, gọi vào sẽ lỗi.
 *
 * Chạy nền phát hiện bản mới thì **mở popup** (cờ `prompt`) thay vì chỉ bắn
 * toast "mở Cài đặt đi": ở popup người dùng đọc ghi chú phát hành (release
 * body) rồi bấm một nút là tải + cài luôn.
 */

const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** Phiên bản đang chạy (từ tauri.conf.json của bản đã build). */
const current = ref("");
const checking = ref(false);
const installing = ref(false);
/** Tiến độ tải: đã tải / tổng (byte). */
const downloaded = ref(0);
const contentLength = ref(0);
const available = ref<{ version: string; notes: string } | null>(null);
const error = ref("");
/** Popup "Có bản cập nhật" — chạy nền thấy bản mới là tự bật lên. */
const prompt = ref(false);

const busy = computed(() => checking.value || installing.value);
const percent = computed(() =>
  contentLength.value > 0 ? Math.round((downloaded.value / contentLength.value) * 100) : 0,
);

async function readVersion() {
  if (!inTauri) return;
  try {
    current.value = await getVersion();
  } catch {
    current.value = "";
  }
}

void readVersion();

/**
 * Dev-only: xem trước popup cập nhật khi chạy `vite dev` (e2e dùng).
 *
 * `?update-preview=0.16.0` giả lập "có bản mới" kèm ghi chú phát hành mẫu để
 * xem được thân hộp thoại (Markdown) mà không cần bản cài + Release thật.
 * Bản cài build qua `vite build` → `import.meta.env.DEV === false` nên đoạn
 * này cùng mẫu ghi chú bị cắt khỏi bundle, người dùng không bao giờ thấy.
 */
const PREVIEW_NOTES = `## [0.16.0](https://github.com/tachibana-shin/hkd-tt152-app/compare/v0.15.1...v0.16.0) (2026-10-05)

### Tính năng mới

* **invoice:** tạo nhanh mặt hàng và thêm **tên khác** ngay trên dòng hóa đơn ([34d64b6](https://github.com/tachibana-shin/hkd-tt152-app/commit/34d64b6))
* **report:** ghi chú phát hành hiển thị dạng Markdown trong popup cập nhật

### Sửa lỗi

* **update:** bỏ khung nhật ký, chỉ còn ghi chú phát hành và nút \`Tải và cài\`

> Bản này là mẫu phục vụ xem trước khi phát triển.
`;

if (import.meta.env.DEV && typeof location !== "undefined") {
  const preview = new URLSearchParams(location.search).get("update-preview");
  if (preview) {
    available.value = { version: preview, notes: PREVIEW_NOTES };
    prompt.value = true;
  }
}

/** Hỏi server có bản mới không (không tải). */
async function checkUpdate() {
  if (!inTauri) return null;
  checking.value = true;
  error.value = "";
  available.value = null;
  downloaded.value = 0;
  contentLength.value = 0;
  try {
    const update = await check();
    if (update) {
      available.value = { version: update.version, notes: update.body ?? "" };
    }
    return update ?? null;
  } catch (e) {
    error.value = String(e);
    return null;
  } finally {
    checking.value = false;
  }
}

/** Tải + cài bản mới, báo tiến độ qua thanh xong thì khởi động lại app. */
async function installUpdate() {
  if (!inTauri || installing.value) return;
  installing.value = true;
  downloaded.value = 0;
  contentLength.value = 0;
  error.value = "";
  try {
    const update = await check();
    if (!update) {
      error.value = "Không có bản cập nhật mới.";
      return;
    }
    available.value = { version: update.version, notes: update.body ?? "" };
    await update.downloadAndInstall((event) => {
      switch (event.event) {
        case "Started":
          contentLength.value = event.data.contentLength ?? 0;
          break;
        case "Progress":
          downloaded.value += event.data.chunkLength ?? 0;
          break;
        default:
          break;
      }
    });
    await relaunch();
  } catch (e) {
    error.value = String(e);
  } finally {
    installing.value = false;
  }
}

/** Xóa cờ lỗi — việc kiểm tra chạy nền nuốt lỗi thay vì để đỏ ở màn Cài đặt. */
function clearError() {
  error.value = "";
}

export function useAppUpdater() {
  return {
    inTauri,
    current,
    available,
    checking,
    installing,
    busy,
    percent,
    downloaded,
    contentLength,
    error,
    prompt,
    checkUpdate,
    installUpdate,
    clearError,
  };
}
