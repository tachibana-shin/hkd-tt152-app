/**
 * Trạng thái viewport dùng chung cho toàn app (một nguồn sự thật, không để mỗi
 * component tự gắn listener). `main.ts` gọi `startViewportWatch()` một lần.
 *
 * Phân biệt hai tình huống mà Tailwind breakpoint không tự phân biệt được:
 *   • Cửa sổ DESKTOP bị thu nhỏ (pointer: fine) → vẫn giữ bảng dữ liệu, chỉ
 *     thu gọn sidebar và cho bảng cuộn ngang.
 *   • ĐIỆN THOẠI / tablet thật (pointer: coarse + màn hẹp) → bảng dữ liệu chuyển
 *     sang dạng thẻ, bấm tay dễ hơn nhiều so với cuộn ngang.
 */
const width = ref(typeof window === "undefined" ? 1280 : window.innerWidth);
const height = ref(typeof window === "undefined" ? 800 : window.innerHeight);
/** Thiết bị cảm ứng: con trỏ thật hoặc có chạm. */
const coarsePointer = ref(
  typeof window !== "undefined" &&
    (window.matchMedia?.("(pointer: coarse)").matches === true || navigator.maxTouchPoints > 0),
);

/** < 768px — điện thoại. */
const isMobile = ref(false);
/** 768–1023px — tablet, hoặc cửa sổ desktop bị thu nhỏ. */
const isTablet = ref(false);
/** Màn hình hẹp: sidebar thu gọn, hộp thoại viết lại chiều dọc. */
const isNarrow = ref(false);
/** Điện thoại/tablet thật + màn hẹp: bảng dữ liệu hiển thị dạng thẻ. */
const isCardMode = ref(false);

function measure() {
  const w = window.innerWidth;
  width.value = w;
  height.value = window.innerHeight;
  isMobile.value = w < 768;
  isTablet.value = w >= 768 && w < 1024;
  isNarrow.value = w < 1024;
  isCardMode.value = coarsePointer.value && w < 768;
}

let watching = false;
let pending = 0;

/** Gắn listener đo viewport (gọi một lần khi app khởi động). */
export function startViewportWatch() {
  if (watching || typeof window === "undefined") return;
  watching = true;
  measure();
  window.addEventListener("resize", () => {
    // Gộp nhiều sự kiện thành một lần đo — kéo cửa sổ phát sinh rất nhiều event.
    if (pending) return;
    pending = window.setTimeout(() => {
      pending = 0;
      measure();
    }, 100);
  });
  window.addEventListener("orientationchange", () => {
    measure();
  });
}

export function useViewport() {
  return { width, height, isMobile, isTablet, isNarrow, isCardMode, coarsePointer };
}
