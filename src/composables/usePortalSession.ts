import { api } from "@/db";
import type { HddtStatus } from "@/types";

/**
 * Phiên cổng HĐĐT — một nguồn sự thật cho MỌI màn.
 *
 * App tự đăng nhập lúc khởi động (`ensureSession`) và giữ token sống bằng
 * heartbeat; nếu solver captcha không giải được thì mở hộp thoại nhập tay.
 * Vì state nằm ở cấp module nên các màn được KeepAlive giữ lại vẫn thấy
 * đúng trạng thái, không cần nạp lại.
 *
 * Màn HĐĐT vẫn giữ phần cấu hình tài khoản / đổi mật khẩu.
 */

/** Kết quả một lần đảm bảo phiên. */
export type PortalEnsureResult = "logged_in" | "not_configured" | "need_manual" | "failed";

const status = ref<HddtStatus | null>(null);
/** Đã gọi xong `hddt_status` chưa — tránh hiện nút "đăng nhập" khi chưa biết. */
const statusLoaded = ref(false);
const loggingIn = ref(false);
const loggedIn = computed(() => !!status.value?.logged_in);

const lastError = ref("");
/** Đang chờ người dùng nhập captcha tay → tạm dừng đăng nhập tự động. */
const manualStopped = ref(false);
const manualVisible = ref(false);
const manualSvg = ref("");
const manualKey = ref("");
const manualValue = ref("");
const manualBusy = ref(false);
const manualError = ref("");
/** Mật khẩu cổng vừa được tự đổi — hiện đúng 1 lần cho người dùng ghi lại. */
const newPassword = ref("");

const RELOGIN_AHEAD_SECONDS = 3600;
let timer: ReturnType<typeof setInterval> | null = null;

function toast() {
  return useToast();
}

async function loadPortalStatus() {
  try {
    status.value = await api.hddtStatus();
  } catch {
    status.value = null;
  } finally {
    statusLoaded.value = true;
  }
}

/** Đăng nhập do người dùng bấm (có toast). */
async function loginPortal() {
  const t = toast();
  loggingIn.value = true;
  try {
    const res = await api.hddtLogin();
    if (res.status) status.value = res.status;
    if (res.ok) {
      lastError.value = "";
      t.add({ severity: "success", summary: "Đã đăng nhập cổng HĐĐT", life: 2500 });
    } else if (res.need_manual) {
      t.add({
        severity: "error",
        summary: "Cần nhập captcha",
        detail: "Mở màn HĐĐT để nhập captcha thủ công, rồi quay lại đây.",
        life: 5000,
      });
    } else {
      lastError.value = res.reason ?? "";
      t.add({
        severity: "error",
        summary: "Đăng nhập HĐĐT thất bại",
        detail: res.reason ?? "",
        life: 5000,
      });
    }
  } catch (e) {
    lastError.value = String(e);
    t.add({
      severity: "error",
      summary: "Đăng nhập HĐĐT thất bại",
      detail: String(e),
      life: 5000,
    });
  } finally {
    loggingIn.value = false;
  }
}

/**
 * Đảm bảo có phiên đăng nhập — gọi lúc app khởi động và khi màn HĐĐT được mở
 * mà phiên đã mất. Không bắn toast (tránh phiền lúc khởi động); lỗi để
 * `lastError` và kết quả trả về cho gọi bà quyết định có bật hộp thoại.
 */
async function ensureSession(): Promise<PortalEnsureResult> {
  if (loggingIn.value || manualStopped.value) {
    return loggedIn.value ? "logged_in" : "failed";
  }
  await loadPortalStatus();
  if (loggedIn.value) {
    lastError.value = "";
    return "logged_in";
  }
  if (!status.value?.configured) return "not_configured";

  loggingIn.value = true;
  try {
    const res = await api.hddtLogin();
    if (res.status) status.value = res.status;
    if (res.ok) {
      lastError.value = "";
      if (res.password_rotated && res.new_password) newPassword.value = res.new_password;
      return "logged_in";
    }
    lastError.value = res.reason ?? "";
    return res.need_manual ? "need_manual" : "failed";
  } catch (e) {
    lastError.value = String(e);
    return "failed";
  } finally {
    loggingIn.value = false;
  }
}

