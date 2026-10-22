# Việc còn dở theo yêu cầu 22/10/2026

Từ cuộc hội thoại: (1) nút tạo mới mặt hàng trong popup hint ở tab định mức; (2) HĐĐT —
lưu mật khẩu ra bảng riêng + chặn thử lại mật khẩu đã sai để không khóa tài khoản; (3)
console.warn ở dev mode. Làm dần, tick khi xong.

## 1. Nút "Tạo nhanh mặt hàng" trong popup dropdown của tab Định mức vật tư (F4) — ✅

Hiện trạng: tab **Products.vue → Định mức vật tư** có 2 ô `ProductSelect` (Thành phẩm
`bom-fg` w-72, Vật tư từng dòng) — chưa bật `allowCreate` nên gõ tên nhóm hàng mới không
tạo được; phải chuyển tab Danh mục để thêm, mất dòng đang đứng.

Cách làm:

- Bật `allow-create` cho cả 2 ô F4 (điều kiện quyền `auth.canStock`).
- Bắt `@create` → mở `ProductDialog` chế độ `quick` (vốn có: tự sinh mã, nhóm ngành,
  thuế), điền `preset-name` = tên vừa gõ.
- `@saved`: gắn mã hàng mới vào đúng ô đang đứng (thành phẩm của bảng thêm dở / dòng
  vật tư), danh mục đã refresh qua store.
- e2e: thêm test F4 — gõ tên mới → bấm dòng "Tạo nhanh" → dialog hiện → lưu → ô nhận mã.

Tham khảo: `ProductSelect` có sẵn dòng "Tạo nhanh" (`allowCreate` + `create` event,
footer overlay); `LineItemsEditor` dùng đúng pattern này (`onQuickCreate` + `ProductDialog quick`).

## 2. HĐĐT: nhật ký mật khẩu + chống khóa tài khoản — ✅

Bối cảnh: mật khẩu HĐĐT hiện lưu 1 giá trị trong `app_setting` (`hddt_password`);
đổi mật khẩu tự động thì ghi đè — nếu người dùng lỡ lưu sai mật khẩu (hoặc đổi xong
quên) → app cứ thử mật khẩu chết nhiều lần → cổng khóa tài khoản.

Yêu cầu của user:

- Lưu toàn bộ mật khẩu từng dùng ra **bảng riêng** ( đầy đủ, không mất mật khẩu còn có
  hiệu lực).
- Khi nhận **phản hồi "mật khẩu không đúng"** → ghi vào bảng và **từ chối thử lại** mật
  khẩu đó cho tới khi tài khoản đăng nhập **thành công** thì reset bảng (tránh bị khóa
  do sai nhiều lần).

Thiết kế:

- Migration `20261022001_hddt_password_journal`:
  ```sql
  CREATE TABLE hddt_password_journal (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    username TEXT NOT NULL,
    password TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'unknown',  -- unknown | ok | bad
    note TEXT NOT NULL DEFAULT '',          -- 'lưu tay' | 'tự đổi' | 'thử đăng nhập'
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    marked_at DATETIME,                     -- lúc đổi state gần nhất
    UNIQUE(username, password)
  );
  ```
- Ghi vào journal:
  - `hddt_save_config` (mật khẩu != rỗng): upsert (username, password), state giữ
    nguyên nếu đã có; lần đầu = `unknown`.
  - `perform_change_password`: sau khi cổng xác nhận đổi → upsert mật khẩu mới
    (`state='unknown'`, note `tự đổi`); mật khẩu cũ giữ nguyên dòng, không xóa.
  - Đăng nhập thành công (auto/manual): upsert mật khẩu đang dùng `state='ok'`, xóa
    mọi dòng `state='bad'` CỦA tài khoản = reset theo rule user.
- Chặn thử lại mật khẩu sai:
  - Phát hiện lỗi từ `authenticate` (`HĐĐT: {message}`): message chứa "mật khẩu"
    và KHÔNG chứa "captcha" → coi là sai mật khẩu → upsert mật khẩu đang thử
    `state='bad'`.
  - `do_full_login` + `hddt_login_manual`: TRƯỚC khi authenticate, nếu mật khẩu
    đang active có `state='bad'` → KHÔNG gọi cổng; trả kết quả
    `{ok:false, blocked_password:true, reason}` để UI nhắc vào màn HĐĐT sửa mật khẩu.
  - Auto-change bỏ qua gate này (phải có phiên sống mới đổi được).
- UI (màn HĐĐT):
  - Bảng nhật ký mật khẩu: thời gian tạo, trạng thái (chưa thử/đang OK/bị từ chối),
    ô hiện mật khẩu (xem được qua `hddt_get_password` style — có phân quyền), nút
    "Dùng" để đưa mật khẩu cũ về active (cứu thuốc khi lỡ lưu sai mật khẩu — không phải gõ lại).
  - Khi `blocked_password`: banner đỏ trên màn HĐĐT + hộp thoại captcha/đăng nhập
    đọc được lý do, gợi ý cập nhật mật khẩu trước khi thử.
