/**
 * Bố cục hóa đơn giá trị gia tăng (A4) — port từ dự án `tra-cuu-hd`
 * (`server/logic/render-invoice-pdf/render-invoice-pdf.ts` → `renderInvoiceHtml`).
 *
 * Hàm này chỉ SINH HTML; phần còn lại của pipeline nằm ở `build.ts` (nhúng script
 * sinh QR + font + ảnh) và lệnh `render_invoice_pdf` của backend (in bằng Chrome
 * headless). Nguồn dữ liệu là `detail_json` đã lưu trong DB nên chạy offline.
 *
 * Khác bản gốc: giá trị chèn vào được escape HTML (xem `escapeValue`) — riêng
 * hai khối `.map().join()` dựng bảng hàng hóa/tổng thuế được bọc `raw()` vì đó
 * là HTML thật.
 */
import type { InvoiceData } from "./types";

/** Dấu hiệu "chuỗi này đã là HTML, đừng escape nữa". */
class RawHtml {
  constructor(readonly value: string) {}
}

const raw = (value: string) => new RawHtml(value);

const ESCAPES: Record<string, string> = {
  "&": "&amp;",
  "<": "&lt;",
  ">": "&gt;",
  '"': "&quot;",
  "'": "&#39;",
};

function escapeValue(value: unknown): string {
  if (value instanceof RawHtml) return value.value;
  // Bản gốc dùng `values[i] ?? ""` — null/undefined thành chuỗi rỗng.
  if (value === null || value === undefined) return "";
  return String(value).replace(/[&<>"']/g, (c) => ESCAPES[c]);
}

function html(strings: TemplateStringsArray, ...values: unknown[]): string {
  return strings.reduce((result, str, i) => result + str + escapeValue(values[i]), "");
}

export const allTChat = {
  1: "Hàng hóa, dịch vụ",
  2: "Khuyến mại",
  3: "Chiết khấu thương mại",
  4: "Ghi chú, diễn giải",
  5: "Hàng hóa đặc trưng",
} as const;

function formatVietnameseDate(dateStr: string): string {
  const date = new Date(dateStr);
  // Cổng có thể trả thiếu/ngày hỏng → tránh in ra "Ngày NaN tháng NaN năm NaN".
  if (Number.isNaN(date.getTime())) return "";
  const day = date.getUTCDate().toString().padStart(2, "0");
  const month = (date.getUTCMonth() + 1).toString().padStart(2, "0");
  const year = date.getUTCFullYear();
  return `Ngày ${day} tháng ${month} năm ${year}`;
}

function formatVND(num = 0): string {
  const value = Number(num);
  // `Intl.NumberFormat.format` in "NaN" khi gặp undefined/không phải số.
  if (!Number.isFinite(value)) return "0";
  return Intl.NumberFormat("vi-VN").format(value);
}

/**
 * Thông tin chữ ký số trong `nbcks` (chuỗi JSON). Cổng có thể không gửi trường
 * này (hóa đơn nháp / cổng mock) → trả object rỗng thay vì ném lỗi parse.
 */
function parseSignature(value: unknown): { Subject?: string; SigningTime?: string } {
  if (typeof value !== "string" || !value) return {};
  try {
    const parsed: unknown = JSON.parse(value);
    return parsed && typeof parsed === "object" ? (parsed as Record<string, string>) : {};
  } catch {
    return {};
  }
}

export function renderInvoiceHtml(data: InvoiceData): string {
  // Chữ ký số (`nbcks` là chuỗi JSON): không có thì bỏ hẳn khối "Signature
  // Valid" — hiển thị ô chữ ký trống sẽ như thể hóa đơn đã được ký.
  const signature = parseSignature(data.nbcks);
  const signed = Boolean(signature.Subject || signature.SigningTime);
  return html`
<html>

<head>
    <META http-equiv="Content-Type" content="text/html; charset=UTF-8">
    <title>${data.shdon} - ${data.nbten}</title>
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <meta http-equiv="X-UA-Compatible" content="IE=Edge">
    <style>
        * {
            box-sizing: border-box;
            -moz-box-sizing: border-box;
        }

        body {
            width: 100%;
            height: 100%;
            margin: 0 auto;
            padding: 0;
            font-size: 13pt;
        }

        .print-page {
            width: 210mm;
            min-height: 297mm;
            margin: 0mm auto;
        }

        .main-page {
            max-width: 210mm;
            padding: 20px 20px 10px;
            margin: auto;
            border: 3px double rgba(145, 87, 21, 0.69);
            line-height: 1.5;
            box-shadow: rgb(222 226 230 / 70%) 0px 0px 9px 2px;
        }
        .main-page > div {
            background-image: url(viewinvoice-bg.jpg);
            background-repeat: no-repeat;
            background-position: center center;
            background-size: 180%;
        }

        canvas {
            display: inline-block
        }

        html {
            font-size: 100%
        }

        .heading-content .main-title {
            font-size: 20pt;
            text-align: center;
            display: block;
            font-weight: bold;
            text-transform: uppercase;
        }

        .heading-content p {
            font-size: 13pt;
            text-align: right;
        }

        .heading-content p.day {
            text-align: center;
            display: block;
        }

        .day .mg-bottom {
            margin-bottom: 8px !important;
            text-align: center;
        }

        .heading-content .top-content {
            display: flex;
            justify-content: space-between;
        }

        .heading-content .code-content {
            display: inline-block;
        }

        .heading-content .code-ms {
            display: flex;
            font-size: 12pt;
        }

        .vip-divide {
            width: 100%;
            height: 0;
            border-bottom: 1px solid rgba(145, 87, 21, 0.69);
        }

        .flex-li {
            display: flex;
        }

        .content-info {
            padding-top: 5px;
        }

        .content-info .list-fill-out {
            list-style: none;
            padding-inline-start: 0;
            margin-top: 5px;
            margin-bottom: 5px;
        }

        .content-info .list-fill-out li {
            font-size: 13pt;
        }

        .content-info .tx-money {
            text-align: right;
        }

        .content-info .square-list {
            display: flex;
            flex-wrap: wrap;
            align-items: center;
            padding: 10px 0px;
        }

        .content-info .square-list ul {
            display: flex;
            flex-wrap: wrap;
            padding-left: 25px;
        }

        .content-info .square-list ul li {
            width: 35px;
            height: 35px;
            display: block;
            border-left: 1px solid #000;
            border-top: 1px solid #000;
            border-bottom: 1px solid #000;
        }

        .content-info .square-list ul li:last-child {
            border-right: 1px solid #000;
        }

        .table-horizontal-wrapper {
            display: flex;
            justify-content: space-between;
        }

        .res-tb {
            border-collapse: collapse;
            border-spacing: 0px;
            width: 100%;
            overflow-x: auto;
            margin: 10px 0px;
            min-width: 250px;
        }

        .res-tb tr td {
            border: 1px solid black;
            padding: 6px 4px 6px 4px;
            vertical-align: baseline;
        }

        .res-tb tr td.tx-center {
            text-align: center;
        }

        .res-tb tr td.tx-left {
            text-align: left;
        }

        .res-tb tr td.tx-right {
            text-align: right;
        }

        .res-tb thead tr th {
            border: 1px solid black;
            vertical-align: middle;
            padding: 6px 4px 6px 4px;
        }

        .res-tb thead tr th.tb-stt {
            width: 65px;
            text-align: center;
        }

        .res-tb thead tr th.tb-thh {
            width: 200px;
            text-align: center;
        }

        .res-tb thead tr th.tb-dvt {
            width: 100px;
            text-align: center;
        }

        .res-tb thead tr th.tb-sl {
            width: 80px;
            text-align: center;
        }

        .res-tb thead tr th.tb-dg {
            width: 80px;
            text-align: center;
        }

        .res-tb thead tr th.tb-ts {
            width: 77px;
            text-align: center;
        }

        .res-tb thead tr th.tb-ttct {
            width: 250px;
            text-align: center;
        }

        .res-tb thead tr th.tb-stt2 {
            width: 70px;
            text-align: center;
        }

        .res-tb thead tr th.tb-thh2 {
            width: 160px;
            text-align: center;
        }

        .res-tb thead tr th.tb-dvt2 {
            width: 90px;
            text-align: center;
        }

        .res-tb thead tr th.tb-sl2 {
            width: 60px;
            text-align: center;
        }

        .res-tb thead tr th.tb-dg2 {
            width: 60px;
            text-align: center;
        }

        .res-tb thead tr th.tb-ts2 {
            width: 70px;
            text-align: center;
        }

        .res-tb thead tr th.tb-ttct2 {
            width: 190px;
            text-align: center;
        }

        .res-tb thead tr th.tb-dvtt {
            width: 70px;
            text-align: center;
        }

        .res-tb thead tr th.tb-tg {
            width: 70px;
            text-align: center;
        }

        .res-tb thead tr th.tb-ttgd {
            width: 170px;
            text-align: center;
        }

        .res-tb thead tr th.tb-ctgd {
            width: 170px;
            text-align: center;
        }

        .ft-sign {
            padding-top: 20px;
        }

        .ft-sign .sign-dx {
            display: flex;
            flex-wrap: wrap;
            justify-content: space-around;
            align-items: flex-start;
        }

        .ft-sign .sign-dx h3 p {
            text-align: center;
            font-size: 13pt;
            font-weight: 100;
        }

        .ft-sign .sign-dx h3 p:nth-child(2) {
            font-size: 14px;
            font-weight: normal;
        }

        .ft-sign .fd-end {
            padding-top: 120px;
            text-align: center;
        }

        .ft-sign .appendix {
            padding-top: 120px;
        }

        .ft-sign .appendix .apen-ul {
            display: flex;
            flex-wrap: wrap;
            list-style: none;
        }

        .ft-sign .appendix .apen-ul li {
            margin-right: 25px;
        }

        .ft-sign .appendix .apen-ul li:last-child {
            margin-right: unset;
        }

        .sign-box {
            width: 260px !important;
            padding: 5px !important;
            border: 2px solid #23b709 !important;
            background-image: url("sign-check.jpg") !important;
            background-repeat: no-repeat !important;
            background-position: right 45px bottom 10px !important;
            background-size: 70px 60px !important;
            margin-top: 10px !important;
            font-weight: 500;
        }

        .sign-box span {
            color: #23b709 !important;
            font-size: 13pt !important;
            text-align: left !important;
            display: block;
        }

        .data-item-auto-w {
            display: flex;
            justify-content: left;
            align-items: flex-start;
            font-size: 13pt;
            color: rgba(0, 0, 0, 0.85);
        }

        .data-item-auto-w .di-label {
            min-height: 25px;
            height: auto;
            border-bottom: 1px dashed transparent;
            display: flex;
            align-items: flex-start;
        }

        .data-item-auto-w .di-value {
            box-sizing: border-box;
            flex: 1;
            min-height: 25px;
            display: flex;
            align-items: flex-start;
            padding-left: 5px;
            height: auto;
            justify-content: center;
        }

        .data-item {
            width: 100%;
            display: flex;
            justify-content: left;
            align-items: flex-start;
            font-size: 13pt;
            color: rgba(0, 0, 0, 0.85);
        }

        .data-item .di-label {
            min-height: 25px;
            height: auto;
            border-bottom: 1px dashed transparent;
            display: flex;
            align-items: flex-start;
        }

        .data-item .di-value {
            box-sizing: border-box;
            flex: 1;
            min-height: 25px;
            /* // border-bottom: 1px dashed #e8e8e8; */
            display: flex;
            align-items: flex-start;
            padding-left: 5px;
            height: auto;
            justify-content: unset;
        }

        .space {
            display: inline-block;
            width: 20px;
            height: 1px;
        }

        .data-text {
            width: 100%;
            font-size: 13pt;
            color: rgba(0, 0, 0, 0.85);
        }

        @page {
            size: A4;
            margin: 0 !important;
        }

        @media print {
            * {
                -webkit-print-color-adjust: exact;
                print-color-adjust: exact;
            }

            body {
                width: auto;
                height: auto;
                margin: 0 auto;
            }

            table tr,
            td {
                page-break-inside: avoid;
            }

            table thead {
                /* display: table-row-group !important; */
            }

            .table-horizontal-wrapper {
                page-break-inside: avoid;
                /* padding-top: 5px; */
            }

            .main-page {
                margin: 0;
                width: initial;
                min-height: 296mm;
                background: none;
                border: none;
            }

            .ft-sign {
                page-break-inside: avoid !important;
                page-break-after: auto;
            }

            .fd-end {
                padding-top: 0 !important;
            }
        }

        body {
            width: 100%;
            height: 100%;
            margin: 0 auto;
            padding: 0;
            font-size: 13pt;
        }

        .bg-container {
            position: fixed;
            width: 100%;
            height: 100%;
            z-index: 0;
            background-image: url("viewinvoice-bg.jpg");
            background-repeat: no-repeat;
            background-position: center;
            background-size: 250%;
            border: 3px double rgba(145, 87, 21, 0.69);
            top: 0px;
            left: 0px;
        }

        .print-page {
            width: 210mm;
            min-height: 297mm;
            margin: 0mm auto;
        }

        .main-page {
            font-family: "Times New Roman";
            padding: 20px;
            line-height: 1.5;
            position: relative;
            z-index: 1;
            border: 3px double rgba(145, 87, 21, 0.69);
        }

        .heading-ct {
            width: 100%;
        }

        .heading-ct .code {
            display: block;
            text-align: right;
            font-size: 13pt;
            padding-bottom: 5px;
        }

        .heading-ct .lg-plan {
            display: flex;
            justify-content: space-between;
        }

        .heading-ct .lg-plan div {
            text-align: center;
        }

        .heading-ct h5 {
            margin: 0;
            padding: 0;
            margin-bottom: 0.5rem;
        }

        .pop-content {
            padding-top: 0px;
        }

        .pop-content .content-head {
            text-align: center;
        }

        .pop-content .content-head h5 {
            display: flex;
            justify-content: center;
            font-size: 16px;
        }

        .pop-content .content-head h5 i {
            font-weight: normal;
        }

        .pop-content .content-info {
            padding-top: 0px;
        }

        .pop-content .content-info ul {
            list-style: none;
            padding: 0;
        }

        .pop-content .content-info .sign-end {
            display: block;
            text-align: right;
        }

        .pop-content .content-info .sign-flex {
            display: flex;
            flex-direction: column;
            justify-content: flex-end;
            align-items: flex-end;
            text-align: center;
            padding-top: 14px;
        }

        .res-tb {
            border-collapse: collapse;
            border-spacing: 0;
            width: 100%;
            margin-top: 10px;
            margin-bottom: 10px;
        }

        .res-tb tr td {
            border: 1px solid black;
            padding: 6px 4px 6px 4px;
            vertical-align: baseline;
        }

        .res-tb tr td.tx-center {
            text-align: center;
        }

        .res-tb thead tr th {
            border: 1px solid black;
            vertical-align: middle;
            /* white-space: nowrap; */
            padding: 6px 4px 6px 4px;
        }

        .res-tb thead tr th.tb-rs-nb {
            width: 150px;
        }

        .res-tb thead tr th.tb-rs-kh {
            width: 300px;
        }

        .res-tb thead tr th.tb-rs-shd {
            width: 250px;
        }

        .res-tb thead tr th.tb-rs-nhd {
            width: 200px;
        }

        .res-tb thead tr th.tb-rs-lap {
            width: 250px;
        }

        .res-tb thead tr th.tb-rs-ldr {
            width: 250px;
        }

        .get-plus::before {
            content: "+"
        }

        .sign-box {
            width: 260px !important;
            padding: 5px !important;
            border: 2px solid #23b709 !important;
            background-image: url("sign-check.jpg") !important;
            background-repeat: no-repeat !important;
            background-position: right 45px bottom 10px !important;
            background-size: 70px 60px !important;
            margin-top: 10px !important;
            font-weight: 500;
        }

        .span-sign-box {
            display: inline !important;
        }

        .sign-box span {
            color: #23b709 !important;
            font-size: 13pt !important;
            text-align: left !important;
            display: block;
        }

        .data-item {
            width: 100%;
            display: flex;
            justify-content: left;
            align-items: flex-start;
            font-size: 13pt;
            color: #000;
            margin-bottom: 10px;
        }

        .data-item .di-label {
            min-height: 25px;
            border-bottom: 1px dashed transparent;
            display: flex;
            align-items: flex-start;
        }

        .data-item .di-value {
            min-height: 25px;
            box-sizing: border-box;
            flex: 1;
            border-bottom: 1px dashed #e8e8e8;
            display: flex;
            align-items: flex-start;
            padding-left: 10px;
            justify-content: flex-start;
        }

        .sign-content b {
            display: block;
        }

        .sign-content b {
            display: block;
        }

        .sign-row {
            padding-top: 20px;
            display: flex;
            justify-content: space-between;
        }

        .text-bold {
            font-weight: bold;
        }

        .text-indent {
            text-indent: 40px
        }

        .p-space {
            text-indent: 30pt;
            font-size: 13pt;
            margin-bottom: 10px;
        }

        .p-no-space {
            text-indent: 30pt;
            font-size: 13pt;
            margin-bottom: 0;
        }

        .justify {
            text-align: justify;
        }

        .font-size-custome {
            font-size: 12.5px !important;
        }

        @page {
            size: A4;
            margin: 0 !important;
        }

        @media print {
            * {
                -webkit-print-color-adjust: exact;
                print-color-adjust: exact;
            }

            body {
                width: auto;
                height: auto;
                margin: 0 auto;
            }

            table,
            tr,
            td {
                /* page-break-inside: avoid; */
            }

            .main-page {
                margin: 0;
                width: initial;
                min-height: 296mm;
                background: none;
                border: none;
            }

            div>.page-break {
                /* page-break-inside: avoid; */
                /* padding-top: 5px; */
            }

            .sign-flex {
                page-break-inside: avoid !important;
                page-break-after: auto;
                padding-top: 0 !important;
            }

            .sign-box {
                line-height: 1.2 !important;
            }
        }
    </style>
</head>

<body>
    <div class="main-page">
        <div class="heading-content">
            <div class="top-content">
                <div style="width: 80px; min-height: 20px">
                    <div id="qrcodeTable"></div>
                </div>
                <div class="code-content">
                    <b>
                        Mẫu số:
                        ${data.khmshdon}</b>
                    <br>
                    <b>
                        Ký hiệu:
                        ${data.khhdon}</b>
                    <br>
                    <b>
                        Số:
                        ${data.shdon}</b>
                </div>
            </div>
            <div class="title-heading">
                <h2 class="main-title">
                    ${String(data.tlhdon ?? "HÓA ĐƠN").toUpperCase()}
                </h2>
                <p class="day">
                <div class="day">
                    <p class="day">${formatVietnameseDate(data.nky)}</p>
                    <p class="day">
                        MCCQT:
                        ${data.mhdon}</p>
                </div>
                </p>
            </div>
        </div>
        <div class="vip-divide"></div>
        <div class="content-info">
            <ul class="list-fill-out">
                <li>
                    <div class="data-item">
                        <div class="di-label">
                            <span>Tên người bán:</span>
                        </div>
                        <div class="di-value">
                            <div>${data.nbten}</div>
                        </div>
                    </div>
                </li>
                <li>
                    <div class="data-item">
                        <div class="di-label">
                            <span>Mã số thuế:</span>
                        </div>
                        <div class="di-value">
                            <div>${data.nbmst}</div>
                        </div>
                    </div>
                </li>
                <li>
                    <div class="data-item">
                        <div class="di-label">
                            <span>Mã cửa hàng:</span>
                        </div>
                        <div class="di-value">
                            <div>${data.chma}</div>
                        </div>
                    </div>
                </li>
                <li>
                    <div class="data-item">
                        <div class="di-label">
                            <span>Tên cửa hàng:</span>
                        </div>
                        <div class="di-value">
                            <div>${data.chten}</div>
                        </div>
                    </div>
                </li>
                <li>
                    <div class="data-item">
                        <div class="di-label">
                            <span>Địa chỉ:</span>
                        </div>
                        <div class="di-value">
                            <div>${data.nbdchi}</div>
                        </div>
                    </div>
                </li>
                <li>
                    <div class="data-item">
                        <div class="di-label">
                            <span>Điện thoại:</span>
                        </div>
                        <div class="di-value">
                            <div>${data.nbsdthoai}</div>
                        </div>
                    </div>
                </li>
                <li>
                    <div class="data-item">
                        <div class="di-label">
                            <span>Số tài khoản:</span>
                        </div>
                        <div class="di-value">
                            <div>
                                ${data.nbstkhoan}    ${data.nbtnhang}
                            </div>
                        </div>
                    </div>
                </li>
                <li>
                    <div class="vip-divide" style="margin: 5px 0;"></div>
                </li>
                <li>
                    <div class="data-item">
                        <div class="di-label">
                            <span>Tên người mua:</span>
                        </div>
                        <div class="di-value">
                            <div>${data.nmten}</div>
                        </div>
                    </div>
                </li>
                <li>
                    <div class="data-item">
                        <div class="di-label">
                            <span>Họ tên người mua:</span>
                        </div>
                        <div class="di-value">
                            <div>${data.nmtnmua}</div>
                        </div>
                    </div>
                </li>
                <li>
                    <div class="data-item">
                        <div class="di-label">
                            <span>Mã số thuế:</span>
                        </div>
                        <div class="di-value">
                            <div>${data.nmmst}</div>
                        </div>
                    </div>
                </li>
                <li>
                    <div class="data-item">
                        <div class="di-label">
                            <span>Mã ĐVCQHVNSNN:</span>
                        </div>
                        <div class="di-value">
                            <div>${data.nbmdvqhnsach}</div>
                        </div>
                    </div>
                </li>
                <li>
                    <div class="data-item">
                        <div class="di-label">
                            <span>CCCD người mua:</span>
                        </div>
                        <div class="di-value">
                            <div>${data.nmcmnd}</div>
                        </div>
                    </div>
                </li>
                <li>
                    <div class="data-item">
                        <div class="di-label">
                            <span>Số hộ chiếu:</span>
                        </div>
                        <div class="di-value">
                            <div>${data.nmshchieu}</div>
                        </div>
                    </div>
                </li>
                <li>
                    <div class="data-item">
                        <div class="di-label">
                            <span>Địa chỉ:</span>
                        </div>
                        <div class="di-value">
                            <div>${data.nmdchi}</div>
                        </div>
                    </div>
                </li>
                <li>
                    <div class="data-item">
                        <div class="di-label">
                            <span>Số tài khoản:</span>
                        </div>
                        <div class="di-value">
                            <div>
                                <div>
                                    ${data.nmstkhoan}    ${data.nmtnhang}
                                </div>
                            </div>
                        </div>
                    </div>
                </li>
                <li>
                    <div class="data-item">
                        <div class="di-label">
                            <span>Hình thức thanh toán:</span>
                        </div>
                        <div class="di-value">
                            <div>${data.thtttoan}</div>
                        </div>
                    </div>
                </li>
                <li class="flex-li">
                    <div class="data-item" style="width: 50%">
                        <div class="di-label">
                            <span>Số bảng kê:</span>
                        </div>
                        <div class="di-value">
                            <div>${data.dksbke}</div>
                        </div>
                    </div>
                    <div class="data-item" style="width: 50%">
                        <div class="di-label">
                            <span>Ngày bảng kê:</span>
                        </div>
                        <div class="di-value">
                            <div>${data.dknlbke}</div>
                        </div>
                    </div>
                </li>
            </ul>
            <table class="res-tb">
                <thead style="text-align: center;">
                    <tr>
                        <th class="tb-stt">STT</th>
                        <th class="tb-stt">Tính chất</th>
                        <th class="tb-stt">Loại hàng hóa đặc trưng</th>
                        <th class="tb-thh">Tên hàng hóa, dịch vụ</th>
                        <th class="tb-dvt">Đơn vị tính</th>
                        <th class="tb-sl">Số lượng</th>
                        <th class="tb-dg">Đơn giá</th>
                        <th class="tb-dg">Chiết khấu</th>
                        <th class="tb-ts">Thuế suất</th>
                        <th class="tb-ttct">Thành tiền chưa có thuế GTGT</th>
                    </tr>
                </thead>
                <tbody>

                    ${raw(
                      (data.hdhhdvu ?? [])
                        .map(
                          (data) => html`
                            <tr t-chat="${data.tchat}">
                              <td class="tx-center">${data.stt}</td>
                              <td class="tx-left">
                                <span>${allTChat[data.tchat as keyof typeof allTChat]}</span>
                              </td>
                              <td
                                class="tx-left"
                                style="max-width: 200px;word-wrap: break-word;"
                              ></td>
                              <td class="tx-left">${data.ten}</td>
                              <td class="tx-left">${data.dvtinh}</td>
                              <td class="tx-center">${data.sluong}</td>
                              <td class="tx-center">${formatVND(data.dgia)}</td>
                              <td class="tx-center">${data.stckhau}</td>
                              <td class="tx-center">
                                <HHDVu>${data.ltsuat}</HHDVu>
                              </td>
                              <td class="tx-center">${formatVND(data.thtien)}</td>
                            </tr>
                          `,
                        )
                        .join("\n"),
                    )}
                </tbody>
            </table>
            <div class="table-horizontal-wrapper">
                <div style="margin-right: 10;">
                    <table class="res-tb">
                        <thead style="text-align: center">
                            <tr>
                                <th>Thuế suất</th>
                                <th>Tổng tiền chưa thuế</th>
                                <th>Tổng tiền thuế</th>
                            </tr>
                        </thead>
                        <tbody>
                            ${raw(
                              (data.thttltsuat ?? [])
                                .map(
                                  (vat) => html`
                                    <tr>
                                      <td class="tx-center">
                                        <LTSuat>${vat.tsuat}</LTSuat>
                                      </td>
                                      <td class="tx-center">${formatVND(vat.thtien)}</td>
                                      <td class="tx-center">${formatVND(vat.tthue)}</td>
                                    </tr>
                                  `,
                                )
                                .join("\n"),
                            )}
                        </tbody>
                    </table>
                </div>
                <div style="flex: 1">
                    <table class="res-tb">
                        <tbody>
                            <tr>
                                <td class="tx-center">
                                    Tổng tiền chưa thuế
                                    <br>
                                    (Tổng cộng thành tiền chưa có thuế)
                                </td>
                                <td class="tx-center" style="min-width: 200px; max-width: 300px;">${formatVND(
                                  data.tgtcthue,
                                )}</td>
                            </tr>
                            <tr>
                                <td class="tx-center">Tổng tiền thuế (Tổng cộng tiền thuế)</td>
                                <td class="tx-center" style="min-width: 200px; max-width: 300px">${formatVND(
                                  data.tgtthue ?? 0,
                                )}</td>
                            </tr>
                            <tr>
                                <td class="tx-center">Tổng tiền phí</td>
                                <td class="tx-center" style="min-width: 200px; max-width: 300px">${formatVND(
                                  data.tgtphi ?? 0,
                                )}</td>
                            </tr>
                            <tr>
                                <td class="tx-center">
                                    Tổng tiền chiết khấu thương mại
                                </td>
                                <td class="tx-center" style="min-width: 200px; max-width: 300px">${formatVND(
                                  data.ttcktmai,
                                )}</td>
                            </tr>
                            <tr>
                                <td class="tx-center">
                                    Tổng tiền thanh toán bằng số
                                </td>
                                <td class="tx-center" style="min-width: 200px; max-width: 300px">${formatVND(
                                  data.tgtttbso,
                                )}</td>
                            </tr>
                            <tr>
                                <td class="tx-center">
                                    Tổng tiền thanh toán bằng chữ
                                </td>
                                <td class="tx-center" style="min-width: 200px; max-width: 300px">${
                                  data.tgtttbchu
                                }</td>
                            </tr>
                        </tbody>
                    </table>
                </div>
            </div>
        </div>
        <div class="vip-divide"></div>
        <div class="ft-sign">
            <div class="sign-dx">
                <h3>
                    <p>NGƯỜI MUA HÀNG</p>
                    <p>
                        <i>(Chữ ký số (nếu có))</i>
                    </p>
                    <td></td>
                </h3>
                <h3>
                    <p>NGƯỜI BÁN HÀNG</p>
                    <p>
                        <i>(Chữ ký điện tử, chữ ký số)</i>
                    </p>
                    ${raw(
                      signed
                        ? html`
                            <div class="sign-box">
                              <span>Signature Valid</span><span class="span-sign-box">Ký bởi</span
                              ><span id="cks" class="span-sign-box">${signature.Subject ?? ""}</span
                              ><span></span><span class="span-sign-box">Ký ngày</span
                              ><span class="span-sign-box">${signature.SigningTime ?? ""}</span>
                            </div>
                          `
                        : "",
                    )}
                </h3>
            </div>
            <div class="fd-end">
                <p>
                    <i>(Cần kiểm tra, đối chiếu khi lập, nhận hóa đơn)</i>
                </p>
            </div>
        </div>
    </div>
    <input type="hidden" id="qrcodeContent"
        value="${data.qrcode}">
    <!-- <script type="text/javascript" src="details.js"></script> -->

    <script type="module" src="/capture.ts">
    </script>
</body>

</html>
`;
}
