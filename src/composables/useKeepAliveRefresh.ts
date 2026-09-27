import { onActivated } from "vue";

/**
 * Nạp lại dữ liệu mỗi khi màn được KeepAlive lấy lại từ cache.
 *
 * `<KeepAlive>` giữ state màn hình (bộ lọc, tab, con trang, dữ liệu đã nạp) khi
 * chuyển menu — đó là điều cần. Nhưng `setup()` chỉ chạy MỘT lần cho cả vòng
 * đời component, nên quay lại màn đã xem sẽ thấy dữ liệu cũ (ví dụ sửa xong
 * hóa đơn bên màn khác rồi quay lại vẫn là danh sách lúc mount).
 *
 * Hook `onActivated` của Vue được gọi mỗi lần component được chèn lại vào DOM
 * từ cache — đúng cái mốc "quay lại màn" cần can thiệp.
 *
 * Lần kích hoạt đầu tiên chính là lúc mount (màn đã tự nạp ở `setup`) nên bỏ
 * qua để không gọi trùng. Màn không nằm trong KeepAlive (màn in phiếu) không
 * bao giờ kích hoạt lại lần hai nên không cần xử lý riêng.
 *
 * Chỉ truyền hàm NẠP LẠI dữ liệu, không truyền phần khởi tạo một lần (ví dụ
 * đặt mặc định khoảng ngày) — nếu không, quay lại màn sẽ xoá luôn state mà
 * KeepAlive đang giữ.
 */
export function useKeepAliveRefresh(reload: () => unknown) {
  let mounted = false;
  onActivated(() => {
    if (!mounted) {
      mounted = true;
      return;
    }
    void reload();
  });
}
