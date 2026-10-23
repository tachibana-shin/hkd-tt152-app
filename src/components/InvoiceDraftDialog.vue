<script setup lang="ts">
/**
 * Lập mới / sửa hóa đơn nháp — dùng chung cho màn Hóa đơn và màn Chờ xuất HĐĐT.
 *
 * Truyền `editInvoice` để sửa (nạp sẵn hóa đơn + dòng hàng), bỏ trống để lập mới
 * (tự sinh số hóa đơn kế tiếp). Lưu xong bắn `done` để màn cha nạp lại danh sách.
 */
import { storeToRefs } from "pinia";
import { useAuthStore } from "@/stores/auth";
import { useBusinessStore } from "@/stores/business";
import { useCatalogStore } from "@/stores/catalog";
import { useInvoiceStore } from "@/stores/invoice";
import LineItemsEditor from "@/components/LineItemsEditor.vue";
import ProductAliasDialog from "@/components/ProductAliasDialog.vue";
import ProductionLotDialog from "@/components/ProductionLotDialog.vue";
import { buildBomXlsx } from "@/utils/bomExcel";
import { buildQuoteXlsx } from "@/utils/quoteExcel";
import { flattenQuoteLines, type FlatQuoteSourceLine } from "@/utils/quoteFlat";
import { downloadBlob } from "@/utils/download";
import type { Invoice } from "@/types";
import { fmtInt as fmt, parseIsoDate, toIsoDate } from "@/utils/format";

const visible = defineModel<boolean>("visible", { default: false });
const props = defineProps<{ editInvoice?: Invoice | null }>();
const emit = defineEmits<{ done: [] }>();

const auth = useAuthStore();
const business = useBusinessStore();
const catalog = useCatalogStore();
const invoiceStore = useInvoiceStore();
const confirm = useConfirm();
const { products, customers, industryGroups, warehouses } = storeToRefs(catalog);
const { invoices } = storeToRefs(invoiceStore);
const toast = useToast();

const saving = ref(false);
/** Đang dựng file báo giá — khóa nút để không tải trùng. */
const quoteExporting = ref(false);
/** Đang dựng báo giá flat (tách định mức) — khóa riêng nút flat. */
const flatQuoteExporting = ref(false);
/** Mã hàng đang xuất Excel định mức từ một dòng (null = không chạy). */
const bomExporting = ref<string | null>(null);
const editingId = ref<number | null>(null);
const form = reactive({
  number: "",
  date: new Date() as Date,
  customer: "",
  customer_tax_code: "",
  items: [] as {
    product_code: string;
    quantity: number;
    unit_price: number;
    industry_code?: string;
    discount?: number;
    warehouse_code?: string;
    /** Tên hiển thị dòng (tên khác / alias) — rỗng = tên sản phẩm (F5). */
    line_name?: string;
  }[],
});

const isEdit = computed(() => editingId.value !== null);
const header = computed(() =>
  isEdit.value ? `Sửa hóa đơn nháp ${form.number}` : "Lập hóa đơn bán hàng (nháp)",
);
const actionLabel = computed(() => (isEdit.value ? "Lưu thay đổi" : "Lập hóa đơn"));

function nextInvoiceNo() {
  const nums = invoices.value
    .map((i) => parseInt(i.number.replace(/[^\d]/g, ""), 10))
    .filter((n) => !isNaN(n));
  const next = nums.length ? Math.max(...nums) + 1 : 1;
  form.number = `HD${String(next).padStart(4, "0")}`;
}

function addRow() {
  form.items.push({
    product_code: "",
    quantity: 1,
    unit_price: 0,
    industry_code: "",
    discount: 0,
    warehouse_code: "",
    line_name: "",
  });
}

function removeRow(i: number) {
  form.items.splice(i, 1);
}

// ─── Thêm khách hàng nhanh: dùng chung PartnerDialog (tự sinh mã + tra cứu MST) ───
const customerDialog = ref(false);

function onCustomerSaved(code: string) {
  const c = catalog.customers.find((x) => x.code === code);
  if (c) {
    form.customer = c.name;
    if (c.tax_code) form.customer_tax_code = c.tax_code;
  }
}

// ─── Nút ở header bảng "Mặt hàng": thêm tên khác + tạo nhanh lô sản xuất ───
const aliasDialog = ref(false);
const lotDialog = ref(false);
const editor = ref<InstanceType<typeof LineItemsEditor> | null>(null);

