import { createApp } from "vue";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import Aura from "@primeuix/themes/aura";
import ToastService from "primevue/toastservice";
import ToastEventBus from "primevue/toasteventbus";
import ConfirmationService from "primevue/confirmationservice";
import Tooltip from "primevue/tooltip";
import router from "./router";
import App from "./App.vue";
import "primeicons/primeicons.css";
import "./style.css";

// Toast tự đóng mặc định: PrimeVue (ToastMessage.startTimer) chỉ tự đóng khi
// message có `life`; lệnh gọi toast.add({...}) không set life → toast dính vĩnh viễn.
// ToastService.add() → ToastEventBus.emit('add', msg), nên wrap emit để điền
// `life: 3000` khi thiếu (lệnh nào đã set life riêng vẫn được giữ nguyên).
const toastBus = ToastEventBus as unknown as {
  emit: (event: string, ...args: unknown[]) => void;
};
const toastEmit = toastBus.emit.bind(toastBus);
toastBus.emit = (event: string, ...args: unknown[]) => {
  if (event === "add" && args[0] && typeof args[0] === "object") {
    const msg = args[0] as { life?: number };
    toastEmit(event, { life: 3000, ...msg });
  } else {
    toastEmit(event, ...args);
  }
};

const app = createApp(App);

app.use(createPinia());
app.use(router);
app.use(PrimeVue, {
  theme: {
    preset: Aura,
    options: { darkModeSelector: ".dark" },
  },
  unstyled: false,
  license: 'Hacked by Tachibana Shin'
});
app.use(ToastService);
app.use(ConfirmationService);
app.directive("tooltip", Tooltip);

app.mount("#app");
