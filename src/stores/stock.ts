import { api } from "@/db";
import type { JournalEntryRow, StockLot, InventoryRow } from "@/types";

export const useStockStore = defineStore("stock", () => {
  const entries = ref<JournalEntryRow[]>([]);
  const summary = ref<InventoryRow[]>([]);
  const lots = ref<StockLot[]>([]);
  const loading = ref(false);

  /** `search` khớp số phiếu / mã hàng / khách / diễn giải / ghi chú. */
  async function loadEntries(entryType = "", fromDate = "", toDate = "", search = "") {
    loading.value = true;
    try {
      entries.value = await api.getJournalEntries(entryType, fromDate, toDate, search);
    } finally {
      loading.value = false;
    }
  }

  async function loadSummary() {
    loading.value = true;
    try {
      summary.value = await api.getInventorySummary();
    } finally {
      loading.value = false;
    }
  }

  async function loadLots(productCode = "") {
    loading.value = true;
    try {
      lots.value = await api.getStockLots(productCode);
    } finally {
      loading.value = false;
    }
  }

  async function inbound(args: {
    posting_date: string;
    voucher_no: string;
    description: string;
    supplier_code: string;
    warehouse_code: string;
    unit_code: string;
    note: string;
    items: {
      product_code: string;
      quantity: number;
      unit_price: number;
      discount: number; // số tiền CK — giá trị nhập kho = Thành tiền − Tiền CK
    }[];
    inbound_type: string; // purchase | production | other | adjust
    reference_no: string;
    vat_rate: number;
    debit_account: string;
    credit_account: string;
    /** Trả tiền ngay: tự tạo phiếu chi (PC) thanh toán cho nhà cung cấp. */
    pay_now: boolean;
    /** Hướng điều chỉnh khi inbound_type = "adjust": up (tăng) | down (giảm — trả lại NCC). */
    adjust_dir: "" | "up" | "down";
  }) {
    const res = await api.saveInbound(args);
    await Promise.all([loadEntries("PN"), loadSummary(), loadLots()]);
    return res;
  }

  async function outbound(args: {
    posting_date: string;
    voucher_no: string;
    description: string;
    customer_code: string;
    unit_code: string;
    note: string;
    items: {
      product_code: string;
      quantity: number;
      unit_price: number;
      industry_code: string;
    }[];
    /** Thu tiền ngay: tự tạo phiếu thu (PT) của khách hàng khi lưu. */
    receive_now: boolean;
    /** Loại xuất: "sale" (bán hàng) | "adjust" (điều chỉnh hóa đơn bán). */
    outbound_type: "" | "sale" | "adjust";
    /** Hướng điều chỉnh khi outbound_type = "adjust": down (khách trả lại) | up (tăng). */
    adjust_dir: "" | "up" | "down";
    /** Lập kèm hóa đơn bán hàng (tab "Hóa đơn") — mặc định bật cho phiếu bán. */
    create_invoice?: boolean;
    /** Thông tin hóa đơn lập kèm: số mặc định = số phiếu xuất. */
    invoice?: {
      number: string;
      e_invoice_no: string;
      e_invoice_symbol: string;
      e_invoice_date: string;
    };
  }) {
    const res = await api.saveOutbound(args);
    await Promise.all([loadEntries("PX"), loadSummary(), loadLots()]);
    return res;
  }

  return {
    entries,
    summary,
    lots,
    loading,
    loadEntries,
    loadSummary,
    loadLots,
    inbound,
    outbound,
  };
});