// ─── Gợi ý thông minh: chọn hàng CHẾ TẠO mà kho thiếu → mở tạo lô ngay ───
/** Số liệu dòng vừa báo thiếu (điền sẵn vào popup tạo lô); null = mở thủ công. */
const lotSuggest = ref<{ code: string; quantity: number; warehouseCode: string } | null>(null);
/** Đang có prompt gợi ý — đừng xếp hàng cảnh báo nếu chọn liên tiếp. */
const prompting = ref(false);

/** Mở tạo lô thủ công từ header — tự điền khi hóa đơn chỉ có 1 hàng chế tạo. */
async function openLot() {
  lotSuggest.value = null;
  // Định mức + tồn phải nạp xong mới biết hàng nào là hàng chế tạo; lỗi nạp
  // thì vẫn mở form trống như cũ (gợi ý chỉ là bổ trợ).
  if (auth.canStock) {
    try {
      await Promise.all([catalog.loadBoms(), catalog.loadOnhand()]);
    } catch {
      // bỏ qua — mở form để người dùng tự chọn
    }
    // Chỉ 1 mặt hàng chế tạo trên hóa đơn nháp → điền sẵn đúng hàng đó + tổng
    // SL các dòng của nó để khỏi phải tìm lại; nhiều mặt hàng thì không đoán.
    const codes = [
      ...new Set(
        form.items
          .map((it) => it.product_code)
          .filter(
            (code) => code && !catalog.productByCode(code)?.is_service && catalog.hasBom(code),
          ),
      ),
    ];
    if (codes.length === 1) {
      const code = codes[0]!;
      const lines = form.items.filter((it) => it.product_code === code);
      const quantity = lines.reduce((sum, it) => sum + (Number(it.quantity) || 0), 0);
      // Các dòng cùng 1 kho thì điền kho đó; khác kho thì để form tự chọn.
      const wh = new Set(lines.map((it) => it.warehouse_code ?? ""));
      lotSuggest.value = {
        code,
        quantity: quantity > 0 ? quantity : 1,
        warehouseCode: wh.size === 1 ? [...wh][0]! : "",
      };
    }
  }
  lotDialog.value = true;
}

/**
 * Người dùng vừa chọn hàng trên dòng: nếu là hàng CHẾ TẠO (có định mức) mà kho
 * đang thiếu → mở prompt tạo lô sản xuất ngay với sản lượng = phần thiếu, không
 * để tới lúc bấm Lưu mới vỡ lẽ. Từ chối thì thôi — dòng vẫn hiện cảnh báo đỏ
 * và khâu lưu vẫn còn confirm chặn riêng.
 */
async function onLinePicked(it: {
  product_code: string;
  quantity: number;
  warehouse_code?: string;
}) {
  // Tạo lô cần quyền kho; đang có prompt khác thì thôi (đừng xô nhau).
  if (!it.product_code || prompting.value || !auth.canStock) return;
  const p = catalog.productByCode(it.product_code);
  if (!p || p.is_service) return; // dịch vụ không theo dõi tồn
  try {
    // Nạp định mức (1 lần trong phiên) + tồn (phòng khi mở popup vội, map
    // chưa kịp nạp xong → soi nhầm "thiếu" là gợi ý sai, thà đợi tí còn hơn).
    await Promise.all([catalog.loadBoms(), catalog.loadOnhand()]);
  } catch {
    return; // soi không được thì bỏ — gợi ý chỉ là bổ trợ, thiếu kho vẫn chặn lúc lưu
  }
  if (!catalog.hasBom(it.product_code)) return; // không phải hàng chế tạo
  const avail = catalog.onhandAt(it.product_code, it.warehouse_code ?? "");
  const short = (it.quantity || 0) - avail;
  if (!(short > 1e-9)) return;

  prompting.value = true;
  confirm.require({
    header: "Thiếu tồn kho — hàng chế tạo",
    message: `${p.name} (${p.code}): cần ${fmt(it.quantity)}, kho chỉ có ${fmt(
      avail,
    )} → thiếu ${fmt(short)}. Đây là hàng có định mức — mở form tạo lô sản xuất ngay? (Lô chỉ được tạo khi bấm Lưu trong form. Phiếu kho độc lập với hoá đơn nháp — xoá hoá đơn không xoá lô.)`,
    icon: "pi pi-factory",
    acceptLabel: "Mở tạo lô",
    rejectLabel: "Để sau",
    accept: () => {
      prompting.value = false;
      lotSuggest.value = {
        code: it.product_code,
        quantity: short,
        warehouseCode: it.warehouse_code ?? "",
      };
      lotDialog.value = true;
    },
    reject: () => {
      prompting.value = false;
    },
  });
}

