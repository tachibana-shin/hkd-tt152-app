# 🧾 Phần mềm Kế toán Hộ kinh doanh — TT 152/2021/TT-BTC

> Ứng dụng desktop kế toán cho **Hộ kinh doanh / Cá nhân kinh doanh**, bám theo Thông tư
> 152/2021/TT-BTC và bộ mẫu Excel HKD (nhật ký chung, sổ quỹ, NXT, bảng lương, chấm công,
> tờ khai thuế GTGT/TNCN, bảng kê giảm thuế GTGT, phiếu nhập/xuất kho, hóa đơn điện tử).
>
> **Phiên bản:** `0.3.0` · **Nền tảng:** Tauri 2 (Rust + sqlx + SQLite) · **Giao diện:** Vue 3 + TypeScript (PrimeVue + Tailwind CSS)

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

## 📚 Tài liệu

- 📐 [Thiết kế chi tiết TT152](docs/thiet-ke-chi-tiet-TT152.md) — kiến trúc, phạm vi nghiệp vụ,
  đối chiếu Thông tư 152/2021/TT-BTC, migration, roadmap.
- 🧪 [Kiểm thử tự động](e2e/README.md) — hai tầng test, danh sách bước E2E, cơ chế.
