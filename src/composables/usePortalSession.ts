import { api } from "@/db";
import type { HddtStatus } from "@/types";

/**
 * Phiên cổng HĐĐT dùng chung cho các màn chỉ cần biết "đã đăng nhập chưa":
 * Tra cứu HĐĐT và Đồng bộ HĐĐT. Cả hai chỉ đọc trạng thái + bấm đăng nhập tự
 * (captcha do solver Rust giải). Việc cấu hình tài khoản, nhập captcha tay, đổi
 * mật khẩu và hẹn giờ tự đăng nhập lại vẫn nằm ở màn HĐĐT.
 *
 * State đặt ở cấp module (không phải trong hàm) nên MỌI màn dùng chung một
 * `status`: các màn được KeepAlive giữ lại không cần nạp lại, vẫn thấy ngay
 * "đã đăng nhập" sau khi màn HĐĐT vừa đăng nhập xong.
 */
const status = ref<HddtStatus | null>(null);
/** Đã gọi xong `hddt_status` chưa — tránh hiện nút "đăng nhập" khi chưa biết. */
const statusLoaded = ref(false);
const loggingIn = ref(false);
const loggedIn = computed(() => !!status.value?.logged_in);

async function loadPortalStatus() {
  try {
    status.value = await api.hddtStatus();
  } catch {
    status.value = null;
  } finally {
    statusLoaded.value = true;
  }
}

async function loginPortal() {
  const toast = useToast();
  loggingIn.value = true;
  try {
    const res = await api.hddtLogin();
    if (res.status) status.value = res.status;
    if (res.ok) {
      toast.add({ severity: "success", summary: "Đã đăng nhập cổng HĐĐT", life: 2500 });
    } else if (res.need_manual) {
      toast.add({
        severity: "error",
        summary: "Cần nhập captcha",
        detail: "Mở màn HĐĐT để nhập captcha thủ công, rồi quay lại đây.",
        life: 5000,
      });
    } else {
      toast.add({
        severity: "error",
        summary: "Đăng nhập HĐĐT thất bại",
        detail: res.reason ?? "",
        life: 5000,
      });
    }
  } catch (e) {
    toast.add({
      severity: "error",
      summary: "Đăng nhập HĐĐT thất bại",
      detail: String(e),
      life: 5000,
    });
  } finally {
    loggingIn.value = false;
  }
}

export function usePortalSession() {
  return { status, statusLoaded, loggingIn, loggedIn, loadPortalStatus, loginPortal };
}