/**
 * Thêm tên khác từ header (chưa đứng trên dòng nào) → gắn thẳng tên vừa thêm
 * vào dòng đầu đang dùng đúng hàng đó và CHƯA đặt tên riêng: in ra đúng tên
 * người vừa gõ. Hàng chưa có trên hóa đơn thì chỉ lưu danh mục — báo luôn cho
 * người dùng khỏi tưởng bảng hỏng.
 */
function onAliasSaved(code: string, added: string[]) {
  const row = form.items.find((it) => it.product_code === code && !it.line_name);
  if (row) row.line_name = added[0];
  const detail = `${added.join(", ")} → ${catalog.productByCode(code)?.name ?? code}`;
  toast.add({
    severity: "success",
    summary: "Đã thêm tên khác",
    detail: row
      ? `${detail} · gắn vào dòng hóa đơn`
      : `${detail} · hàng chưa có trên hóa đơn nên chỉ lưu vào danh mục`,
  });
}

/**
 * Tạo lô ngay trong popup: nạp lại danh mục + tồn (để soi thiếu tồn không dùng
 * số cũ) rồi gắn thành phẩm vừa tạo vào dòng trống — không phải quay lại ô chọn.
 */
async function onLotDone(payload: { voucherNo: string; productCode: string }) {
  try {
    await Promise.all([catalog.loadAll(), catalog.loadOnhand(true)]);
  } catch (e) {
    // Nạp lại lỗi vẫn gắn hàng được — popup thiếu tồn sẽ báo khi lưu.
    toast.add({ severity: "warn", summary: "Không làm mới được tồn kho", detail: String(e) });
  }
  // Lô tạo từ gợi ý (hàng ĐANG đứng trên hóa đơn) → nạp tồn xong là dòng tự
  // hết cảnh báo; thêm dòng mới nữa sẽ trùng hàng trên cùng hóa đơn.
  if (form.items.some((it) => it.product_code === payload.productCode)) return;
  let index = form.items.findIndex((it) => !it.product_code);
  if (index < 0) {
    addRow();
    index = form.items.length - 1;
  }
  editor.value?.assignProduct(index, payload.productCode);
}

/** Mỗi lần mở dialog: lập mới (tự sinh số) hoặc nạp hóa đơn cần sửa. */
watch(
  [visible, () => props.editInvoice?.id],
  async ([open, id], [wasOpen, wasId]) => {
    if (!open || (wasOpen && id === wasId)) return;
    // Danh mục phải sẵn sàng cho ô chọn (mặt hàng, khách hàng, kho, nhóm
    // ngành) — màn Chờ xuất HĐĐT không tự nạp như màn Hóa đơn nên popup tự
    // bảo đảm khi mở, không phụ thuộc màn cha đứng sau.
    void catalog.loadAll();
    // Định mức cần cho nút Excel từng dòng + báo giá flat. Nạp nền, không chặn form.
    void catalog.loadBoms();
    // Soi thiếu tồn phải theo số mới nhất — map chỉ nạp 1 lần ở nơi khác nên
    // ép tải lại mỗi lần mở (nhanh, không chặn render form).
    void catalog.loadOnhand(true);
    // Hết gợi ý/confirm của lần mở trước — đừng vướng lại số liệu dòng cũ.
    prompting.value = false;
    lotSuggest.value = null;
    if (!id) {
      editingId.value = null;
      Object.assign(form, {
        date: new Date(),
        customer: "",
        customer_tax_code: "",
        items: [],
        // Số để trống tới khi gợi ý xong — tránh hiện tạm một số đã tồn tại
        // (mở app mới thì store chưa nạp danh sách hóa đơn).
        number: "",
      });
      saving.value = true;
      try {
        await invoiceStore.loadInvoices();
        nextInvoiceNo();
      } catch {
        // Không tải được danh sách → vẫn gợi ý từ số đang có trong store.
        nextInvoiceNo();
      } finally {
        saving.value = false;
      }
      return;
    }
    saving.value = true;
    try {
      const d = await invoiceStore.loadDetail(id);
      if (!d) return;
      editingId.value = id;
      Object.assign(form, {
        number: d.invoice.number,
        date: parseIsoDate(d.invoice.date) ?? new Date(),
        customer: d.invoice.customer ?? "",
        customer_tax_code: d.invoice.customer_tax_code ?? "",
        items: d.items.map((it) => ({
          product_code: it.product_code ?? "",
          quantity: it.quantity,
          unit_price: it.unit_price,
          industry_code: it.industry_code ?? "",
          discount: it.discount ?? 0,
          warehouse_code: it.warehouse_code ?? "",
          // Giữ nguyên TÊN người dùng đã chọn (có thể là tên khác / alias) —
          // `product_name` trả về đã là tên hiển thị, nhưng cần bản gốc để lưu.
          line_name: it.line_name ?? "",
        })),
      });
    } catch (e) {
      toast.add({ severity: "error", summary: "Không mở được hóa đơn", detail: String(e) });
      visible.value = false;
    } finally {
      saving.value = false;
    }
  },
  { immediate: true },
);

