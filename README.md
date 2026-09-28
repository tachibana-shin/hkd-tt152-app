# 🧾 Phần mềm Kế toán Hộ kinh doanh — TT 152/2021/TT-BTC

Ứng dụng desktop kế toán cho **Hộ kinh doanh / Cá nhân kinh doanh**, bám theo Thông tư
152/2021/TT-BTC và bộ mẫu Excel HKD (nhật ký chung, sổ quỹ, NXT, bảng lương, chấm công, tờ khai thuế GTGT/TNCN, bảng kê giảm thuế GTGT, phiếu nhập/xuất kho, hóa đơn điện tử).

> [!WARNING]
> **⚠️ Read this before using or reusing the code.**
>
> **Regulatory scope.** This application implements the record-keeping, e-invoice and
> tax-reporting requirements of the Ministry of Finance's **Circular 152** (Vietnam) for
> household businesses (hộ kinh doanh / cá nhân kinh doanh). It is an independent tool —
> it is not affiliated with, endorsed by, or a substitute for any decision of the
> Ministry of Finance, the General Department of Taxation or any e-invoice service
> provider, and it does not guarantee that a particular business is compliant. You stay
> responsible for the accuracy of your records and for meeting Vietnamese tax deadlines.
>
> **Not open source.** The code here is **source-available** and published for
> transparency and contributions only. You may read it, fork/clone it, and run the
> development commands to submit a contribution. You may **not** build distributable
> installers, reverse engineer it, or redistribute it in any form. See [LICENSE](LICENSE).

---

## ✨ Tổng quan

- 🗂️ **Một bảng nhật ký trung tâm** (`journal_entry`): mọi nghiệp vụ (nhập/xuất kho, thu/chi,
  hóa đơn, lương…) đều ghi vào đây kèm cặp **Nợ/Có** và **tỷ lệ thuế**, phục vụ mọi sổ sách & tờ khai.
- 🧮 **Tự động tính thuế** theo nhóm ngành: tờ khai theo kỳ, ghép **giảm thuế GTGT**
  (tỷ lệ giảm 80%, thuế giảm = doanh thu × tỷ lệ × 20%).
- 💰 **Bảng lương & BHXH/TNCN**: công thức khớp bộ mẫu Excel, tự ghi phiếu chi lương.
- 🕒 **Chấm công** theo ngày, khớp ký hiệu sheet _Cham Cong_, nạp thẳng vào bảng lương.
- 📦 **Kho theo lô (FIFO)**: nhập kho tạo lô, xuất kho trừ lô, kiểm kê điều chỉnh.
- 🧾 **Hóa đơn & HĐĐT**: lập hóa đơn nháp, liên kết hóa đơn điện tử (connector mô phỏng cục bộ).
- 👥 **Đa người dùng & phân quyền**: `admin` / `ketoan` / `kho` / `xem`; kiểm tra vai trò **ở backend**.
- 🏠 **Đa hồ sơ HKD** trên một máy (mỗi hộ = một file DB riêng), chuyển hồ sơ ngay khi đang chạy.
- 🛟 **Sao lưu / khôi phục** và **audit log** ghi dấu vết mọi thao tác ghi.

---

## 🧩 Tính năng theo màn hình

