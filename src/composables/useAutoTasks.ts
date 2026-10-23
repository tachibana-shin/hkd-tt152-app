import { api } from "@/db";
import { toIsoDate } from "@/utils/format";
import { useAppUpdater } from "./useAppUpdater";
import { usePortalSession } from "./usePortalSession";
import type { AppSettings } from "@/types";

/**
 * Tự kiểm tra cập nhật + tự quét hóa đơn HĐĐT ngay khi mở app, rồi lặp lại theo
 * khoảng thời gian đặt ở màn Cài đặt (Tự động khi mở ứng dụng).
 *
 * Nguyên tắc: **mọi lỗi đều nuốt**. Bản web không có updater, cổng HĐĐT chưa
 * đăng nhập, hồ sơ chưa khai ngày bắt đầu… — mở app không bao giờ được bật toast
 * đỏ vì một việc chạy nền. Quét xong có thể tự TẠO PHIẾU NHẬP (cài đặt
 * `auto_sync_import`, mặc định bật) nhưng chỉ với hóa đơn đã có dòng hàng; hóa
 * đơn lỗi chi tiết vẫn nằm chờ để người dùng xử lý. Kết quả lần chạy được ghi
 * lại (`auto_sync_last_*`) để màn Đồng bộ hiển thị.
 *
 * Riêng CẬP NHẬT thì mở popup (không phải toast): gặp bản mới là hiện ngay
 * hộp thoại có ghi chú phát hành + nút "Tải và cài", người dùng bấm một cái là
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

/**
 * Quét cổng HĐĐT từ ngày bắt đầu dùng HĐĐT tới hôm nay, rồi (nếu bật) tạo
 * phiếu nhập cho hóa đơn đã có dòng hàng. Trả về tóm tắt để ghi "lần chạy gần
 * nhất" ở màn Đồng bộ.
 *
 * Quét xong MỚI nhập, và mỗi việc gọi tuần tự từng request (không chạy song
 * song) — cổng giới hạn tần suất, dồn request là bị khoá tài khoản.
 */
async function syncPortal(doImport: boolean): Promise<string> {
  // Lúc mở app, việc chạy nền có thể bắt đầu TRƯỚC khi App đăng nhập xong cổng.
  // Chỉ đọc `hddtStatus` rồi bỏ qua sẽ làm mất luôn lượt quét đầu tiên (chờ tới
  // chu kỳ sau, mặc định 15 phút). `ensureSession` gộp chung lượt đăng nhập đang
  // chạy nên ở đây chỉ cần chờ phiên; chưa đăng nhập được thì mới bỏ qua.
  const portal = usePortalSession();
  const ensured = await portal.ensureSession();
  if (ensured !== "logged_in") return "Chưa đăng nhập cổng HĐĐT — bỏ qua";
  const cfg = await api.getBusinessConfig();
  const scan = await api.hddtSyncScan({
    from: cfg.hddt_start_date || undefined,
    to: toIsoDate(new Date()),
  });
  let text = `Quét ${scan.days_scanned} ngày · HĐ mới ${scan.invoices_new}`;
  if (scan.details_failed > 0) text += ` · lỗi chi tiết ${scan.details_failed}`;
  if (doImport) {
    const imp = await api.hddtSyncImport({ ids: [] });
    text += ` · nhập kho ${imp.imported}`;
    if (imp.failed > 0) text += ` · lỗi ${imp.failed}`;
  }
  return text;
}

/** Ghi "lần chạy gần nhất" để màn Đồng bộ hiển thị; lỗi quyền/CSDL thì bỏ qua. */
async function saveLastRun(at: string, result: string): Promise<void> {
  try {
    await api.saveAppSettings({
      auto_sync_last_run_at: at,
      auto_sync_last_result: result,
    });
  } catch {
    /* chỉ là thông tin hiển thị — không được làm hỏng việc chạy nền */
  }
}

/** Hỏi server có bản mới — THẬT SỰ có thì mở popup, lỗi thì im lặng. */
async function checkUpdate(): Promise<void> {
  const updater = useAppUpdater();
  if (!updater.inTauri) return; // bản web mở bằng trình duyệt không có updater
  // Popup đang mở hoặc đang tải dở → đừng hỏi lại: hỏi lại sẽ reset popup
  // người dùng đang đọc và hỏi trùng khi họ chưa kịp bấm "Tải và cài".
  if (updater.prompt.value || updater.busy.value) return;
  const update = await updater.checkUpdate();
  if (updater.error.value) {
    // Repo riêng tư trả 404, máy offline… là chuyện của việc chạy nền: xóa cờ
    // để màn Cài đặt không vô hiện dòng lỗi đỏ.
    updater.clearError();
    return;
  }
  if (update) {
    // Thay toast "Mở Cài đặt → Cập nhật": mở popup ngay, ở đó có sẵn ghi chú
    // phát hành và nút "Tải và cài" làm luôn một lượt.
    updater.prompt.value = true;
  }
}

/** Timer có thể reo khi lượt trước chưa xong (quét nhiều ngày rất lâu) → bỏ
 *  lượt trùng, tránh dồn request và ghi đè kết quả đang chạy. */
let running = false;

async function run(settings?: AppSettings | null): Promise<void> {
  if (running) return;
  running = true;
  try {
    const s = settings ?? (await loadSettings());
    if (!s) return;
    if (s.auto_check_update) await checkUpdate();
    if (s.auto_sync_enabled) {
      const at = new Date().toISOString();
      let result: string;
      try {
        result = await syncPortal(s.auto_sync_import);
      } catch (e) {
        // Lỗi mạng/cổng chập chờn: không bật toast đỏ khi khởi động, nhưng ghi
        // lại để màn Đồng bộ thấy "lần chạy gần nhất" — biết nó CÓ chạy.
        result = `Lỗi: ${String(e).slice(0, 180)}`;
      }
      await saveLastRun(at, result);
    }
  } finally {
    running = false;
  }
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
