// Builds the Excel test file for the "Nhập liệu" screen (the `import` E2E step).
// The original "NHAP LIEU" sheet is an empty template, so we generate a small
// file that matches the column map ImportView.vue reads (see e2e/README.md).
//
// Usage: bun e2e/make_fixtures.mjs [output-path]
import * as XLSX from "xlsx";
import { mkdirSync } from "node:fs";
import { dirname } from "node:path";

const out = process.argv[2] || "e2e/fixtures/nhap-lieu-e2e.xlsx";

// idx: 0 LoaiPhieu | 1 SoPhieu | 2 NgayGhiSo | 3 SoHieu | 4 NgayChungTu | 5 KyKhaiThue
//      6 Dien giai | 7 SL | 8 So tien | 9 MaKH | 10 MaNCC | 11 MaVatTu
//      18 NhomNganhNghe | 19 TyLeGTGT | 20 TyLeTNCN
const header = [
  "LoaiPhieu", "SoPhieu", "NgayGhiSo", "SoHieu", "NgayChungTu", "KyKhaiThue",
  "Dien giai", "SoLuong", "So tien", "MaKH", "MaNCC", "MaVatTu",
  "", "", "", "", "", "", "NhomNganhNghe", "TyLeGTGT", "TyLeTNCN",
];
const row = (v) => {
  const r = new Array(21).fill("");
  return Object.assign(r, v);
};

const aoa = [
  ["Mẫu nhập liệu E2E — fixture tự sinh (e2e/make_fixtures.mjs)"],
  ["Dòng tiêu đề phụ 2 (bắt buộc có nội dung để SheetJS giữ đúng số dòng)"],
  ["Dòng 3"],
  ["Dòng 4"],
  ["Dòng 5"],
  header,
  row({ 1: "PN101", 2: "2026-01-05", 3: "HD-IMP1", 4: "2026-01-05", 5: 202601,
        6: "Nhập từ Excel E2E", 7: 3, 8: 300000, 10: "NCC001", 11: "SP001",
        18: "PPHH", 19: 1, 20: 0.5 }),
  row({ 1: "PX101", 2: "2026-01-06", 3: "HD-IMP2", 4: "2026-01-06", 5: 202601,
        6: "Xuất từ Excel E2E", 7: 2, 8: 200000, 9: "KH001", 11: "SP001",
        18: "PPHH", 19: 1, 20: 0.5 }),
];

const ws = XLSX.utils.aoa_to_sheet(aoa);
const wb = XLSX.utils.book_new();
XLSX.utils.book_append_sheet(wb, ws, "NHAP LIEU");
mkdirSync(dirname(out), { recursive: true });
XLSX.writeFile(wb, out);
console.log("wrote", out);
