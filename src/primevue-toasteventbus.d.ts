// primevue/toasteventbus không kèm file khai báo type (chỉ index.mjs).
declare module "primevue/toasteventbus" {
  interface ToastEventBus {
    on(event: string, cb: (message: unknown) => void): void;
    off(event: string, cb: (message: unknown) => void): void;
    emit(event: string, ...args: unknown[]): void;
  }
  const ToastEventBus: ToastEventBus;
  export default ToastEventBus;
}
