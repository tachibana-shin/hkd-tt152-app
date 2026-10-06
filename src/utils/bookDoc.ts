/**
 * Xuất sổ kế toán TT 152/2025/TT-BTC ra file Word `.docx` — khổ A4 dọc.
 *
 * Vì sao là `.docx` thật chứ không phải HTML bọc đuôi `.doc`: Word bản mới cứ
 * hiện cảnh báo "đuôi file không khớp định dạng" mỗi lần mở, còn `.docx` (OOXML)
 * thì Word lẫn LibreOffice mở thẳng. Dùng thư viện `docx` thay vì tự nối chuỗi
 * XML vì bảng sổ có viền, tô nền theo hàng và khai độ rộng cột — tự làm rất
 * dễ sai một mắt xích là Word mở lỗi.
 *
 * Khổ giấy bám theo màn in của app (`@page { size: A4 portrait; margin: 12mm }`
 * ở PrintVoucher) để file Word in ra cùng một khổ với nút In. Vùng in còn lại
 * là 186mm; tỉ lệ `rem` từng mẫu được dàn đều lên đúng 186mm nên cột vẫn đúng
 * tỉ lệ mẫu in (S1a 7-7-24-10) và bảng phủ hết trang, không hở lề bên phải.
 *
 * Chỉ đưa nội dung chính của sổ vào: đầu sổ, bảng, chữ ký. `book.notes` là lời
 * khuyên của app khi thao tác (cảnh báo chọn nhầm mẫu, tổng hợp nhanh) — không
 * phải nội dung nộp cơ quan thuế nên không in vào file.
 */
import {
  AlignmentType,
  BorderStyle,
  convertMillimetersToTwip,
  Document,
  PageOrientation,
  Packer,
  Paragraph,
  ShadingType,
  Table,
  TableCell,
  TableLayoutType,
  TableRow,
  TextRun,
  VerticalAlign,
  WidthType,
} from "docx";
import type { BookColumn, BookHeader, TaxBook } from "@/types";
import { bookLabelColumn, bookCellValue, renderBookCell } from "@/utils/bookCell";
import { downloadBinary } from "@/utils/download";

/** Lề 12mm — cùng khổ màn in của app, đổi sang twip (1/20 điểm) cho Word. */
const MARGIN = convertMillimetersToTwip(12);
/** Vùng in của A4 dọc: 210 − 2×12 = 186mm. */
const CONTENT = convertMillimetersToTwip(210 - 12 * 2);

/**
 * Phông chữ: Times New Roman là phông chuẩn của biểu mẫu thuế Việt Nam, có đủ
 * dấu tiếng Việt ở mọi máy có Office/LibreOffice.
 */
const FONT = "Times New Roman";

/** Kẻ ô mảnh như 1px trên màn — Word đo độ dày theo 1/8 điểm (6 ≈ 0,75pt). */
const GRID = { style: BorderStyle.SINGLE, size: 6, color: "000000" };
/** Không kẻ — dùng cho khung đầu sổ (không có đáy, không có vách giữa). */
const NO_LINE = { style: BorderStyle.NONE, size: 0, color: "auto" };

/**
 * Cỡ chữ theo nửa điểm của Word (28 = 14pt).
 *
 * Cỡ chữ gốc của bộ xuất file (Word lẫn Excel) là 14pt theo yêu cầu — in A4
 * đọc rõ; các dòng phụ scale theo: tiêu đề cột/khối 13pt, chú thích 12pt, tên
 * sổ 16pt. Bảng vẫn `TableLayoutType.FIXED` nên cột hẹp tự xuống dòng thay vì
 * tràn ô.
 */
const SIZE_BODY = 28; // 14pt — chữ trong bảng
const SIZE_BAND = 26; // 13pt — tiêu đề cột, hàng khối ngành
const SIZE_NOTE = 24; // 12pt — dòng "(Ký, ghi rõ họ tên…)"
const SIZE_TITLE = 32; // 16pt — tên sổ

/** Nền hàng cột và hàng tổng hợp — theo `@media print` của bảng sổ. */
const FILL_HEAD = "EEEEEE";
const FILL_BAND = "F6F6F6";
/** Số âm tô đỏ — cùng `.text-rose-600` khi in từ giao diện. */
const COLOR_NEG = "C00000";

type RunOpts = {
  bold?: boolean;
  italics?: boolean;
  size?: number;
  caps?: boolean;
  color?: string;
};

type Align = (typeof AlignmentType)[keyof typeof AlignmentType];

