import { api } from "@/db";
import type { Invoice, InvoiceItem, InventoryCount, CountItemRow } from "@/types";

export const useInvoiceStore = defineStore("invoice", () => {
  const invoices = ref<Invoice[]>([]);
  const detail = ref<{ invoice: Invoice; items: InvoiceItem[] } | null>(null);
  const counts = ref<InventoryCount[]>([]);
  const countDetail = ref<CountItemRow[]>([]);
  const loading = ref(false);

  async function loadInvoices() {
    loading.value = true;
    try {
      invoices.value = await api.getInvoices();
    } finally {
      loading.value = false;
    }
  }

  async function loadDetail(id: number) {
    detail.value = await api.getInvoiceDetail(id);
  }

  async function saveDraft(args: {
    number: string;
    date: string;
    customer: string;
    customer_tax_code: string;
    items: { product_code: string; quantity: number; unit_price: number }[];
  }) {
    const res = await api.saveInvoice(args);
    await loadInvoices();
    return res;
  }

  async function loadCounts() {
    counts.value = await api.getInventoryCounts();
  }

  async function loadCountDetail(id: number) {
    countDetail.value = await api.getInventoryCountDetail(id);
  }

  async function saveCount(args: {
    date: string;
    note: string;
    items: { product_code: string; warehouse_code: string; counted_qty: number }[];
  }) {
    const res = await api.saveInventoryCount(args);
    await loadCounts();
    return res;
  }

  return {
    invoices,
    detail,
    counts,
    countDetail,
    loading,
    loadInvoices,
    loadDetail,
    saveDraft,
    loadCounts,
    loadCountDetail,
    saveCount,
  };
});