/** Dòng hàng hợp lệ để báo giá — báo lỗi ngay nếu thiếu hàng/khách. */
function requireQuoteRows() {
  const rows = form.items.filter((it) => it.product_code);
  if (!rows.length) {
    toast.add({
      severity: "warn",
      summary: "Chưa có mặt hàng",
      detail: "Chọn ít nhất 1 hàng hóa/dịch vụ trước khi xuất báo giá.",
    });
    return null;
  }
  if (!form.customer.trim()) {
    toast.add({
      severity: "warn",
      summary: "Thiếu khách hàng",
      detail: "Nhập khách hàng để báo giá đúng người nhận.",
    });
    return null;
  }
  return rows;
}

/** Thông tin hộ kinh doanh ở đầu báo giá — thiếu cũng vẫn xuất được phần bảng. */
async function quoteSeller() {
  if (!business.config) {
    try {
      await business.load();
    } catch {
      // Bỏ qua: báo giá vẫn dùng được với các dòng hàng bên dưới.
    }
  }
  const seller = business.config;
  return seller
    ? {
        name: seller.name,
        taxCode: seller.tax_code,
        address: seller.location || seller.address,
        phone: seller.phone,
      }
    : undefined;
}

/** Dòng form → dòng Excel, giữ đúng tên hiển thị/đơn giá/chiết khấu đang nhập. */
function quoteLine(it: (typeof form.items)[number]): FlatQuoteSourceLine {
  const product = catalog.productByCode(it.product_code);
  return {
    product_code: it.product_code,
    name: it.line_name || product?.name || it.product_code,
    unit: product?.unit ?? "",
    quantity: it.quantity || 0,
    price: it.unit_price || 0,
    discount: it.discount ?? 0,
  };
}

/**
 * Xuất Excel báo giá từ đúng các dòng đang nhập trên form (kể cả sửa đổi chưa
 * lưu): gói sửa chữa có nhân công + vật tư vẫn ra bảng giá gửi khách ngay.
 * Đơn giá/chiết khấu/thành tiền lấy y như hóa đơn nháp, không đọc lại từ DB.
 */
async function exportQuote() {
  if (quoteExporting.value) return;
  const rows = requireQuoteRows();
  if (!rows) return;

  quoteExporting.value = true;
  try {
    const seller = await quoteSeller();
    const blob = await buildQuoteXlsx({
      quoteNumber: form.number,
      quoteDate: form.date,
      seller,
      customer: form.customer,
      customerTaxCode: form.customer_tax_code,
      lines: rows.map(quoteLine).map((it) => ({
        code: it.product_code,
        name: it.name,
        unit: it.unit,
        quantity: it.quantity,
        price: it.price,
        discount: it.discount,
      })),
    });
    const total = rows.reduce(
      (sum, it) => sum + (it.quantity || 0) * (it.unit_price || 0) - (it.discount ?? 0),
      0,
    );
    const date = toIsoDate(form.date).replace(/-/g, "");
    const safeNo = (form.number || "nhap").replace(/[^\w.-]+/g, "-");
    const saved = await downloadBlob(`bao-gia-${safeNo}-${date}.xlsx`, blob);
    if (saved) {
      toast.add({
        severity: "success",
        summary: "Đã xuất Excel báo giá",
        detail: `${form.customer} — ${rows.length} dòng • Tổng ${fmt(total)} đ`,
      });
    }
  } catch (e) {
    toast.add({
      severity: "error",
      summary: "Không xuất được Excel báo giá",
      detail: String(e),
    });
  } finally {
    quoteExporting.value = false;
  }
}