/** Một đoạn chữ, mặc định cỡ chữ trong bảng. */
function run(text: string, o: RunOpts = {}): TextRun {
  return new TextRun({
    text,
    size: o.size ?? SIZE_BODY,
    bold: o.bold,
    italics: o.italics,
    allCaps: o.caps,
    color: o.color,
  });
}

/** Một dòng không giãn thừa — các dòng trong sổ kẻ sát nhau như mẫu in. */
function para(
  children: readonly TextRun[],
  o: { align?: Align; before?: number; after?: number } = {},
): Paragraph {
  return new Paragraph({
    children,
    alignment: o.align,
    spacing: { before: o.before ?? 0, after: o.after ?? 0 },
  });
}

/**
 * Độ rộng cột (twip) theo tỉ lệ `rem` backend khai — cùng tỉ lệ với bảng trên
 * màn hình, chỉ đổi sang khổ trang. Làm tròn xong cộng lại lệch vài twip so
 * 186mm nên bù vào cột cuối để bảng luôn đúng khổ trang.
 */
function columnTwips(cols: BookColumn[]): number[] {
  if (cols.length === 0) return [];
  const rem = cols.map((c) => parseFloat(c.width) || 0);
  const total = rem.reduce((a, b) => a + b, 0);
  if (total <= 0) {
    // Không khai độ rộng (hoặc khai lỗi) thì chia đều, vẫn phủ đúng khổ.
    const each = Math.floor(CONTENT / cols.length);
    return cols.map(() => each);
  }
  const w = rem.map((r) => Math.round((CONTENT * r) / total));
  w[w.length - 1] += CONTENT - w.reduce((a, b) => a + b, 0);
  return w;
}

type CellOpts = {
  col: BookColumn;
  width: number;
  text: string;
  fill?: string;
  bold?: boolean;
  size?: number;
  caps?: boolean;
  negative?: boolean;
};

/** Một ô: số căn phải, chữ căn trái, luôn bám đỉnh ô như trên giao diện. */
function dataCell(o: CellOpts): TableCell {
  return new TableCell({
    width: { size: o.width, type: WidthType.DXA },
    verticalAlign: VerticalAlign.TOP,
    // Lề ô 0,2rem như `.book-table td` — cột ngày hẹp vẫn còn chỗ đọc được.
    margins: { top: 30, left: 50, bottom: 30, right: 50 },
    shading: o.fill ? { type: ShadingType.CLEAR, color: "auto", fill: o.fill } : undefined,
    children: [
      para(
        [
          run(o.text, {
            bold: o.bold,
            size: o.size,
            caps: o.caps,
            color: o.negative ? COLOR_NEG : undefined,
          }),
        ],
        { align: o.col.kind === "text" ? AlignmentType.LEFT : AlignmentType.RIGHT },
      ),
    ],
  });
}

/** Khối đầu sổ: họ tên hộ bên trái, khối "Mẫu số …" in nghiêng bên phải. */
function headBox(header: BookHeader): Table {
  const widths = [Math.round(CONTENT * 0.6), CONTENT - Math.round(CONTENT * 0.6)];
  const lines = [
    `HỘ, CÁ NHÂN KINH DOANH: ${header.owner || "…"}`,
    `Địa chỉ: ${header.address || "…"}`,
    `Mã số thuế: ${header.tax_code || "…"}`,
  ];
  return new Table({
    width: { size: CONTENT, type: WidthType.DXA },
    columnWidths: widths,
    layout: TableLayoutType.FIXED,
    // Kẻ quanh khối nhưng bỏ đáy: tên sổ nằm ngay bên dưới nên có đáy là thừa —
    // đúng như `.book-head { border-bottom: 0 }` trên giao diện.
    borders: {
      top: GRID,
      left: GRID,
      right: GRID,
      bottom: NO_LINE,
      insideHorizontal: NO_LINE,
      insideVertical: NO_LINE,
    },
    rows: [
      new TableRow({
        children: [
          new TableCell({
            width: { size: widths[0], type: WidthType.DXA },
            margins: { top: 40, left: 60, bottom: 40, right: 60 },
            children: lines.map((l, i) => para([run(l)], { before: i === 0 ? 0 : 40 })),
          }),
          new TableCell({
            width: { size: widths[1], type: WidthType.DXA },
            margins: { top: 40, left: 60, bottom: 40, right: 60 },
            children: [
              para([run(header.form_ref, { italics: true })], { align: AlignmentType.RIGHT }),
            ],
          }),
        ],
      }),
    ],
  });
}

