/**
 * Mock cổng HĐĐT cho E2E — chạy offline, không cần credentials, không gọi mạng.
 *
 * App gọi vào mock qua `hddt_save_config` (base_url trỏ sang đây) nên không cần
 * sửa code app. Payload giữ đúng tên field của cổng thật (chỉ giữ field app
 * dùng) để UI render y hệt, và dữ liệu là số giả.
 *
 * Chạy kèm bởi playwright.config.ts (webServer thứ hai). Muốn test với cổng
 * thật thì đặt E2E_HDDT_LIVE=1 — app.spec.ts sẽ bỏ qua mock.
 */
import { createServer } from "node:http";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const PORT = Number(process.env.E2E_MOCK_PORTAL_PORT || 45733);
const TOKEN = "mock-token-e2e";

/**
 * Captcha thật của cổng (bắt 1 lần, lưu làm fixture) để solver glyph của app
 * giải được — đường đăng nhập offline đi qua đúng solver như ngoài đời.
 */
const CAPTCHA_SVG = readFileSync(
  fileURLToPath(new URL("../fixtures/captcha.svg", import.meta.url)),
  "utf8",
);

/** ZIP `invoice.xml` + `invoice.html` — đúng cấu trúc endpoint `export-xml`
 *  của cổng thật (đối chiếu 04/10/2026) để test lưu file XML offline. */
const XML_ZIP = readFileSync(
  fileURLToPath(new URL("../fixtures/invoice-xml.zip", import.meta.url)),
);

/** Dòng hóa đơn trong danh sách (khớp tên field của cổng). */
const LIST_ROW = {
  id: "99999999-8888-7777-6666-555555555555",
  khmshdon: 1,
  khhdon: "C26MOC",
  shdon: "0001",
  tdlap: "2026-09-01T10:00:00Z",
  tthai: 1,
  ttxly: 5,
  nbmst: "0100000000",
  nbten: "CÔNG TY CỔ PHẦN MOCK",
  nmmst: "0200000000",
  nmtnmua: "HỘ KINH DOANH MOCK",
  tgtcthue: 100000,
  tgtthue: 8000,
  ttcktmai: 0,
  tgtttbso: 108000,
};

/** Dòng hàng của hóa đơn (endpoint detail). */
const DETAIL_LINES = [
  {
    stt: 1,
    tchat: 1,
    ten: "Bình nước MOCK",
    dvtinh: "Bình",
    mhhdvu: "SP-MOCK",
    sluong: 2,
    dgia: 50000,
    thtien: 100000,
    stckhau: 0,
    ltsuat: "8%",
    tsuat: 0.08,
  },
];

/**
 * Chi tiết hóa đơn trả về `/invoices/detail` — đủ trường mà bố cục PDF
 * (`src/invoice-pdf`) in ra: tiêu đề, ngày, 2 bên, bảng tổng thuế và chữ ký số.
 * Giữ tối giản ở mức test được, không mô phỏng cả 122 trường của cổng thật.
 */
const DETAIL = {
  ...LIST_ROW,
  tlhdon: "HÓA ĐƠN GIÁ TRỊ GIA TĂNG",
  nky: "2026-09-01T10:00:00Z",
  mhdon: "MOCK0001",
  tchat: 1,
  chma: "CQT-HN",
  chten: "Cục Thuế Thành phố Hà Nội",
  nbdchi: "12 Đường Mock, Quận Ba Đình, Hà Nội",
  nbsdthoai: "0241234567",
  nbstkhoan: "000123456789",
  nbtnhang: "NGÂN HÀNG MOCK",
  nmtnmua: "HỘ KINH DOANH MOCK",
  nmten: "HỘ KINH DOANH MOCK",
  nmdchi: "Số 409 Đường Mock, Hà Nội",
  thtttoan: "Chuyển khoản",
  hdhhdvu: DETAIL_LINES,
  thttltsuat: [{ tsuat: "8%", thtien: 100000, tthue: 8000, gttsuat: null }],
  tgtcthue: 100000,
  tgtthue: 8000,
  tgtphi: 0,
  ttcktmai: 0,
  tgtttbso: 108000,
  tgtttbchu: "Một trăm lẻ tám nghìn đồng",
  nbcks: JSON.stringify({ Subject: "CÔNG TY CỔ PHẦN MOCK", SigningTime: "01/09/2026 10:00:00" }),
  qrcode: "https://hoadondientu.gdt.gov.vn/e2e/99999999",
};

const json = (res: import("node:http").ServerResponse, status: number, body: unknown) => {
  const text = JSON.stringify(body);
  res.writeHead(status, {
    "Content-Type": "application/json;charset=UTF-8",
    "Content-Length": Buffer.byteLength(text),
  });
  res.end(text);
};

const server = createServer((req, res) => {
  const url = new URL(req.url ?? "/", `http://127.0.0.1:${PORT}`);
  const path = url.pathname;
  console.log(`[mock-portal] ${req.method} ${path}${url.search}`);

  if (path === "/api/captcha") {
    return json(res, 200, { key: "mock-captcha-key", content: CAPTCHA_SVG });
  }
  if (path === "/api/security-taxpayer/authenticate") {
    return json(res, 200, { token: TOKEN });
  }
  if (path === "/api/security-taxpayer/profile") {
    return json(res, 200, {
      name: "HỘ KINH DOANH MOCK",
      username: "0100000000",
      password_expire: "2030-01-01T00:00:00Z",
      tinInfoTT86: { mst: "0100000000", mstUTien: "0100000000" },
    });
  }
  if (path.endsWith("/invoices/detail")) {
    // Cổng thật bắt buộc khmshdon dạng số — giữ luôn để test bắt lỗi gửi sai.
    const khmshdon = url.searchParams.get("khmshdon") ?? "";
    if (!/^\d+$/.test(khmshdon)) {
      return json(res, 500, {
        timestamp: "25/09/2026 01:18:41",
        message:
          "Failed to convert value of type 'java.lang.String' to required type 'byte'; " +
          `nested exception is java.lang.NumberFormatException: For input string: "${khmshdon}"`,
        details: "",
        path: "uri=/invoices/detail",
      });
    }
    return json(res, 200, DETAIL);
  }
  if (path.endsWith("/invoices/export-xml")) {
    // Trả ZIP binary (không phải JSON) — đúng hành vi cổng thật.
    res.writeHead(200, {
      "Content-Type": "application/zip",
      "Content-Length": XML_ZIP.length,
    });
    return res.end(XML_ZIP);
  }
  if (path.endsWith("/invoices/sold") || path.endsWith("/invoices/purchase")) {
    return json(res, 200, { datas: [LIST_ROW], state: "", total: 1, time: 12 });
  }
  return json(res, 404, { message: "not found" });
});

server.listen(PORT, "127.0.0.1", () => {
  console.log(`[mock-portal] listening on http://127.0.0.1:${PORT}`);
});
