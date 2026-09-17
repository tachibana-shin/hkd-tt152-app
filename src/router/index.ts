import { createRouter, createWebHistory } from "vue-router";
import type { RouteRecordRaw } from "vue-router";

const routes: RouteRecordRaw[] = [
  { path: "/", name: "dashboard", component: () => import("@/views/Dashboard.vue"), meta: { title: "Tổng quan" } },
  { path: "/products", name: "products", component: () => import("@/views/Products.vue"), meta: { title: "Danh mục sản phẩm" } },
  { path: "/accounts", name: "accounts", component: () => import("@/views/Accounts.vue"), meta: { title: "Danh mục tài khoản" } },
  { path: "/catalogs", name: "catalogs", component: () => import("@/views/Catalogs.vue"), meta: { title: "Danh mục đối tác & kho" } },
  { path: "/payroll", name: "payroll", component: () => import("@/views/Payroll.vue"), meta: { title: "Bảng lương" } },
  { path: "/attendance", name: "attendance", component: () => import("@/views/Attendance.vue"), meta: { title: "Chấm công" } },
  { path: "/print/:voucherNo", name: "print-voucher", component: () => import("@/views/PrintVoucher.vue"), meta: { title: "In phiếu" } },
  { path: "/import", name: "import", component: () => import("@/views/ImportView.vue"), meta: { title: "Nhập liệu Excel" } },
  { path: "/audit", name: "audit", component: () => import("@/views/AuditLog.vue"), meta: { title: "Nhật ký hoạt động" } },
  { path: "/cash", name: "cash", component: () => import("@/views/Cash.vue"), meta: { title: "Phiếu thu / chi" } },
  { path: "/ledger", name: "ledger", component: () => import("@/views/Ledger.vue"), meta: { title: "Sổ nhật ký chung" } },
  { path: "/inbound", name: "inbound", component: () => import("@/views/Inbound.vue"), meta: { title: "Nhập kho" } },
  { path: "/outbound", name: "outbound", component: () => import("@/views/Outbound.vue"), meta: { title: "Xuất kho / Bán hàng" } },
  { path: "/inventory", name: "inventory", component: () => import("@/views/Inventory.vue"), meta: { title: "Tồn kho" } },
  { path: "/invoices", name: "invoices", component: () => import("@/views/Invoices.vue"), meta: { title: "Hóa đơn" } },
  { path: "/accounting", name: "accounting", component: () => import("@/views/Accounting.vue"), meta: { title: "Kế toán HKD" } },
  { path: "/users", name: "users", component: () => import("@/views/Users.vue"), meta: { title: "Người dùng & phân quyền" } },
  { path: "/profiles", name: "profiles", component: () => import("@/views/Profiles.vue"), meta: { title: "Hồ sơ HKD" } },
  { path: "/settings", name: "settings", component: () => import("@/views/Settings.vue"), meta: { title: "Cài đặt" } },
  { path: "/:pathMatch(.*)*", redirect: "/" },
];

const router = createRouter({
  history: createWebHistory(),
  routes,
});

export default router;