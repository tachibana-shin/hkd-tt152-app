<script setup lang="ts">
/**
 * Hộp thoại cổng HĐĐT dùng chung cho cả app: nhập captcha tay khi solver không
 * giải được, và báo mật khẩu mới sau khi tự đổi. App mở một bản duy nhất nên
 * các màn không cần tự quản lý captcha.
 */
import { usePortalSession } from "@/composables/usePortalSession";

const toast = useToast();
const {
  manualVisible,
  manualSvg,
  manualValue,
  manualBusy,
  manualError,
  newPassword,
  submitManual,
  onManualHide,
  dismissNewPassword,
} = usePortalSession();

/** Dialog mật khẩu mới mở khi nào app vừa tự sinh mật khẩu. */
const passwordDialogVisible = computed({
  get: () => newPassword.value !== "",
  set: (v: boolean) => {
    if (!v) dismissNewPassword();
  },
});

async function copyNewPassword() {
  if (!newPassword.value) return;
  try {
    await navigator.clipboard.writeText(newPassword.value);
    toast.add({ severity: "info", summary: "Đã sao chép mật khẩu mới", life: 2000 });
  } catch {
    toast.add({ severity: "warn", summary: "Không truy cập được clipboard", life: 3000 });
  }
}
</script>

<template>
  <!-- Nhập captcha thủ công -->
  <Dialog
    v-model:visible="manualVisible"
    modal
    header="Nhập captcha cổng HĐĐT"
    :style="{ width: '420px' }"
    @hide="onManualHide"
  >
    <div class="space-y-3">
      <p class="text-sm text-gray-600">
        App không tự giải được captcha. Nhập 6 ký tự trong ảnh dưới đây để đăng nhập cổng HĐĐT:
      </p>
      <div
        class="flex justify-center rounded border border-gray-200 bg-gray-50 p-3"
        v-html="manualSvg"
      />
      <InputText
        v-model="manualValue"
        class="w-full text-center text-lg tracking-widest"
        maxlength="6"
        placeholder="XXXXXX"
        @keyup.enter="submitManual"
      />
      <div v-if="manualError" class="text-sm text-red-600">{{ manualError }}</div>
    </div>
    <template #footer>
      <Button label="Bỏ qua" severity="secondary" text @click="manualVisible = false" />
      <Button label="Đăng nhập" icon="pi pi-check" :loading="manualBusy" @click="submitManual" />
    </template>
  </Dialog>

  <!-- Mật khẩu mới (sau khi tự đổi) -->
  <Dialog
    v-model:visible="passwordDialogVisible"
    modal
    header="Mật khẩu cổng đã được đổi"
    :style="{ width: '400px' }"
    :closable="false"
    :dismissable-mask="false"
  >
    <div class="space-y-3">
      <p class="text-sm text-gray-600">
        HĐĐT đã tự sinh mật khẩu mới và đăng nhập lại. Mật khẩu được lưu trong app; nếu bạn cần đăng
        nhập trực tiếp trên cổng web, hãy giữ lại mật khẩu dưới đây:
      </p>
      <div class="flex gap-2">
        <InputText :value="newPassword" readonly class="flex-1 font-mono" />
        <Button icon="pi pi-copy" severity="secondary" @click="copyNewPassword" />
      </div>
    </div>
    <template #footer>
      <Button label="Đã lưu" icon="pi pi-check" @click="dismissNewPassword" />
    </template>
  </Dialog>
</template>