| Màn hình               | Route               | Nghiệp vụ chính                                                                    |
| ---------------------- | ------------------- | ---------------------------------------------------------------------------------- |
| 📊 Tổng quan           | `/`                 | Bảng điều khiển nhanh                                                              |
| 🛒 Danh mục sản phẩm   | `/products`         | Hàng hóa/dịch vụ, giá bán–giá vốn, VAT, tồn tối thiểu                              |
| 📒 Danh mục tài khoản  | `/accounts`         | Hệ thống tài khoản + số dư đầu kỳ                                                  |
| 🏷️ Đối tác & kho       | `/catalogs`         | Kho, khách hàng, nhà cung cấp, nhóm ngành                                          |
| 💵 Phiếu thu / chi     | `/cash`             | Phiếu thu (`PT`) / chi (`PC`), hạch toán Nợ/Có                                     |
| 📥 Nhập kho            | `/inbound`          | Phiếu nhập kho (`PN`), tạo lô FIFO                                                 |
| 📤 Xuất kho / Bán hàng | `/outbound`         | Phiếu xuất kho (`PX`), trừ lô FIFO, kiểm tra tồn                                   |
| 📦 Tồn kho             | `/inventory`        | NXT theo lô, phiếu kiểm kê điều chỉnh                                              |
| 🧾 Hóa đơn             | `/invoices`         | Lập hóa đơn nháp, chi tiết, liên kết HĐĐT                                          |
| 🖨️ In phiếu            | `/print/:voucherNo` | In PNK `01-VT` / PXK `02-VT`, số tiền bằng chữ                                     |
| 🧮 Kế toán HKD         | `/accounting`       | Tờ khai theo nhóm ngành, bảng kê giảm thuế GTGT, tờ khai theo kỳ, sổ sách, sao lưu |
| 📚 Sổ nhật ký chung    | `/ledger`           | Sổ NKC theo bộ lọc                                                                 |
| 💼 Bảng lương          | `/payroll`          | Nhân viên, lập bảng lương, BHXH/TNCN, chi lương                                    |
| 🕒 Chấm công           | `/attendance`       | Lưới công theo ngày, tổng công theo kỳ                                             |
| 📄 Nhập liệu Excel     | `/import`           | Nhập khối dữ liệu sheet `NHAP LIEU`                                                |
| 📝 Nhật ký hoạt động   | `/audit`            | Audit log (ai, khi nào, thao tác gì)                                               |
| 👥 Người dùng          | `/users`            | Tài khoản & phân quyền                                                             |
| 🏠 Hồ sơ HKD           | `/profiles`         | Tạo/đổi tên/xóa/chuyển hồ sơ kinh doanh                                            |

---

## 📱 Giao diện thích ứng (responsive)

App dùng được cả trên cửa sổ nhỏ lẫn trên điện thoại/tablet (mở bằng trình duyệt
cùng mạng LAN — xem mục _Dữ liệu & tài khoản mặc định_). Hai tình huống được xử lý
khác nhau, phân biệt bằng **loại con trỏ** chứ không chỉ độ rộng màn hình
(`src/composables/useViewport.ts`):

| Tình huống                                                 | Sidebar                | Bảng dữ liệu                                                 | Form / hộp thoại                  |
| ---------------------------------------------------------- | ---------------------- | ------------------------------------------------------------ | --------------------------------- |
| **Cửa sổ desktop bị thu nhỏ**                              | Thu gọn thành icon     | **Giữ nguyên bảng**, cuộn ngang                              | Lưới 2–4 cột, hộp thoại như cũ    |
| **Điện thoại / tablet thật** (`pointer: coarse` + màn hẹp) | Ngăn kéo trượt, nút ☰ | **Chuyển thành thẻ**: mỗi dòng một thẻ, cột → nhãn + giá trị | 1 cột, hộp thoại gần hết màn hình |

Thẻ được **sinh tự động từ chính các `<Column>`** mà màn hình đã khai báo
(`AppDataTable`), nên không phải viết lại từng màn: ô có template `#body` vẫn hiển thị
đúng, và nút thao tác của dòng chuyển xuống cuối thẻ. Lưu ý: sửa ô trực tiếp bằng
double-tap là tính năng của bảng, nên trên điện thoại hãy dùng nút **Sửa** trên thẻ.

---

## 🏗️ Kiến trúc & công nghệ

```
┌─────────────────────────────┐       invoke       ┌──────────────────────────────┐
│  Vue 3 + TypeScript         │  ───────────────▶  │  Tauri 2 (Rust)              │
│  PrimeVue · Tailwind · Pinia │  ◀───────────────  │  sqlx · SQLite · migrations  │
└─────────────────────────────┘      commands       └──────────────────────────────┘
       │   ▲                                              │
       │   │ fetch /api/<cmd> (cùng JSON, camelCase)      │ web server local
       ▼   │                                              ▼
┌─────────────────────────────┐     127.0.0.1:45731      ┌──────────────────────────────┐
│  Trình duyệt (Chrome)       │  ─────────────────────▶  │  axum (nhúng trong app)      │
└─────────────────────────────┘   serve dist nhúng       └──────────────────────────────┘
```

