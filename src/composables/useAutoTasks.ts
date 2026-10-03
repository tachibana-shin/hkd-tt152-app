import { api } from "@/db";
import { toIsoDate } from "@/utils/format";
import { useAppUpdater } from "./useAppUpdater";
import type { AppSettings } from "@/types";

/**
 * Tự kiểm tra cập nhật + tự quét hóa đơn HĐĐT ngay khi mở app, rồi lặp lại theo
 * khoảng thời gian đặt ở màn Cài đặt (Tự động khi mở ứng dụng).
 *
 * Nguyên tắc: **mọi lỗi đều nuốt**. Bản web không có updater, cổng HĐĐT chưa
 * đăng nhập, hồ sơ chưa khai ngày bắt đầu… — mở app không bao giờ được bật toast
 * đỏ vì một việc chạy nền. Và auto sync chỉ *quét* (kéo danh sách hóa đơn về
 * cache): việc nhập kho tạo phiếu vẫn là thao tác tay, nên tuyệt đối không sinh
 * bút toán khi app khởi động.
 */
export type AutoTaskNotice = { summary: string; detail?: string };

/** Timer + thông báo giữ ở module: app mở 1 lần, Settings/App cùng dùng chung. */
let timer: ReturnType<typeof setInterval> | null = null;
let notice: ((n: AutoTaskNotice) => void) | null = null;

/** Cài đặt chạy nền; lỗi khi đọc → coi như không bật gì. */
async function loadSettings(): Promise<AppSettings | null> {
  try {
    return await api.getAppSettings();
  } catch {
    return null;
  }
}

/** Quét cổng HĐĐT từ ngày bắt đầu dùng HĐĐT tới hôm nay. Lỗi thì hẹn lần sau. */
async function syncPortal(): Promise<void> {
  try {
    const cfg = await api.getBusinessConfig();
    await api.hddtSyncScan({
      from: cfg.hddt_start_date || undefined,
      to: toIsoDate(new Date()),
    });
  } catch {
    /* chưa đăng nhập cổng / chưa khai mốc bắt đầu / lỗi mạng → bỏ qua */
  }
}

/** Hỏi server có bản mới không — chỉ báo khi THẬT SỰ có, lỗi thì im lặng. */
async function checkUpdate(): Promise<void> {
  const updater = useAppUpdater();
  if (!updater.inTauri) return; // bản web mở bằng trình duyệt không có updater
  const update = await updater.checkUpdate();
  if (updater.error.value) {
    // Repo riêng tư trả 404, máy offline… là chuyện của việc chạy nền: xóa cờ
    // để màn Cài đặt không vô hiện dòng lỗi đỏ.
    updater.clearError();
    return;
  }
  if (update) {
    notice?.({
      summary: `Có bản cập nhật ${update.version}`,
      detail: "Mở Cài đặt → Cập nhật ứng dụng để tải và cài.",
    });
  }
}

async function run(settings?: AppSettings | null): Promise<void> {
  const s = settings ?? (await loadSettings());
  if (!s) return;
  if (s.auto_check_update) await checkUpdate();
  if (s.auto_sync_enabled) await syncPortal();
}

/**
 * Bật chạy nền: chạy ngay 1 lượt rồi hẹn lại theo cài đặt.
 *
 * `onNotice` do component truyền vào (`useToast()` cần injection context nên
 * composable tự lấy không được).
 */
export async function startAutoTasks(onNotice: (n: AutoTaskNotice) => void): Promise<void> {
  notice = onNotice;
  stopAutoTasks();
  const s = await loadSettings();
  if (!s || (!s.auto_check_update && !s.auto_sync_enabled)) return;
  void run(s);
  const minutes = Math.min(1440, Math.max(1, s.auto_sync_interval_min));
  timer = setInterval(() => void run(), minutes * 60_000);
}

export function stopAutoTasks(): void {
  if (timer) {
    clearInterval(timer);
    timer = null;
  }
}

/** Chạy lại theo cài đặt vừa lưu ở màn Cài đặt. */
export async function restartAutoTasks(): Promise<void> {
  await startAutoTasks(notice ?? (() => {}));
}