/** Lấy captcha mới + mở hộp thoại cho người dùng nhập tay. */
async function openManual() {
  manualStopped.value = true; // dừng tự đăng nhập để không đè captcha đang nhập
  try {
    const c = await api.hddtCaptcha();
    manualKey.value = c.key;
    manualSvg.value = c.svg;
    manualValue.value = "";
    manualError.value = "";
    manualVisible.value = true;
  } catch (e) {
    toast().add({
      severity: "error",
      summary: "Không lấy được captcha",
      detail: String(e),
      life: 5000,
    });
  }
}

async function submitManual() {
  const val = manualValue.value.trim();
  if (!val) {
    manualError.value = "Nhập mã captcha";
    return;
  }
  manualBusy.value = true;
  manualError.value = "";
  try {
    const res = await api.hddtLoginManual({ captchaKey: manualKey.value, captchaValue: val });
    if (res.status) status.value = res.status;
    if (res.ok) {
      manualVisible.value = false;
      manualStopped.value = false;
      lastError.value = "";
      toast().add({ severity: "success", summary: "Đã đăng nhập cổng HĐĐT", life: 2500 });
    }
  } catch (e) {
    manualError.value = String(e);
    await openManual(); // captcha sai → lấy mã mới
  } finally {
    manualBusy.value = false;
  }
}

/** Đóng hộp thoại mà chưa đăng nhập được → cho phép tự thử lại ở lượt sau. */
function onManualHide() {
  if (!loggedIn.value) manualStopped.value = false;
}

/** Cho phép tự đăng nhập lại (sau khi lưu cấu hình mới, hoặc người dùng đóng popup). */
function resumeAutoLogin() {
  manualStopped.value = false;
}

function dismissNewPassword() {
  newPassword.value = "";
}

/** Đăng xuất cổng (màn HĐĐT) — dùng chung để các màn khác thấy ngay. */
async function logoutPortal() {
  try {
    await api.hddtLogout();
    manualStopped.value = false;
    await loadPortalStatus();
    toast().add({ severity: "info", summary: "Đã đăng xuất cổng HĐĐT", life: 2000 });
  } catch (e) {
    toast().add({
      severity: "error",
      summary: "Không đăng xuất được",
      detail: String(e),
      life: 5000,
    });
  }
}

/** Nhịp giữ token: đạt hạn thì tự đăng nhập lại (chạy ở cấp app). */
async function tick() {
  if (manualVisible.value || manualStopped.value || loggingIn.value) return;
  try {
    await loadPortalStatus();
  } catch {
    return;
  }
  const s = status.value;
  if (!s?.configured) return;
  if (!s.logged_in || s.seconds_left <= RELOGIN_AHEAD_SECONDS) {
    await ensureSession();
  }
}

function startHeartbeat() {
  if (!timer) timer = setInterval(tick, 60_000);
}

function stopHeartbeat() {
  if (timer) {
    clearInterval(timer);
    timer = null;
  }
}

export function usePortalSession() {
  return {
    status,
    statusLoaded,
    loggedIn,
    loggingIn,
    /** Tên ngắn cho template: đang đăng nhập cổng. */
    busy: loggingIn,
    lastError,
    // captcha tay + mật khẩu mới (hộp thoại dùng chung cho cả app)
    manualVisible,
    manualSvg,
    manualValue,
    manualBusy,
    manualError,
    newPassword,
    loadPortalStatus,
    loginPortal,
    ensureSession,
    openManual,
    submitManual,
    onManualHide,
    resumeAutoLogin,
    dismissNewPassword,
    logoutPortal,
    startHeartbeat,
    stopHeartbeat,
  };
}