- **Backend (Rust):** module hóa theo nghiệp vụ (`auth`, `stock`, `payroll`, `accounting`,
  `invoice`, `profile`, `hddt`, …). Command mỏng (`require_role` + audit) gọi **lõi `*_core(pool, …)`**
  để dễ test. Migration SQL trong `src-tauri/migrations/`.
- **Frontend (Vue 3 `<script setup>` + TS strict):** Pinia store, Vue Router, PrimeVue (đăng ký toàn cục),
  Tailwind; component dùng chung trong `src/components/`, tiện ích định dạng trong `src/utils/`.
- **Dữ liệu:** mỗi hồ sơ HKD = một file SQLite riêng; sao lưu dạng `hkd-backup-<yyyyMMdd-HHMMSS>.db`.

---

## 📁 Cấu trúc thư mục

```
hkd-tt152-app/
├─ src/                      # Frontend Vue 3
│  ├─ views/                 #   Màn hình theo route
│  ├─ components/            #   Component dùng chung (AppDialog, FormField, SectionCard, …)
│  ├─ stores/                #   Pinia store (auth, catalog, stock, business, profile, …)
│  ├─ db/                    #   Wrapper invoke Tauri có kiểu
│  ├─ router/                #   Định tuyến + tiêu đề trang
│  ├─ utils/                 #   Định dạng số/tiền tệ, xuất Excel
│  └─ types/                 #   Kiểu dữ liệu dùng chung
├─ src-tauri/                # Backend Rust
│  ├─ src/                   #   Module nghiệp vụ + command
│  └─ migrations/            #   Migration SQLite
├─ e2e/                      # Bộ kiểm thử E2E (app thật + WebKit inspector + DB thật)
├─ docs/                     # Tài liệu thiết kế chi tiết
└─ *.xlsx                    # Bộ mẫu Excel HKD (TT152) làm nguồn đối chiếu
```

---

## ✅ Yêu cầu môi trường