/** Bảng sổ: hàng cột lặp lại ở mỗi trang, hàng khối tô nền theo mẫu in. */
function bookTable(book: TaxBook): Table {
  const cols = book.columns;
  const widths = columnTwips(cols);
  const labelKey = bookLabelColumn(cols);

  const head = new TableRow({
    tableHeader: true,
    children: cols.map((col, i) =>
      dataCell({
        col,
        width: widths[i],
        text: col.label,
        fill: FILL_HEAD,
        bold: true,
        size: SIZE_BAND,
      }),
    ),
  });

  const body = book.rows.map((row) => {
    // Hàng khối (section) in hoa + nhỏ như `.book-row-section`; hàng tổng cùng
    // sổ in đậm; cột nhãn thì mọi dòng đều đậm (`.book-label`).
    const caps = row.kind === "section";
    const fill = row.kind === "section" || row.kind === "subtotal" ? FILL_BAND : undefined;
    return new TableRow({
      children: cols.map((col, i) => {
        const v = bookCellValue(row, col.key);
        return dataCell({
          col,
          width: widths[i],
          text: renderBookCell(row, col),
          fill,
          caps,
          size: caps ? SIZE_BAND : SIZE_BODY,
          bold: caps || row.kind === "total" || col.key === labelKey,
          negative: typeof v === "number" && v < 0,
        });
      }),
    });
  });

  return new Table({
    width: { size: widths.reduce((a, b) => a + b, 0), type: WidthType.DXA },
    columnWidths: widths,
    layout: TableLayoutType.FIXED,
    borders: {
      top: GRID,
      bottom: GRID,
      left: GRID,
      right: GRID,
      insideHorizontal: GRID,
      insideVertical: GRID,
    },
    rows: [head, ...body],
  });
}

/** Chữ ký cuối sổ — các dòng canh giữa như `.book-sign`. */
function signature(header: BookHeader): Paragraph[] {
  const lines = [
    header.sign_date ? `Ngày ${header.sign_date}` : "",
    header.signer,
    "(Ký, ghi rõ họ tên và đóng dấu nếu có)",
  ].filter((l) => l !== "");
  return lines.map((l, i) =>
    para([run(l, { italics: true, size: i === lines.length - 1 ? SIZE_NOTE : SIZE_BODY })], {
      align: AlignmentType.CENTER,
      before: i === 0 ? 240 : 40,
    }),
  );
}

/** Dựng file `.docx` cho một mẫu sổ — phần thuần dữ liệu, dễ đối chiếu. */
export function buildBookDoc(book: TaxBook): Document {
  const h = book.header;
  const children: (Paragraph | Table)[] = [
    headBox(h),
    // Tên sổ in hoa canh giữa — `.book-title`.
    para([run(book.title, { bold: true, size: SIZE_TITLE, caps: true })], {
      align: AlignmentType.CENTER,
      before: 144,
      after: 84,
    }),
  ];

  // Hai dòng dưới tiêu đề: địa điểm kinh doanh + kỳ kê khai — `.book-sub`.
  if (h.location_label) {
    children.push(
      para([run(`${h.location_label}: ${h.location || "…"}`)], {
        align: AlignmentType.CENTER,
        before: 30,
      }),
    );
  }
  children.push(
    para([run(`Kỳ kê khai: ${h.period || book.period_label}`)], {
      align: AlignmentType.CENTER,
      before: 30,
    }),
  );

  // "Đơn vị tính:" in nghiêng canh phải ngay trên bảng (S2d không có dòng này).
  if (h.unit) {
    children.push(
      para([run(`Đơn vị tính: ${h.unit}`, { italics: true })], {
        align: AlignmentType.RIGHT,
        before: 84,
        after: 84,
      }),
    );
  }

  children.push(bookTable(book), ...signature(h));

  return new Document({
    styles: { default: { document: { run: { font: FONT, size: SIZE_BODY } } } },
    sections: [
      {
        properties: {
          page: {
            size: {
              width: convertMillimetersToTwip(210),
              height: convertMillimetersToTwip(297),
              orientation: PageOrientation.PORTRAIT,
            },
            margin: { top: MARGIN, right: MARGIN, bottom: MARGIN, left: MARGIN },
          },
        },
        children,
      },
    ],
  });
}

/**
 * Dựng file và lưu qua util chung `downloadBinary` — trong app desktop mở hộp
 * thoại "Lưu file" của hệ điều hành, trong trình duyệt tải về như cũ.
 */
export async function exportBookDocx(book: TaxBook, filename: string): Promise<boolean> {
  const blob = await Packer.toBlob(buildBookDoc(book));
  return downloadBinary(`${filename}.docx`, new Uint8Array(await blob.arrayBuffer()), blob.type);
}
