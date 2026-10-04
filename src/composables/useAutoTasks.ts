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
 *
 * Riêng CẬP NHẬT thì mở popup (không phải toast): gặp bản mới là hiện ngay
 * hộp thoại có nhật ký từng bước + nút "Tải và cài", người dùng bấm một cái là
 * xong — không phải tự nhớ đường vào màn Cài đặt.
 */

/** Timer giữ ở module: app mở 1 lần, Settings/App cùng dùng chung. */
let timer: ReturnType<typeof setInterval> | null = null;

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

/** Hỏi server có bản mới — THẬT SỰ có thì mở popup, lỗi thì im lặng. */
async function checkUpdate(): Promise<void> {
  const updater = useAppUpdater();
  if (!updater.inTauri) return; // bản web mở bằng trình duyệt không có updater
  // Popup đang mở hoặc đang tải dở → đừng hỏi lại: hỏi sẽ xóa nhật ký người
  // dùng đang đọc và hỏi trùng khi họ chưa kịp bấm "Tải và cài".
  if (updater.prompt.value || updater.busy.value) return;
  const update = await updater.checkUpdate();
  if (updater.error.value) {
    // Repo riêng tư trả 404, máy offline… là chuyện của việc chạy nền: xóa cờ
    // để màn Cài đặt không vô hiện dòng lỗi đỏ.
    updater.clearError();
    return;
  }
  if (update) {
    // Thay toast "Mở Cài đặt → Cập nhật": mở popup ngay, ở đó có sẵn nhật ký
    // các bước và nút "Tải và cài" làm luôn một lượt.
    updater.prompt.value = true;
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
 */
export async function startAutoTasks(): Promise<void> {
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
  await startAutoTasks();
}
