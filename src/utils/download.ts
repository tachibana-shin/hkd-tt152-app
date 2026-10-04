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
