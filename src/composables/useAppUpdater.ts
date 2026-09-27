import { getVersion } from "@tauri-apps/api/app";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

/**
 * Cập nhật OTA — bản cài từ Release của chính repo này.
 *
 * App chạy được ở HAI dạng: cửa sổ Tauri (có updater) và web server nội bộ
 * mở bằng Chrome (chỉ để dùng nhanh, không có updater). Vì vậy mọi lệnh gọi
 * đều bọc trong `inTauri` — bản web không có lệnh updater, gọi vào sẽ lỗi.
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

/** Hỏi server có bản mới không (không tải). */
async function checkUpdate() {
  if (!inTauri) return null;
  checking.value = true;
  error.value = "";
  available.value = null;
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

/** Tải + cài bản mới, báo tiến độ theo từng đoạn, xong thì khởi động lại app. */
async function installUpdate() {
  if (!inTauri) return;
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
    checkUpdate,
    installUpdate,
  };
}