/**
 * Báo giá flat: thay mỗi hàng chế tạo (có định mức) bằng các dòng vật tư/dịch
 * vụ con, số lượng nhân theo số lượng thành phẩm đang báo giá. Hàng thường và
 * dịch vụ không có định mức giữ nguyên.
 */
async function exportFlatQuote() {
  if (flatQuoteExporting.value) return;
  const rows = requireQuoteRows();
  if (!rows) return;

  flatQuoteExporting.value = true;
  try {
    // Định mức có thể vừa sửa ở tab khác → lấy bản mới nhất trước khi tách.
    await catalog.loadBoms(true);
    const lines = flattenQuoteLines(rows.map(quoteLine), catalog.boms);
    if (!lines.length) {
      toast.add({
        severity: "warn",
        summary: "Không có dòng nào để báo giá flat",
        detail: "Các mặt hàng đang nhập chưa có định mức để tách.",
      });
      return;
    }

    const seller = await quoteSeller();
    const blob = await buildQuoteXlsx({
      quoteNumber: form.number,
      quoteDate: form.date,
      flat: true,
      seller,
      customer: form.customer,
      customerTaxCode: form.customer_tax_code,
      lines: lines.map((it) => ({
        code: it.code,
        name: it.name,
        unit: it.unit,
        quantity: it.quantity,
        price: it.price,
        discount: it.discount ?? 0,
      })),
    });
    const total = lines.reduce(
      (sum, it) => sum + (it.quantity || 0) * (it.price || 0) - (it.discount ?? 0),
      0,
    );
    const date = toIsoDate(form.date).replace(/-/g, "");
    const safeNo = (form.number || "nhap").replace(/[^\w.-]+/g, "-");
    const saved = await downloadBlob(`bao-gia-flat-${safeNo}-${date}.xlsx`, blob);
    if (saved) {
      toast.add({
        severity: "success",
        summary: "Đã xuất Excel báo giá flat",
        detail: `${form.customer} — ${lines.length} dòng • Tổng ${fmt(total)} đ`,
      });
    }
  } catch (e) {
    toast.add({
      severity: "error",
      summary: "Không xuất được Excel báo giá flat",
      detail: String(e),
    });
  } finally {
    flatQuoteExporting.value = false;
  }
}

/**
 * Xuất Excel định mức của MỘT hàng chế tạo ngay trên dòng hóa đơn — cùng mẫu
 * với nút "Xuất Excel" ở tab Định mức vật tư, nhưng nhân sẵn số lượng đang báo
 * giá để gửi khách/đối chiếu nhanh.
 */
async function exportLineBom(it: { product_code: string; quantity: number; line_name?: string }) {
  if (!it.product_code || bomExporting.value) return;
  bomExporting.value = it.product_code;
  try {
    await catalog.loadBoms(true);
    const rows = catalog.boms.filter((b) => b.product_code === it.product_code);
    if (!rows.length) {
      toast.add({
        severity: "warn",
        summary: "Hàng chưa có định mức",
        detail: "Khai định mức ở Sản phẩm → tab Định mức vật tư trước khi xuất.",
      });
      return;
    }
    const product = catalog.productByCode(it.product_code);
    const blob = await buildBomXlsx({
      productCode: it.product_code,
      productName: it.line_name || product?.name || it.product_code,
      productQuantity: it.quantity || 0,
      lines: rows.map((b) => ({
        code: b.material_code,
        name: b.material_name,
        unit: b.material_unit,
        quantity: b.quantity,
        price: b.unit_price,
      })),
    });
    const date = toIsoDate(form.date).replace(/-/g, "");
    const safeCode = it.product_code.replace(/[^\w.-]+/g, "-");
    const saved = await downloadBlob(`dinh-muc-${safeCode}-${date}.xlsx`, blob);
    if (saved) {
      toast.add({
        severity: "success",
        summary: "Đã xuất Excel định mức",
        detail: `${it.line_name || product?.name || it.product_code} × ${it.quantity || 0} — ${rows.length} dòng`,
      });
    }
  } catch (e) {
    toast.add({
      severity: "error",
      summary: "Không xuất được Excel định mức",
      detail: String(e),
    });
  } finally {
    bomExporting.value = null;
  }
}

