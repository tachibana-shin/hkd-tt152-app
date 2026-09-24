# Kiểm thử tự động — HKD TT152

Hai tầng kiểm thử:

| Tầng                 | Vị trí                       | Lệnh                         | Phạm vi                                                                             |
| -------------------- | ---------------------------- | ---------------------------- | ----------------------------------------------------------------------------------- |
| **Rust (nghiệp vụ)** | `src-tauri/src/**/mod tests` | `cd src-tauri && cargo test` | Lõi tính toán tách khỏi command: FIFO, lương, tờ khai thuế, migration/seed, helpers |
| **E2E UI**           | `e2e/`                       | `./e2e/run_e2e.sh`           | App thật + WebKit inspector + **DB SQLite thật** làm nguồn sự thật                  |

## 1. Test Rust

```bash
cd src-tauri
cargo test            # unit + integration, chạy offline hoàn toàn
```

Lõi nghiệp vụ nằm ở các hàm `*_core(pool, …)` (không phụ thuộc Tauri `State`);
command chỉ là vỏ mỏng `require_role` + audit rồi gọi lõi. Test dùng
`SqlitePool` in-memory qua `init_database` (`src-tauri/src/test_support.rs`).

Phủ: `helpers` (băm mật khẩu, làm tròn), `test_support` (migration + seed admin),
`stock` (FIFO 1 lô/nhiều lô/thiếu hàng, nhập–xuất ghi sổ), `payroll` (công thức
khớp Excel, giảm trừ TNCN, idempotent theo kỳ), `accounting` (tờ khai, giảm 80%
GTGT, trừ doanh thu điều chỉnh), `hddt` (client + đồng bộ).

**Mock cổng HĐĐT cho test offline** (`hddt/mock_portal.rs`): HTTP server tự dựng
trên cổng ngẫu nhiên, trả payload đúng tên field của cổng thật (kể cả 500 kiểu
Spring khi `khmshdon` không phải số, 429 để test retry). Test đồng bộ chạy trọn
vòng quét → nhập kho → chống trùng mà **không cần mạng hay credentials**.

```bash
cd src-tauri
cargo test --lib live_probe_invoices -- --ignored   # test live (cần ../info.txt + IP VN)
cargo test --lib live_probe_page_size -- --ignored  # kiểm tra giới hạn size của cổng
```

## 2. Test E2E UI (Playwright)

```bash
bun run test:e2e            # offline: HĐĐT chạy với mock portal
E2E_HDDT_LIVE=1 bun run test:e2e   # HĐĐT chạy với cổng thật (cần info.txt + IP VN)
```

App thật (`bun run tauri dev` do Playwright tự khởi động, data dir riêng), trình
duyệt Chrome, Vite ở cổng riêng. Test HĐĐT **mặc định offline**: `mock-portal.ts`
đứng trên cổng 45733, app trỏ `base_url` vào đó qua `hddt_save_config` (nên không
cần sửa code app) — captcha dùng fixture `e2e/fixtures/captcha.svg` (ảnh thật của
cổng) để solver glyph giải được, đúng đường đăng nhập ngoài đời. Đặt
`E2E_HDDT_LIVE=1` để dùng cổng thật.

## 3. Test E2E app thật (WebKit inspector)

Chạy app **thật** (Tauri/webview), thao tác DOM qua WebKit remote inspector, và
**kiểm chứng kết quả bằng chính file SQLite của app** — không tin UI.

```bash
# Yêu cầu: đã có binary debug
(cd src-tauri && cargo build)

# Chạy toàn bộ luồng (tự khởi động Vite nếu cần, tự tạo app + dữ liệu mới)
./e2e/run_e2e.sh

# Chỉ chạy vài bước
./e2e/run_e2e.sh login warehouse product inbound outbound
```

Các bước có sẵn (thứ tự khuyến nghị):

| Bước         | Kiểm chứng (ngoài UI còn đối chiếu DB)                                                                                              |
| ------------ | ----------------------------------------------------------------------------------------------------------------------------------- |
| `login`      | sai mật khẩu bị từ chối (toast) → đăng nhập `admin/admin123`; DB có user admin                                                      |
| `warehouse`  | tạo kho `KHO1`, khách `KH001`, NCC `NCC001` ở Danh mục                                                                              |
| `product`    | tạo `SP001` (tên/ĐVT)                                                                                                               |
| `inbound`    | phiếu nhập `PN001`: ghi sổ + tạo lô kho                                                                                             |
| `outbound`   | phiếu xuất `PX001`: ghi sổ + trừ lô FIFO                                                                                            |
| `print`      | in phiếu: mở `/print/PN001`, đúng mẫu `01-VT`, dòng SP, tổng tiền, số tiền bằng chữ, có gọi lệnh in (đã stub); chứng từ sai báo lỗi |
| `invoice`    | hóa đơn nháp đúng khách + tổng tiền                                                                                                 |
| `cash`       | phiếu thu `PT` đúng số tiền                                                                                                         |
| `backup`     | "Tạo sao lưu" → danh sách có `hkd-backup-<yyyyMMdd-HHMMSS>.db`, file tồn tại > 4KB trên đĩa, audit `backup`                         |
| `employee`   | nhân viên `NV001` + lương HĐ/BH                                                                                                     |
| `attendance` | "Cả tháng" + lưu; DB có bản ghi công                                                                                                |
| `payroll`    | lập bảng lương, lấy công từ chấm công, lưu; số công & net_pay                                                                       |
| `import`     | gắn file Excel fixture → "Đọc file" nhận diện 2 dòng → nhập; DB thêm 2 bút toán `PN101`/`PX101`, audit `import`                     |
| `users`      | thêm người dùng `kho01` vai trò Kho; DB lưu vai trò + mật khẩu đã băm                                                               |
| `reports`    | mở được các trang sổ cái / kế toán / kho                                                                                            |
| `role`       | đăng xuất → đăng nhập `kho01`: menu ẩn Người dùng/Bảng lương/Kế toán HKD, hiện Nhập kho/Sản phẩm; đăng nhập lại admin               |

