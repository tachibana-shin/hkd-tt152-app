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
 * toast "mở Cài đặt đi": ở popup người dùng bấm một nút là tải + cài luôn,
 * và đọc được nhật ký từng bước qua `log`.
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
/** Nhật ký các bước cập nhật — hiện trong popup, mới nhất ở cuối. */
const log = ref<string[]>([]);
/** Popup "Có bản cập nhật" — chạy nền thấy bản mới là tự bật lên. */
const prompt = ref(false);

const busy = computed(() => checking.value || installing.value);
const percent = computed(() =>
  contentLength.value > 0 ? Math.round((downloaded.value / contentLength.value) * 100) : 0,
);

/** Ghi một bước vào nhật ký — popup hiển thị nguyên trạng, không tự đổi ý. */
function push(line: string) {
  log.value.push(line);
  // Trần: mỗi lần tải chỉ ghi khoảng chục dòng tiến độ, nhưng cứ giữ cho chắc.
  if (log.value.length > 200) log.value.splice(0, log.value.length - 200);
}

const mb = (bytes: number) => (bytes / 1024 / 1024).toFixed(1);

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
  downloaded.value = 0;
  contentLength.value = 0;
  log.value = [];
  push(`Hỏi server có bản mới (đang chạy ${current.value || "?"})…`);
  try {
    const update = await check();
    if (update) {
      available.value = { version: update.version, notes: update.body ?? "" };
      push(`Tìm thấy bản ${update.version} — chờ người dùng bấm "Tải và cài".`);
    } else {
      push("Không có bản mới — đang chạy bản mới nhất.");
    }
    return update ?? null;
  } catch (e) {
    error.value = String(e);
    push(`✗ Không hỏi được server — ${e}`);
    return null;
  } finally {
    checking.value = false;
  }
}

/** Tải + cài bản mới, báo tiến độ theo từng đoạn, xong thì khởi động lại app. */
async function installUpdate() {
  if (!inTauri || installing.value) return;
  installing.value = true;
  downloaded.value = 0;
  contentLength.value = 0;
  error.value = "";
  // Chỉ ghi tiếp nhật ký cũ (đang đọc dở giữa chừng) — không xóa ở đây.
  /** Mốc 10% cuối đã ghi — tiến độ đến chục phần trăm mới nói một lần. */
  let loggedPercent = 0;
  try {
    const update = await check();
    if (!update) {
      error.value = "Không có bản cập nhật mới.";
      push("Không tìm thấy bản mới nữa (có thể đã được cập nhật ở máy khác).");
      return;
    }
    available.value = { version: update.version, notes: update.body ?? "" };
    push(`Bắt đầu tải bản ${update.version}…`);
    await update.downloadAndInstall((event) => {
      switch (event.event) {
        case "Started":
          contentLength.value = event.data.contentLength ?? 0;
          push(
            contentLength.value > 0 ? `Tải ${mb(contentLength.value)} MB…` : "Tải file cập nhật…",
          );
          break;
        case "Progress":
          downloaded.value += event.data.chunkLength ?? 0;
          if (percent.value - loggedPercent >= 10) {
            loggedPercent = percent.value;
            push(
              `Đã tải ${percent.value}% — ${mb(downloaded.value)}/${mb(contentLength.value)} MB`,
            );
          }
          break;
        default:
          break;
      }
    });
    push("Đã tải xong — cài đặt rồi khởi động lại app…");
    await relaunch();
  } catch (e) {
    error.value = String(e);
    push(`✗ Cập nhật thất bại — ${e}`);
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
    log,
    prompt,
    checkUpdate,
    installUpdate,
    clearError,
  };
}