async function save() {
  // Chốt bấm nhiều lần: lần gọi sau bỏ qua, nếu không sẽ lập ra nhiều hóa đơn
  // trùng số (cùng đọc một số tự sinh ở form).
  if (saving.value) return;
  // Kiểu nhập liên tục (pre-input) luôn để lại dòng trống cuối → bỏ dòng chưa chọn hàng.
  const rows = form.items.filter((it) => it.product_code);
  if (!rows.length || !form.customer) {
    toast.add({
      severity: "warn",
      summary: "Thiếu thông tin",
      detail: "Cần khách hàng và ít nhất 1 mặt hàng",
    });
    return;
  }

  // Soi tồn NGAY LÚC CHỐT bằng backend (đúng logic lúc lưu thật, kể cả phần
  // hàng đã bị các hóa đơn nháp khác "giữ chỗ" — số trên dòng có thể lạc hậu
  // sau khi vừa tạo lô / sửa nháp khác). Kiểm không được thì bỏ tường báo
  // trước: khâu lưu vẫn tự soi thiếu và báo như cũ, lỗi nhỏ không cản việc.
  let missing: string[] = [];
  try {
    missing = await invoiceStore.checkStock({ items: rows, except_id: editingId.value });
  } catch (e) {
    toast.add({
      severity: "warn",
      summary: "Không kiểm được tồn kho trước khi lưu",
      detail: String(e),
    });
    return persist(rows);
  }

  if (missing.length) {
    // Nháp THÌ vẫn lưu được (HĐĐT đã xuất cũng cần thời gian mới lên cổng thông
    // tin điện tử) — nhưng phải hỏi trước; ghi số HĐĐT sau này vẫn bị chặn.
    confirm.require({
      header: "Tồn kho đang thiếu",
      message: `${missing.join("; ")}. Vẫn lưu nháp? Hóa đơn nháp lưu được, nhưng đến lúc ghi số HĐĐT sẽ bị chặn nếu vẫn thiếu kho.`,
      icon: "pi pi-exclamation-triangle",
      acceptLabel: "Vẫn lưu nháp",
      rejectLabel: "Quay lại",
      accept: async () => {
        await persist(rows);
      },
    });
    return;
  }
  await persist(rows);
}

