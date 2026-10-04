/** Chuẩn hoá việc tải 1 file nhị phân về máy người dùng.

 *  App chạy cả dạng desktop (Tauri) lẫn web (trình duyệt) nên không dùng thư
 *  viện dialog của Tauri; đi theo cùng cơ chế với xuất Excel/Word: dựng
 *  `Blob` → `URL.createObjectURL` → `<a download>` (xem `utils/excel.ts`,
 *  `utils/bookDoc.ts`).
 */

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

/**
 * Tải 1 file về máy. `revokeDelayMs` giữ link đủ lâu cho trình duyệt tải xong —
 * revoke sớm là trình duyệt huỷ blob khi đang tải dở.
 */
export function downloadBinary(
  filename: string,
  bytes: Uint8Array<ArrayBuffer>,
  mime: string,
  revokeDelayMs = 60_000,
): void {
  const url = URL.createObjectURL(new Blob([bytes], { type: mime }));
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), revokeDelayMs);
}

/** Tải ZIP gộp file XML hóa đơn (mime `application/zip`). */
export function downloadZip(filename: string, base64: string): void {
  downloadBinary(filename, base64ToBytes(base64), "application/zip");
}

/** Tải file đã dựng sẵn dạng `Blob` (Excel/Word từ thư viện tạo file). */
export async function downloadBlob(filename: string, blob: Blob): Promise<void> {
  downloadBinary(filename, new Uint8Array(await blob.arrayBuffer()), blob.type);
}
