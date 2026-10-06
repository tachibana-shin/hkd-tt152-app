/** Chuẩn hoá việc lưu 1 file nhị phân ra máy người dùng.

 *  App chạy cả dạng desktop (Tauri) lẫn web (trình duyệt) nên đường lui khác nhau:
 *  - **Trình duyệt**: dựng `Blob` → `URL.createObjectURL` → `<a download>` — trình
 *    duyệt tự tải về (không hỏi chỗ lưu, đúng như hành vi web mọi người quen).
 *  - **Bản desktop (Tauri/WebKitGTK)**: webview KHÔNG xử lý `<a download>` trên
 *    `blob:` — bấm nút xuất file là lặng thinh, không hộp thoại, không lỗi (bug
 *    "nhấn xuất file không có gì xảy ra"). Thay bằng hộp thoại "Lưu file" GỐC của
 *    hệ điều hành (tauri-plugin-dialog) rồi ghi qua lệnh Rust `save_file`.
 *
 *  Trả `false` khi người dùng HỦY chọn chỗ lưu (chỉ bản desktop mới có) — caller
 *  không được toast "Đã tải" khi thấy `false`.
 */

import { invoke } from "@tauri-apps/api/core";
import { message, save } from "@tauri-apps/plugin-dialog";

// Chạy trong app Tauri hay trong trình duyệt thường? Cùng cách với `db/index.ts`.
const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** base64 trả từ lệnh Rust (PDF, ZIP…) → byte thô. */
export function base64ToBytes(base64: string): Uint8Array<ArrayBuffer> {
  return Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
}

/**
 * Byte → base64 để **gửi file dựng sẵn sang backend** (vd đóng gói báo cáo kỳ
 * thuế: tờ khai Excel, sổ kế toán). Chia khối `0x8000` vì `String.fromCharCode`
 * bị giới hạn số đối số — `apply`/spread trên mảng hàng trăm MB là sập tab.
 */
export function bytesToBase64(bytes: Uint8Array): string {
  let bin = "";
  const CHUNK = 0x8000;
  for (let i = 0; i < bytes.length; i += CHUNK) {
    bin += String.fromCharCode(...bytes.subarray(i, i + CHUNK));
  }
  return btoa(bin);
}

/** `Blob` → base64 (xem [`bytesToBase64`]). */
export async function blobToBase64(blob: Blob): Promise<string> {
  return bytesToBase64(new Uint8Array(await blob.arrayBuffer()));
}

/** Phần mở rộng thường (không dấu chấm), thường hoá chữ thường. */
function extensionOf(filename: string): string {
  const i = filename.lastIndexOf(".");
  return i > 0 ? filename.slice(i + 1).toLowerCase() : "";
}

/** Tên hiển thị bộ lọc trong hộp thoại hệ điều hành theo phần mở rộng. */
const EXT_LABEL: Record<string, string> = {
  xlsx: "Bảng tính Excel",
  docx: "Tài liệu Word",
  pdf: "Tài liệu PDF",
  zip: "Gói nén ZIP",
  xml: "Tệp XML",
  csv: "Tệp CSV",
};

/**
 * Lưu 1 file về máy. Trả `false` nếu người dùng hủy chọn chỗ lưu.
 *
 * Desktop: mở hộp thoại "Lưu file" gốc → ghi bằng lệnh Rust `save_file` (xem
 * `src-tauri/src/commands/files.rs`). Lỗi ghi (quyền/đĩa đầy) hiện hộp thoại
 * lỗi rồi trả `false` — không nuốt im lặng, cũng không ném lỗi để caller khỏi
 * toast nhầm "thành công"/"thất bại" lần hai.
 *
 * Web: tải qua `Blob` + `<a download>` (xem ghi chú đầu file). `revokeDelayMs`
 * giữ link đủ lâu cho trình duyệt tải xong — revoke sớm là hủy blob khi tải dở.
 */
export async function downloadBinary(
  filename: string,
  bytes: Uint8Array<ArrayBuffer>,
  mime: string,
  revokeDelayMs = 60_000,
): Promise<boolean> {
  if (!inTauri) {
    const url = URL.createObjectURL(new Blob([bytes], { type: mime }));
    const a = document.createElement("a");
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(url), revokeDelayMs);
    return true;
  }

  const ext = extensionOf(filename);
  const path = await save({
    defaultPath: filename,
    filters: ext ? [{ name: EXT_LABEL[ext] ?? ext.toUpperCase(), extensions: [ext] }] : undefined,
  });
  if (!path) return false;
  try {
    await invoke("save_file", { path, contents: bytesToBase64(bytes) });
  } catch (e) {
    // Đã chọn chỗ lưu rồi mà ghi lỗi — báo ngay bằng hộp thoại hệ điều hành.
    await message(`Không ghi được file:\n${String(e)}`, {
      title: "Lỗi lưu file",
      kind: "error",
    }).catch(() => undefined);
    return false;
  }
  return true;
}

/** Lưu ZIP gộp file XML hóa đơn (mime `application/zip`). */
export async function downloadZip(filename: string, base64: string): Promise<boolean> {
  return downloadBinary(filename, base64ToBytes(base64), "application/zip");
}

/** Lưu file đã dựng sẵn dạng `Blob` (Excel/Word từ thư viện tạo file). */
export async function downloadBlob(filename: string, blob: Blob): Promise<boolean> {
  return downloadBinary(filename, new Uint8Array(await blob.arrayBuffer()), blob.type);
}