async function persist(rows: typeof form.items) {
  saving.value = true;
  try {
    const payload = {
      number: form.number,
      date: toIsoDate(form.date),
      customer: form.customer,
      customer_tax_code: form.customer_tax_code,
      items: rows.map((it) => ({ ...it })),
    };
    const res = JSON.parse(
      editingId.value
        ? await invoiceStore.updateInvoice({ id: editingId.value, ...payload })
        : await invoiceStore.saveDraft(payload),
    );
    // Backend luôn báo các dòng thiếu tồn dù đã lưu — gộp vào toast duy nhất.
    const shortage: string[] = res.shortage ?? [];
    toast.add({
      severity: shortage.length ? "warn" : "success",
      summary: editingId.value ? `Đã sửa hóa đơn ${form.number}` : `Đã lập hóa đơn ${form.number}`,
      detail: [
        `Tổng tiền: ${fmt(res.total)} đ • Thuế phải nộp (tỷ lệ nhóm ngành): ${fmt(res.tax_payable ?? 0)} đ`,
        ...(shortage.length ? [`Thiếu tồn kho: ${shortage.join("; ")}`] : []),
      ].join(" • "),
    });
    visible.value = false;
    emit("done");
  } catch (e) {
    toast.add({ severity: "error", summary: "Không lưu được hóa đơn", detail: String(e) });
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <AppDialog
    v-model:visible="visible"
    :header="header"
    width="max-w-6xl"
    :action-label="actionLabel"
    :saving="saving"
    :show-action="auth.canAccounting"
    @action="save"
  >
    <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4 py-2">
      <FormField label="Số hóa đơn">
        <!-- Lập mới tự sinh số; sửa thì người dùng được sửa lại. -->
        <InputText v-model="form.number" :disabled="!isEdit" size="small" />
      </FormField>
      <FormField label="Ngày lập">
        <DatePicker v-model="form.date" dateFormat="dd/mm/yy" size="small" class="w-full" />
      </FormField>
      <FormField label="MST khách hàng">
        <InputText v-model="form.customer_tax_code" placeholder="Mã số thuế" size="small" />
      </FormField>
      <FormField label="Khách hàng" required class="col-span-3">
        <div class="flex gap-2">
          <Select
            v-model="form.customer"
            :options="customers"
            optionLabel="name"
            optionValue="name"
            :editable="true"
            filter
            size="small"
            class="w-full"
            placeholder="Chọn hoặc nhập tên khách hàng"
            @change="
              (e: any) => {
                const c = customers.find((x) => x.name === e.value) as any;
                if (c?.tax_code) form.customer_tax_code = (c as any).tax_code;
              }
            "
          />
          <Button
            v-if="auth.canAccounting"
            icon="pi pi-plus"
            text
            rounded
            severity="secondary"
            aria-label="Thêm khách hàng nhanh"
            v-tooltip="'Thêm khách hàng nhanh'"
            @click="customerDialog = true"
          />
        </div>
      </FormField>
    </div>

    <LineItemsEditor
      ref="editor"
      :items="form.items"
      :products="products"
      :industry-groups="industryGroups"
      :warehouses="warehouses"
      :can-edit="auth.canAccounting"
      price-field="sale_price"
      show-industry
      show-amount
      amount-input
      show-discount
      show-unit
      show-warehouse
      compact
      show-add-product
      show-bom-export
      info="Nhập thẳng Thành tiền từng dòng (Đơn giá tự tính = Thành tiền ÷ SL, đổi SL được). Nhóm ngành tự lấy theo sản phẩm (đổi được trên dòng). Tiền CK trừ vào giá trị dòng (Thành tiền − CK). Hàng hóa phải có đủ tồn kho theo đúng kho xuất trên dòng; sản phẩm dịch vụ (nhân công...) không cần tồn kho."
      total-label="Tổng tiền:"
      @add="addRow"
      @remove="removeRow"
      @picked="onLinePicked"
      @export-bom="exportLineBom"
    >
      <template #actions>
        <Button
          label="Xuất Excel báo giá"
          icon="pi pi-file-excel"
          size="small"
          text
          :loading="quoteExporting"
          :disabled="!form.items.some((it) => it.product_code)"
          data-testid="header-export-quote"
          v-tooltip="'Xuất báo giá từ các dòng đang nhập — kể cả thay đổi chưa lưu'"
          @click="exportQuote"
        />
        <Button
          label="Excel flat"
          icon="pi pi-sitemap"
          size="small"
          text
          :loading="flatQuoteExporting"
          :disabled="!form.items.some((it) => it.product_code)"
          data-testid="header-export-quote-flat"
          v-tooltip="'Báo giá flat: tách hàng chế tạo thành vật tư/dịch vụ theo định mức'"
          @click="exportFlatQuote"
        />
        <Button
          label="Thêm tên khác"
          icon="pi pi-tag"
          size="small"
          text
          :disabled="!auth.canAccounting"
          data-testid="header-add-alias"
          v-tooltip="'Thêm tên khác cho một mặt hàng'"
          @click="aliasDialog = true"
        />
        <Button
          label="Tạo lô sản xuất"
          icon="pi pi-factory"
          size="small"
          text
          :disabled="!auth.canStock"
          data-testid="header-create-lot"
          v-tooltip="'Tạo nhanh lô sản xuất — thành phẩm được gắn vào dòng trống'"
          @click="openLot"
        />
      </template>
    </LineItemsEditor>

    <PartnerDialog
      v-model:visible="customerDialog"
      kind="customer"
      :show-action="auth.canAccounting"
      @saved="onCustomerSaved"
    />

    <!-- Thêm tên khác từ header bảng — cùng dialog với icon trên từng dòng. -->
    <ProductAliasDialog v-model:visible="aliasDialog" @saved="onAliasSaved" />
    <!-- Tạo nhanh lô sản xuất trong lúc đang lập hóa đơn (kèm gợi ý từ dòng thiếu). -->
    <ProductionLotDialog
      v-model:visible="lotDialog"
      :product-code="lotSuggest?.code"
      :quantity="lotSuggest?.quantity"
      :warehouse-code="lotSuggest?.warehouseCode"
      @done="onLotDone"
    />
  </AppDialog>
</template>
