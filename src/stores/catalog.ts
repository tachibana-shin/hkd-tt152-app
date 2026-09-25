import { api } from "@/db";
import type { Product, Warehouse, Supplier, Customer, IndustryGroup } from "@/types";

export const useCatalogStore = defineStore("catalog", () => {
  const products = ref<Product[]>([]);
  const warehouses = ref<Warehouse[]>([]);
  const suppliers = ref<Supplier[]>([]);
  const customers = ref<Customer[]>([]);
  const industryGroups = ref<IndustryGroup[]>([]);
  const loading = ref(false);
  const loadingOnhand = ref(false);

  // Trang sản phẩm cho màn Danh mục sản phẩm (lazy-load theo event PrimeVue).
  const pageProducts = ref<Product[]>([]);
  const totalProducts = ref(0);
  const productsLoading = ref(false);

  // Tồn theo (sản phẩm × kho) — khóa "product|kho" (kho rỗng = tổng mọi kho).
  // Nạp lazy qua loadOnhand chỉ khi popup cần (bảng dòng có cột Kho/Tồn).
  const onhand = ref(new Map<string, number>());

  /** Nạp tất cả lô tồn → cộng dồn theo (sản phẩm × kho) cho cảnh báo thiếu tồn. */
  async function loadOnhand() {
    if (onhand.value.size || loadingOnhand.value) return;
    loadingOnhand.value = true;
    try {
      const lots = await api.getStockLots();
      const m = new Map<string, number>();
      for (const lot of lots) {
        const keyAll = lot.product_code + "|";
        m.set(keyAll, (m.get(keyAll) ?? 0) + lot.quantity);
        const keyWh = lot.product_code + "|" + lot.warehouse_code;
        m.set(keyWh, (m.get(keyWh) ?? 0) + lot.quantity);
      }
      onhand.value = m;
    } finally {
      loadingOnhand.value = false;
    }
  }

  /** Tồn còn lại của sản phẩm tại kho (rỗng → tổng mọi kho); 0 nếu chưa nạp. */
  function onhandAt(productCode: string, warehouseCode = "") {
    return onhand.value.get(productCode + "|" + warehouseCode) ?? 0;
  }

  async function loadAll() {
    loading.value = true;
    try {
      const [p, w, s, c, ig] = await Promise.all([
        api.getProducts(),
        api.getWarehouses(),
        api.getSuppliers(),
        api.getCustomers(),
        api.getIndustryGroups(),
      ]);
      products.value = p;
      warehouses.value = w;
      suppliers.value = s;
      customers.value = c;
      industryGroups.value = ig;
    } finally {
      loading.value = false;
    }
  }

  /** Chỉ nạp danh mục nhóm ngành (màn Sản phẩm lazy-load không gọi loadAll). */
  async function loadIndustryGroups() {
    industryGroups.value = await api.getIndustryGroups();
  }

  /** Lazy-load 1 trang sản phẩm theo event PrimeVue (first/rows/sort/filters). */
  async function loadProductsPage(lazyEvent: unknown) {
    productsLoading.value = true;
    try {
      const { rows, total } = await api.getProductsPage(lazyEvent);
      pageProducts.value = rows;
      totalProducts.value = total;
    } finally {
      productsLoading.value = false;
    }
  }

  async function saveProduct(p: Partial<Product>) {
    await api.saveProduct(p);
    // Làm mới danh sách đầy đủ — thêm nhanh từ phiếu nhập cần dòng mới
    // hiện ngay trong ô chọn sản phẩm (màn Sản phẩm lazy-load riêng).
    products.value = await api.getProducts();
  }

  async function deleteProduct(id: number) {
    await api.deleteProduct(id);
  }

  /** Xóa nhiều sản phẩm (dùng cho xóa hàng loạt theo checkbox). */
  async function deleteProducts(ids: number[]) {
    await api.deleteProducts(ids);
  }

  /**
   * Gán nhóm ngành PPHH cho hàng hóa chưa có nhóm (nút ở màn Sản phẩm).
   * Trả về số đã gán + số hàng hóa giữ nguyên nhóm cũ để view báo toast.
   */
  async function assignGoodsIndustry() {
    return api.assignGoodsIndustry();
  }

  async function saveWarehouse(code: string, name: string) {
    await api.saveWarehouse(code, name);
    await loadAll();
  }

  async function saveSupplier(s: Partial<Supplier>) {
    await api.saveSupplier(s);
    await loadAll();
  }

  async function saveCustomer(c: Partial<Customer>) {
    await api.saveCustomer(c);
    await loadAll();
  }

  async function saveIndustryGroup(g: {
    code: string;
    name: string;
    vat_rate: number;
    pit_rate: number;
  }) {
    await api.saveIndustryGroup(g);
    await loadIndustryGroups();
  }

  async function deleteIndustryGroup(code: string) {
    await api.deleteIndustryGroup(code);
    await loadIndustryGroups();
  }

  const productByCode = (code: string) => products.value.find((p) => p.code === code);

  return {
    products,
    warehouses,
    suppliers,
    customers,
    industryGroups,
    loading,
    pageProducts,
    totalProducts,
    productsLoading,
    onhand,
    loadingOnhand,
    loadOnhand,
    onhandAt,
    loadAll,
    loadIndustryGroups,
    loadProductsPage,
    saveProduct,
    deleteProduct,
    deleteProducts,
    assignGoodsIndustry,
    saveWarehouse,
    saveSupplier,
    saveCustomer,
    saveIndustryGroup,
    deleteIndustryGroup,
    productByCode,
  };
});