- **Bun** (hoặc npm) và **Node.js**
- **Rust** (stable) + **Cargo**
- Phụ thuộc hệ thống của Tauri 2 (WebKitGTK trên Linux) — xem
  [hướng dẫn Tauri](https://tauri.app/start/prerequisites/).
- Kiểm thử E2E (tuỳ chọn): `Xvfb`, `python3`, trình điều khiển webkit.

---

## 🚀 Cài đặt & chạy

```bash
# 1) Cài phụ thuộc frontend
bun install

# 2) Chạy chế độ phát triển (Vite + Tauri, tự mở cửa sổ app)
bun run tauri dev

# Hoặc chỉ chạy frontend (Vite dev server tại http://localhost:1420)
bun run dev
```

Đóng gói / kiểm tra kiểu:

```bash
bun run build          # vue-tsc --noEmit + vite build
cd src-tauri && cargo build          # build app (debug)
cd src-tauri && cargo build --release
```

### 🌐 Dùng app qua trình duyệt (Chrome)

App có sẵn một **web server local** (chỉ `127.0.0.1`) vừa serve frontend vừa phơi REST API:

- **Bật/tắt:** bản production (release) **mặc định TẮT** vì lý do bảo mật — bật trong
  **Cài đặt → "Dùng app qua trình duyệt (Chrome)"** (toggle áp dụng ngay, không cần khởi động
  lại; chỉ thao tác được từ **cửa sổ app desktop**). Bản dev chạy `bun run tauri dev` thì
  **luôn bật**.
- **Cổng mặc định:** `45731` (nếu bận, tự dò cổng trống kế tiếp — xem log `[web]`).
- **Cách dùng:** app đang chạy → mở Chrome vào `http://127.0.0.1:45731`.
- **Cùng dữ liệu & phiên đăng nhập:** trình duyệt gọi đúng các command backend (REST
  `/api/<cmd>`) qua lớp transport chung `src/db/index.ts` — trong app dùng IPC, trong
  trình duyệt dùng `fetch`, tự chọn theo môi trường (`__TAURI_INTERNALS__`).
- Ở **dev** cũng dùng được: `bun run tauri dev` (chạy app) rồi mở `http://localhost:1420`
  (Vite proxy `/api` về cổng web server của app).
- **Ngữ nghĩa HTTP:** `200` thành công; `400` lỗi nghiệp vụ/tham số; `401` sai tài khoản/mật khẩu
  khi đăng nhập; `404` lệnh không hỗ trợ; `500` chỉ còn dành cho lỗi thật sự của server.
- **Nhật ký yêu cầu:** mỗi request web được ghi vào `web.log` (thư mục dữ liệu app, cạnh `hkd.db`)
  — gồm thời gian, lệnh, mã HTTP và thông báo lỗi (không ghi mật khẩu).

---

## 🧪 Kiểm thử

| Tầng                    | Vị trí                       | Lệnh                         | Phạm vi                                                                                                                                        |
| ----------------------- | ---------------------------- | ---------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| **Rust (nghiệp vụ)**    | `src-tauri/src/**/mod tests` | `cd src-tauri && cargo test` | FIFO, lương, tờ khai thuế, migration/seed, helper                                                                                              |
| **E2E UI (Playwright)** | `e2e/playwright/`            | `bun run test:e2e`           | App thật qua **Chrome** trên cổng riêng + **DB cô lập** (không đụng dữ liệu thật): login, điều hướng, render dữ liệu, log console sạch, logout |
| **E2E UI (WebKit đen)** | `e2e/`                       | `./e2e/run_e2e.sh`           | App thật + WebKit inspector + **DB SQLite thật** làm nguồn sự thật                                                                             |

```bash
cd src-tauri && cargo test        # test lõi nghiệp vụ

bun run test:e2e                  # E2E Playwright trên Chrome (tự chạy tauri dev: Vite 1421 + web server 45821, DB riêng)
bun run test:e2e:headed           # chạy có cửa sổ Chrome để xem
bun run test:e2e:debug            # chạy từng test, dừng chờ thao tác (Playwright Inspector)
bun run test:e2e:report           # mở báo cáo HTML (kể cả trace của test fail)

./e2e/run_e2e.sh                  # chạy toàn bộ luồng E2E (16 bước)
./e2e/run_e2e.sh login product    # chỉ chạy vài bước
```

> Chi tiết 16 bước, biến môi trường và cơ chế Xvfb: xem [`e2e/README.md`](e2e/README.md).

### 🤖 E2E bằng Playwright (Chrome)

Vì app đã chạy được trên Chrome (web server nhúng), E2E dùng Playwright điều khiển
**Chrome hệ thống** (không tải Chromium riêng). **Không cần build trước** — mỗi lần
chạy test, Playwright tự khởi động **một instance `bun run tauri dev` riêng** cô lập:

- `HKD_WEB_PORT=45821` — web server nhúng của app ở cổng riêng, không đụng app người
  dùng (45731). `HKD_DATA_DIR=<thư mục tạm>` — DB hoàn toàn cô lập; seed sẵn thông tin
  HKD + 1 sản phẩm mẫu để test render dữ liệu thật. Đường dẫn DB của lần chạy gần nhất
  nằm ở `/tmp/hkd-e2e-dir`.
- `VITE_PORT=1421` + proxy `/api` → `http://127.0.0.1:45821` — Vite dev server ở cổng
  riêng, không đụng `bun run tauri dev` của người dùng (1420).
- Playwright chờ `GET /api/health` (qua proxy Vite → app) trả `200` mới bắt đầu test —
  đảm bảo cả Vite lẫn app đã sẵn sàng, không bị race lúc boot.

Debug test khi fail: `bun run test:e2e:debug` (Playwright Inspector), `--headed` để xem
trực tiếp, hoặc mở trace trong `bun run test:e2e:report`.

---

## 💾 Dữ liệu & tài khoản mặc định

- 🔑 **Tài khoản quản trị mặc định:** `admin` / `admin123` (đổi được trong màn _Người dùng_).
- 📂 **Vị trí dữ liệu (Linux):** `${XDG_DATA_HOME}/git.shin.hdk-tt152-app/profiles/<hồ sơ>/hkd.db`
  (mặc định `~/.local/share/...`); bản sao lưu nằm ở thư mục `backups/` cùng cấp.
- 👤 **Vai trò:** `admin` (toàn quyền), `ketoan` (kế toán), `kho` (thủ kho), `xem` (chỉ xem — mặc định).
- 🕵️ Mọi thao tác ghi đều được ghi **audit log** kèm tên người dùng.

---

## 🚀 CI · Đóng gói · Cập nhật OTA

### Kiểm tra tự động (`.github/workflows/ci.yml`)

Mỗi lần push/PR chạy 4 việc song song:

| Job        | Kiểm tra                                                                                |
| ---------- | --------------------------------------------------------------------------------------- |
| `frontend` | `vue-tsc` (kiểu dữ liệu) · `oxlint` · `oxfmt --check` · phiên bản khớp tag · build Vite |
| `rust`     | `cargo fmt --check` · `cargo clippy --all-targets -- -D warnings` · `cargo test --lib`  |
| `sqlx`     | bộ cache truy vấn `.sqlx` còn khớp code (`cargo sqlx prepare --check`)                  |
| `e2e`      | Playwright offline đầy đủ (dựng app dưới Xvfb)                                          |

Chạy đúng bộ kiểm tra đó ngay trên máy:

```bash
bun run ci             # fmt + lint + typecheck + test Rust + check sqlx
bun run version:check  # 4 file manifest cùng phiên bản và khớp tag v* mới nhất
bunx playwright test
```

Husky chặn lỗi trước khi commit: `pre-commit` chạy fmt/lint/typecheck/test Rust,
`pre-push` kiểm tra `.sqlx` (bỏ qua kiểm tra sqlx nếu máy chưa có `sqlite3` CLI).

### Đóng gói đa nền tảng (`.github/workflows/release.yml`)

Commit vào `main` → [semantic-release](https://semantic-release.gitbook.io/) tính bản
mới rồi build 6 target trong một Release nháp:

- Windows x64 (NSIS + MSI), Windows ARM64 (NSIS)
- macOS Intel + Apple Silicon (.dmg)
- Linux x64 (AppImage + .deb), Linux ARM64 (AppImage)

Cách tính phiên bản: `feat:` → minor, `fix:` → patch, `!` trong tiêu đề hoặc
`BREAKING CHANGE` → major. Nhãn bản phải theo
[Conventional Commits](https://www.conventionalcommits.org/).

> `docs:`, `chore:`, `test:` **không** kích hoạt bản mới — workflow sẽ chạy
> semantic-release rồi bỏ qua luôn bước đóng gói, nên những commit đó không tốn
> runner nào. Commit **không ghi kiểu** (kiểu `"Update README"`) vẫn ra patch theo
> `releaseRules` trong [`.releaserc.json`](.releaserc.json).

**Mỗi lần release, semantic-release tự động:**

1. tạo tag `v<phiên bản>` và commit `chore(release): <phiên bản> [skip ci]`;
2. ghi phiên bản vào **cả 4 file manifest** — `package.json`,
   `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`
   (script `scripts/set-version.mjs`) nên bundle luôn khớp tag;
3. cập nhật [`CHANGELOG.md`](CHANGELOG.md) bằng ghi chú tiếng Việt theo từng nhóm
   commit (Tính năng mới / Sửa lỗi / Hiệu năng / Tài liệu / …);
4. tạo Release ở trạng thái **draft** kèm ghi chú trên.

Sau đó 6 job đóng gói gắn asset vào release đó, và job `publish` mới **công khai**
nó (kèm mục tương ứng trong `CHANGELOG.md` trong phần mô tả). Nếu một target hỏng
thì release **vẫn ở dạng nháp** — người dùng và updater không nhìn thấy bản thiếu
file; sửa xong chạy lại workflow là tiếp.

Cấu hình nằm ở [`.releaserc.json`](.releaserc.json); 7 plugin đã khai báo trong
`devDependencies` nên CI cài đúng phiên bản đã ghim.

> ⚠️ Tên file cấu hình phải là `.releaserc.json` (**có dấu chấm ở đầu**).
> semantic-release 25 dùng cosmiconfig nên không đọc `releaserc.json`; đặt sai
> tên thì nó âm thầm nạp plugin mặc định (`@semantic-release/npm`) và cố phát
> hành lên npm thay vì GitHub. Kiểm tra cấu hình bằng `bun run release:dry`.

> ⚠️ Job Windows ARM64 và Linux ARM64 cần runner `windows-11-arm` /
> `ubuntu-24.04-arm` (GitHub-hosted ARM). Nếu tài khoản chưa bật loại runner này thì
> xoá 2 entry đó khỏi `matrix.include` trong `release.yml`.

**Secrets cần thêm trong repo** (Settings → Secrets and variables → Actions):

| Secret                                                                   | Bắt buộc             | Dùng để                                            |
| ------------------------------------------------------------------------ | -------------------- | -------------------------------------------------- |
| `TAURI_SIGNING_PRIVATE_KEY`                                              | ✅                   | ký gói cập nhật OTA (nội dung file `.tauri/*.key`) |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`                                     | nếu khoá có mật khẩu | mở khoá ký                                         |
| `APPLE_CERTIFICATE` + `APPLE_CERTIFICATE_PASSWORD` + `KEYCHAIN_PASSWORD` | không                | ký app macOS (thiếu thì build .dmg chưa ký)        |
| `APPLE_ID`, `APPLE_TEAM_ID`, `APPLE_PASSWORD`                            | không                | notarize để mở .dmg không cần "Bỏ qua"             |

Khoá ký OTA đã sinh sẵn ở `.tauri/hkd-tt152-app.key` (thư mục này **không** commit).
Mất khoá này thì các bản cập nhật về sau không kiểm chữ ký được — hãy sao lưu riêng.

### Cập nhật OTA trong app

Bản cài (.exe / .dmg / .AppImage) tự làm: màn **Cài đặt → Cập nhật ứng dụng** →
kiểm tra → tải (có thanh tiến trình) → tự khởi động lại. App lấy
`latest.json` từ Release mới nhất của chính repo này, nên chỉ cần publish
Release là người dùng nhận được bản mới.

- Bản chạy trong trình duyệt (web server nội bộ) không có updater — giao diện báo
  rõ điều đó thay vì báo lỗi.
- Máy Linux chỉ nhận OTA qua **AppImage**; bản `.deb` phải cài tay.

> ⚠️ **Repo đang để Private** nên endpoint `releases/latest/download/latest.json`
> trả 404 (không có token) → nút cập nhật sẽ báo lỗi và người dùng tải bản cài
> cũng phải đăng nhập GitHub. Muốn OTA + tải miễn phí thì chuyển repo sang
> Public, hoặc đổi `plugins.updater.endpoints` trong `tauri.conf.json` sang nơi
> lưu `latest.json` công khai ( ví dụ Cloudflare R2 / trang tĩnh).

Bản macOS chỉ build được `.dmg` sau khi có secret
`APPLE_CERTIFICATE` + `APPLE_CERTIFICATE_PASSWORD` + `KEYCHAIN_PASSWORD`; thiếu
chúng thì app ký ad-hoc (mở được nhưng phải bấm "Mở" ở Gatekeeper).

---

## 📚 Tài liệu

- 📐 [Thiết kế chi tiết TT152](docs/thiet-ke-chi-tiet-TT152.md) — kiến trúc, phạm vi nghiệp vụ,
  đối chiếu Thông tư 152/2021/TT-BTC, migration, roadmap.
- 🧪 [Kiểm thử tự động](e2e/README.md) — hai tầng test, danh sách bước E2E, cơ chế.
