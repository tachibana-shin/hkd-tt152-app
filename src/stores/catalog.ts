import { api } from "@/db";
import type { Product, Warehouse, Supplier, Customer, IndustryGroup } from "@/types";

export const useCatalogStore = defineStore("catalog", () => {
  const products = ref<Product[]>([]);
  const warehouses = ref<Warehouse[]>([]);
  const suppliers = ref<Supplier[]>([]);
  const customers = ref<Customer[]>([]);
  const industryGroups = ref<IndustryGroup[]>([]);
  const loading = ref(false);

  // Trang sản phẩm cho màn Danh mục sản phẩm (lazy load — không tải toàn bộ).
  const pageProducts = ref<Product[]>([]);
  const totalProducts = ref(0);
  const productsLoading = ref(false);

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
    await Promise.all(ids.map((id) => api.deleteProduct(id)));
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

  const productByCode = (code: string) =>
    products.value.find((p) => p.code === code);

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
    loadAll,
    loadIndustryGroups,
    loadProductsPage,
    saveProduct,
    deleteProduct,
    deleteProducts,
    saveWarehouse,
    saveSupplier,
    saveCustomer,
    saveIndustryGroup,
    deleteIndustryGroup,
    productByCode,
  };
});