- e2e/Rust test: mock portal có sẵn `authenticate` sai → thấy `state='bad'` + đăng
  nhập tiếp bị chặn; login thành công → bảng reset.

File đụng tới: `src-tauri/migrations/*`, `src-tauri/src/commands/hddt.rs`,
`src-tauri/src/hddt/client.rs` (phân loại message), `src/views/Hddt.vue`,
`src/db/index.ts`, e2e.

## 3. Console.warn dưới dev mode — ✅

Gặp 2 loại trong log trình duyệt (e2e bắt được, dev mode hiện trên console):

- `[Vue warn]: Non-function value encountered for default slot` — `AppDataTable.vue`
  render `h(IconField, …, [h(InputIcon…), h(InputText…)])` bằng array children; truyền slot là FUNCTION thì hết cảnh báo (PrimeVue đọc
  slot qua function).
- `[Vue warn]: Extraneous non-props attributes (data-testid)` — `TaxEntryForm.vue`
  có 2 root (`<SectionCard>` + `<Teleport to="body">`) nên `data-testid` trên usage
  không gắn được vào node gốc; gom Teleport vào trong SectionCard (single root) thì
  hết cảnh báo (Teleport vẫn đưa DOM ra body như cũ).

Rà thêm sau khi bật gate `[Vue warn]` trong e2e, tìm ra 2 lỗi console còn lại:

- `[Vue warn]: Slot "default" invoked outside of the render function` —
  `AppDataTable.vue` truyền slot `default`/`header` cho PrimeVue DataTable bằng
  HÀM THƯỜNG; DataTable gọi slot ngay trong `setup` → Vue cảnh báo. Đánh dấu
  slot như slot đã biên dịch (cờ `_n` của Vue) là hết cảnh báo, hành vi giữ
  nguyên (KHÔNG dùng `withCtx` — nó đổi ngữ cảnh render và làm hỏng render
  của DataTable).
- `POST /api/hddt_status -> 400` (console đỏ) — app gọi `hddt_status` lúc
  khởi động khi người dùng CÒN Ở màn đăng nhập; lệnh này cần vai trò người
  dùng nên cổng trả 400. Chỉ gọi `ensurePortalSession()` khi đã đăng nhập app
  (`App.vue`) — lúc đó cũng chẳng đăng nhập được cổng nên không mất gì.

## Kết quả triển khai

✅ Mục 1 — bật `allow-create` trên cả ô Thành phẩm + Vật tư ở tab Định mức; bấm dòng
"Tạo nhanh" mở `ProductDialog` quick (tương tự bảng dòng phiếu), lưu xong gán mã vào
đúng ô đang đứng (`bomQuickTarget` giữ tham chiếu object row). e2e mới có kiểm tra
tên mới → dialog → lưu → ô nhận hàng.

✅ Mục 2 — migration `20261022001_hddt_password_journal`, journal ghi theo
(username, password) state unknown/ok/bad; sai mật khẩu (`is_wrong_password_message`)
→ mark `bad`; `do_full_login` + `hddt_login_manual` có gate chặn authenticate khi
mật khẩu active `bad` (trả `blocked_password:true`, heartbeat 60s không đập cổng
nữa); thành công → reset dấu `bad` của tài khoản về unknown + mark `ok` cho mật
khẩu đang dùng. `save_config`/auto-change ghi nhật ký; `hddt_password_journal` +
`hddt_use_journal_password` (Dùng mật khẩu cũ) cho UI; màn HĐĐT có bảng nhật ký
(bấm mắt "Hiện mật khẩu" để soi), banner đỏ khi bị chặn; `hddt_status` trả
`active_password_bad`. Mock portal e2e từ chối riêng password `mock-wrong`.

✅ Mục 3 — `AppDataTable` đổi slot IconField sang dạng function (InputIcon nhận class
trực tiếp); `TaxEntryForm` gom Teleport bản in vào SectionCard để component còn 1 root;
slot `default`/`header` gửi sang PrimeVue được đánh dấu như slot đã biên dịch (cờ `_n`)
để hết cảnh báo "Slot invoked outside of the render function" (slot phải trả MẢNG vì
Vue không bọc mảng cho slot `_n`); app không gọi `hddt_status` khi người dùng còn ở màn
đăng nhập (bỏ 400 đỏ console). e2e "no JS/console errors" nay gate thêm cảnh báo Vue và
đi qua tab Kế toán HKD — nơi dựng `TaxEntryForm`.

Ngoài ra — sửa lỗi e2e sót từ commit trước (`461b0b4`): lệnh seed hóa đơn
(`hddt_sync_test_seed`) dùng CHUNG số hóa đơn "C26E2E-001" cho mọi lần seed, nên
guard "mỗi hóa đơn chỉ được nhập 1 phiếu" (thêm ở commit đó) chặn oan các test
seed sau đó trong cùng phiên ("Xem phiếu…", "Xem PDF hóa đơn…"). Nay mỗi lần
seed sinh số hóa đơn riêng (serial `C26E2E` giữ nguyên).
