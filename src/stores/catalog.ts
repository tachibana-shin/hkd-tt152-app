import { api } from "@/db";
import type { Product, Warehouse, Supplier, Customer, IndustryGroup } from "@/types";

export const useCatalogStore = defineStore("catalog", () => {
  const products = ref<Product[]>([]);
  const warehouses = ref<Warehouse[]>([]);
  const suppliers = ref<Supplier[]>([]);
  const customers = ref<Customer[]>([]);
  const industryGroups = ref<IndustryGroup[]>([]);
  const loading = ref(false);

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

  async function saveProduct(p: Partial<Product>) {
    await api.saveProduct(p);
    await loadAll();
  }

  async function deleteProduct(id: number) {
    await api.deleteProduct(id);
    await loadAll();
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

  const productByCode = (code: string) =>
    products.value.find((p) => p.code === code);

  return {
    products,
    warehouses,
    suppliers,
    customers,
    industryGroups,
    loading,
    loadAll,
    saveProduct,
    deleteProduct,
    saveWarehouse,
    saveSupplier,
    saveCustomer,
    productByCode,
  };
});