> Bước `import` cần fixture Excel tại `e2e/fixtures/nhap-lieu-e2e.xlsx`
> (`run_e2e.sh` tự sinh bằng SheetJS nếu thiếu và truyền qua `E2E_IMPORT_FILE`).

### Cách ly dữ liệu

Mỗi lần chạy dùng `XDG_DATA_HOME` riêng (`${TMPDIR:-/tmp}/hkd-tt152-e2e/data`),
nên **không đụng dữ liệu thật**. DB kiểm chứng:
`$E2E_DATA_HOME/git.shin.hdk-tt152-app/profiles/default/hkd.db`.

Biến môi trường hữu ích:

| Biến               | Mặc định                             | Ý nghĩa                                                                                |
| ------------------ | ------------------------------------ | -------------------------------------------------------------------------------------- |
| `E2E_DATA_HOME`    | `${TMPDIR:-/tmp}/hkd-tt152-e2e/data` | `XDG_DATA_HOME` riêng cho app test                                                     |
| `E2E_INSPECT_PORT` | `9223`                               | Cổng WebKit remote inspector                                                           |
| `E2E_LOG_DIR`      | `${TMPDIR:-/tmp}/hkd-tt152-e2e/log`  | Nơi ghi log Vite/app/Xvfb                                                              |
| `E2E_IMPORT_FILE`  | `e2e/fixtures/nhap-lieu-e2e.xlsx`    | File Excel cho bước `import`                                                           |
| `E2E_USE_XVFB`     | `1`                                  | `0` để dùng `DISPLAY` hiện tại thay vì Xvfb                                            |
| `E2E_XVFB_DISPLAY` | `:99`                                | Display Xvfb dùng cho app                                                              |
| `E2E_KEEP_VITE`    | `0`                                  | `1` để giữ Vite dev server lại sau khi chạy (mặc định script tự dọn Vite nó khởi động) |

### Cơ chế

- `run_e2e.sh`: khởi động Vite (nếu cần) → **Xvfb display riêng** → app với
  `XDG_DATA_HOME` mới → chờ inspector → chạy `e2e.py` → dọn app + Xvfb + Vite
  (chỉ dọn Vite nếu chính script khởi động; dùng `E2E_KEEP_VITE=1` để giữ lại).
- `insp.py` + `cdp.py`: transport tới WebKit inspector qua
  `Target.sendMessageToTarget` (webview không hỗ trợ `awaitPromise` → gọi nhiều
  `Runtime.evaluate` đồng bộ + `time.sleep`).
- `e2e.py`: `Sess` (js/wait/nav/set_file), helper DOM (`__E`), `check`/`wait_ok`/
  `wait_btn`, và các `step_*`.
- PrimeVue: `Select` overlay dùng `.p-select-option`; InputNumber **chỉ nhận giá
  trị khi phát `update:modelValue` qua Vue instance** (sự kiện `input` tổng hợp
  bị bỏ qua); dialog đóng vẫn còn trong DOM → `__E.dlg()` chọn dialog đang hiển
  thị (mới nhất).
- `Sess.nav` chờ vùng nội dung (`main > .flex-1`) **đổi khỏi trang cũ** rồi mới
  trả về (không `sleep` cố định): route component là lazy import nên `router-view`
  giữ trang cũ cho tới khi chunk resolve xong.
- Dev server: `vite.config.ts` khai báo `optimizeDeps.include` cho PrimeVue vì Vite
  không quét được import nằm trong `.vue`; thiếu bước này dev server pre-bundle
  muộn và **reload trang** giữa ca test.
- **Vì sao phải có Xvfb?** Trên Wayland/XWayland, khi phiên desktop bị khoá/che
  khuất thì WebKit báo `document.visibilityState = "hidden"` →
  `requestAnimationFrame` **không chạy** → transition của PrimeVue
  (Dialog/Toast) không bao giờ kết thúc, phần tử "kẹt" lại và chặn tương tác.
  Xvfb giữ webview luôn `visible` nên rAF chạy → test tất định.
- Gắn file cho `<input type=file>`: WebKitGTK **không hỗ trợ**
  `DOM.setFileInputFiles`; `Sess.set_file` đọc file → base64 → tạo `File` +
  `DataTransfer` trong trang → gán `input.files` → phát `change`.
- Các bước luôn đóng dialog (kể cả khi một kiểm tra thất bại) để không rò rỉ
  modal chặn các bước sau.

> Lưu ý: `run_e2e.sh` chạy app với `GDK_BACKEND=x11` và yêu cầu Vite dev server
> (port 1420) vì bản debug tải frontend từ đó.
