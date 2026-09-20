<script setup lang="ts">
import { api } from "@/db";
import type { HddtStatus } from "@/types";

const toast = useToast();

const loading = ref(false);
const saving = ref(false);
const busy = ref(false); // đang đăng nhập

const cfg = reactive({ username: "", password: "", base_url: "" });
const hasPassword = ref(false);
const configured = ref(false);
const showPassword = ref(false);
const savedPassword = ref("");
const showSavedPassword = ref(false);

const status = ref<HddtStatus | null>(null);
const lastError = ref("");

// ─── Captcha nhập tay (khi tự giải thất bại) ───
const manualVisible = ref(false);
const manualSvg = ref("");
const manualKey = ref("");
const manualValue = ref("");
const manualBusy = ref(false);
const manualError = ref("");
/** Đang chờ người dùng nhập captcha → tạm ngừng tự đăng nhập. */
const manualStopped = ref(false);

/** Tự đổi mật khẩu khi sắp hết hạn (mặc định tắt — khôi phục mật khẩu bị mất rất khó). */
const autoChange = ref(false);
const changeBusy = ref(false);
const pwDialogVisible = ref(false);
const pwDialogNew = ref("");

let timer: ReturnType<typeof setInterval> | null = null;

/** Tự đăng nhập lại trước khi token hết hạn 1 giờ. */
const RELOGIN_AHEAD = 3600;

const tokenText = computed(() => {
  const s = status.value?.seconds_left ?? 0;
  if (!status.value?.logged_in || s <= 0) return "—";
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (h > 0) return `${h} giờ ${m} phút`;
  if (m > 0) return `${m} phút`;
  return `${s} giây`;
});

const userJson = computed(() => {
  const u = status.value?.user;
  if (!u) return "";
  try {
    return JSON.stringify(u, null, 2);
  } catch {
    return "";
  }
});

/** Format timestamp của cổng (RFC3339) sang chuỗi giờ Việt Nam. */
function fmtDate(ts: string) {
  const d = new Date(ts);
  return Number.isNaN(d.getTime()) ? ts : d.toLocaleString("vi-VN");
}

/** Trạng thái hạn mật khẩu (`password_expire` từ profile cổng). */
const passwordExpire = computed<{ raw: string; expired: boolean; daysLeft: number } | null>(() => {
  const u = status.value?.user as Record<string, unknown> | null;
  const pe = u?.password_expire;
  if (typeof pe !== "string" || !pe) return null;
  const d = new Date(pe);
  if (Number.isNaN(d.getTime())) return { raw: pe, expired: false, daysLeft: 0 };
  const ms = d.getTime() - Date.now();
  return { raw: pe, expired: ms <= 0, daysLeft: Math.ceil(ms / 86_400_000) };
});

/** Các trường thông tin NNT đã đọc ra để hiển thị trực quan (bỏ qua rỗng/null). */
const taxFields = computed(() => {
  const u = status.value?.user;
  if (!u || typeof u !== "object") return [] as { label: string; value: string }[];
  const rec = u as Record<string, unknown>;
  const tin = (rec.tinInfoTT86 ?? {}) as Record<string, unknown>;
  const rows: { label: string; value: string }[] = [];
  const put = (label: string, v: unknown) => {
    if (v === null || v === undefined) return;
    const s = Array.isArray(v) ? v.join(", ") : String(v);
    if (s.trim() === "") return;
    rows.push({ label, value: s });
  };
  put("Tên người nộp thuế", rec.name);
  if (tin.mstUTien !== undefined) put("MST (ưu tiên)", tin.mstUTien);
  else put("MST (ưu tiên)", rec.id ?? rec.username);
  if (tin.mst !== undefined) put("MST", tin.mst);
  put("Nhóm MST", Array.isArray(tin.dsMst) ? tin.dsMst : tin.groupIds);
  put("Định danh CCCD", tin.cccd === true ? "Có" : tin.cccd === false ? "Không" : tin.cccd);
  put(
    "Trạng thái đối ứng",
    tin.doiUng === true ? "Có" : tin.doiUng === false ? "Không" : tin.doiUng,
  );
  const pe = rec.password_expire;
  if (typeof pe === "string" && pe) {
    put("Hạn mật khẩu", fmtDate(pe));
  }
  put("Loại tài khoản", rec.type);
  return rows;
});

function toastError(summary: string, e: unknown) {
  toast.add({ severity: "error", summary, detail: String(e), life: 5000 });
}

async function loadConfig() {
  const c = await api.hddtGetConfig();
  cfg.username = c.username;
  cfg.base_url = c.base_url;
  hasPassword.value = c.has_password;
  configured.value = c.configured;
  autoChange.value = c.auto_change_password;
  cfg.password = "";
  savedPassword.value = "";
  try {
    savedPassword.value = await api.hddtGetPassword();
  } catch {
    savedPassword.value = "";
  }
}

/** Nạp lại mật khẩu đã lưu → nút "Xem mật khẩu đã lưu" luôn phản ánh dữ liệu thật
 *  (sau khi lưu/đăng nhập/tự đổi mật khẩu, giá trị trong CSDL có thể đã thay đổi). */
async function reloadSavedPassword() {
  try {
    savedPassword.value = await api.hddtGetPassword();
  } catch {
    savedPassword.value = "";
  }
}

async function refreshStatus() {
  status.value = await api.hddtStatus();
}

/** Đăng nhập (tự giải captcha). `interactive` = do người dùng bấm → có toast. */
async function autoLogin(interactive = false) {
  if (busy.value || manualStopped.value) return;
  busy.value = true;
  lastError.value = "";
  try {
    const res = await api.hddtLogin();
    if (res.status) status.value = res.status;
    if (!res.ok && res.need_manual) {
      lastError.value = res.reason ?? "";
      await openManual();
    } else {
      // Mật khẩu cổng vừa được tự đổi → cập nhật để nút "Xem mật khẩu đã lưu" hiện đúng.
      await reloadSavedPassword();
      if (res.password_rotated && res.new_password) {
        pwDialogNew.value = res.new_password;
        pwDialogVisible.value = true;
      } else if (interactive) {
        toast.add({ severity: "success", summary: "Đã đăng nhập cổng HĐĐT", life: 2500 });
      }
    }
  } catch (e) {
    lastError.value = String(e);
    if (interactive) toastError("Đăng nhập HĐĐT thất bại", e);
  } finally {
    busy.value = false;
  }
}

/** Lấy captcha mới + mở hộp thoại cho người dùng nhập tay. */
async function openManual() {
  manualStopped.value = true; // dừng tự đăng nhập để không đè captcha đang nhập
  try {
    const c = await api.hddtCaptcha();
    manualKey.value = c.key;
    manualSvg.value = c.svg;
    manualValue.value = "";
    manualError.value = "";
    manualVisible.value = true;
  } catch (e) {
    toastError("Không lấy được captcha", e);
  }
}

async function submitManual() {
  const val = manualValue.value.trim();
  if (!val) {
    manualError.value = "Nhập mã captcha";
    return;
  }
  manualBusy.value = true;
  manualError.value = "";
  try {
    const res = await api.hddtLoginManual({ captchaKey: manualKey.value, captchaValue: val });
    if (res.status) status.value = res.status;
    if (res.ok) {
      manualVisible.value = false;
      manualStopped.value = false;
      lastError.value = "";
      toast.add({ severity: "success", summary: "Đã đăng nhập cổng HĐĐT", life: 2500 });
    }
  } catch (e) {
    manualError.value = String(e);
    await openManual(); // captcha sai → lấy mã mới
  } finally {
    manualBusy.value = false;
  }
}

function onManualHide() {
  // Người dùng đóng hộp thoại mà chưa đăng nhập được → cho phép tự thử lại.
  if (!status.value?.logged_in) manualStopped.value = false;
}

async function save() {
  if (!cfg.username.trim()) {
    toastError("Thiếu thông tin", "Nhập tên đăng nhập HĐĐT");
    return;
  }
  saving.value = true;
  try {
    const c = await api.hddtSaveConfig({
      username: cfg.username.trim(),
      password: cfg.password,
      baseUrl: cfg.base_url.trim(),
    });
    hasPassword.value = c.has_password;
    configured.value = c.configured;
    cfg.password = "";
    manualStopped.value = false;
    toast.add({ severity: "success", summary: "Đã lưu tài khoản HĐĐT", life: 2500 });
    await refreshStatus();
    if (c.configured) await autoLogin(true);
    await reloadSavedPassword(); // cập nhật để nút "Xem mật khẩu đã lưu" hiển thị đúng
  } catch (e) {
    toastError("Không lưu được", e);
  } finally {
    saving.value = false;
  }
}

async function toggleAutoChange() {
  const next = autoChange.value;
  const prev = !next;
  try {
    const c = await api.hddtSetAutoChangePassword(next);
    autoChange.value = c.auto_change_password;
    toast.add({
      severity: "info",
      summary: "Tự đổi mật khẩu " + (next ? "đã bật" : "đã tắt"),
      life: 2500,
    });
  } catch (e) {
    autoChange.value = prev;
    toastError("Không lưu cài đặt", e);
  }
}

/** Đổi mật khẩu cổng ngay (user-initiated). Mật khẩu mới hiện 1 lần rồi lưu vào app. */
async function changePassword() {
  changeBusy.value = true;
  try {
    const res = await api.hddtChangePassword();
    if (res.status) status.value = res.status;
    if (res.ok && res.new_password) {
      pwDialogNew.value = res.new_password;
      pwDialogVisible.value = true;
      changeBusy.value = false;
      await reloadSavedPassword(); // mật khẩu mới đã được lưu vào CSDL → cập nhật nút "Xem mật khẩu đã lưu"
      toast.add({ severity: "success", summary: "Đã đổi mật khẩu & đăng nhập lại", life: 4000 });
    } else if (res.need_manual) {
      lastError.value = res.reason ?? "";
      await openManual();
    } else {
      toast.add({
        severity: "error",
        summary: "Đổi mật khẩu thất bại",
        detail: res.reason ?? "",
        life: 5000,
      });
    }
  } catch (e) {
    toastError("Đổi mật khẩu thất bại", e);
  } finally {
    changeBusy.value = false;
  }
}

/** Bấm "Xem" → đọc thẳng mật khẩu từ CSDL ngay lúc đó (không dùng cache mount). */
async function toggleSavedPassword() {
  if (showSavedPassword.value) {
    showSavedPassword.value = false;
    return;
  }
  await reloadSavedPassword(); // luôn đọc giá trị thật tại thời điểm bấm
  showSavedPassword.value = true;
}

/** Copy mật khẩu mới ra clipboard (nếu bị chặn thì hiện toast). */
async function copyNewPassword() {
  if (!pwDialogNew.value) return;
  try {
    await navigator.clipboard.writeText(pwDialogNew.value);
    toast.add({ severity: "info", summary: "Đã sao chép mật khẩu mới", life: 2000 });
  } catch {
    toast.add({ severity: "warn", summary: "Không truy cập được clipboard", life: 3000 });
  }
}

async function doLogout() {
  try {
    await api.hddtLogout();
    manualStopped.value = false;
    await refreshStatus();
    toast.add({ severity: "info", summary: "Đã đăng xuất cổng HĐĐT", life: 2000 });
  } catch (e) {
    toastError("Không đăng xuất được", e);
  }
}

/** Định kỳ: token sắp/đã hết hạn thì tự đăng nhập lại. */
async function tick() {
  if (manualVisible.value || manualStopped.value || busy.value) return;
  try {
    await refreshStatus();
  } catch {
    return;
  }
  const s = status.value;
  if (!s?.configured) return;
  if (!s.logged_in || s.seconds_left <= RELOGIN_AHEAD) {
    await autoLogin();
  }
}

onMounted(async () => {
  loading.value = true;
  try {
    await loadConfig();
    await tick();
  } catch (e) {
    toastError("Không tải được cấu hình HĐĐT", e);
  } finally {
    loading.value = false;
  }
  timer = setInterval(tick, 60_000);
});

onUnmounted(() => {
  if (timer) clearInterval(timer);
});
</script>

<template>
  <div class="space-y-4">
    <Toolbar>
      <template #start>
        <div class="flex items-center gap-2">
          <i class="pi pi-cloud text-xl text-primary-500" />
          <h3 class="font-semibold">Hóa đơn điện tử (HĐĐT)</h3>
        </div>
      </template>
      <template #end>
        <Tag
          v-if="status?.logged_in"
          severity="success"
          icon="pi pi-check-circle"
          :value="`Đã kết nối (${tokenText})`"
        />
        <Tag
          v-else-if="configured"
          severity="warn"
          icon="pi pi-exclamation-triangle"
          value="Chưa đăng nhập"
        />
        <Tag v-else severity="secondary" icon="pi pi-minus-circle" value="Chưa cấu hình" />
      </template>
    </Toolbar>

    <div v-if="loading" class="flex justify-center py-10">
      <ProgressSpinner />
    </div>

    <template v-else>
      <!-- Tài khoản cổng thuế -->
      <Card>
        <template #title>Tài khoản cổng thuế</template>
        <template #content>
          <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
            <FormField label="Tên đăng nhập (MST)">
              <InputText v-model="cfg.username" class="w-full" placeholder="Nhập mã số thuế" />
            </FormField>
            <FormField label="Mật khẩu">
              <div class="relative w-full">
                <InputText
                  v-model="cfg.password"
                  :type="showPassword ? 'text' : 'password'"
                  class="w-full pr-10"
                  :placeholder="hasPassword ? 'Đã lưu — để trống nếu không đổi' : 'Nhập mật khẩu'"
                />
                <button
                  type="button"
                  class="absolute inset-y-0 right-0 flex w-9 items-center justify-center text-gray-400 hover:text-gray-600"
                  :aria-label="showPassword ? 'Ẩn mật khẩu' : 'Hiện mật khẩu'"
                  :title="showPassword ? 'Ẩn mật khẩu' : 'Hiện mật khẩu'"
                  tabindex="-1"
                  @click="showPassword = !showPassword"
                >
                  <i :class="showPassword ? 'pi pi-eye-slash' : 'pi pi-eye'" />
                </button>
              </div>
            </FormField>
            <!-- Xem mật khẩu đã lưu (local only) -->
            <div v-if="hasPassword" class="mt-2">
              <Button
                size="small"
                :label="showSavedPassword ? 'Ẩn mật khẩu đã lưu' : 'Xem mật khẩu đã lưu'"
                :icon="showSavedPassword ? 'pi pi-eye-slash' : 'pi pi-eye'"
                text
                severity="secondary"
                :loading="busy"
                @click="toggleSavedPassword"
              />
              <InputText
                v-if="showSavedPassword"
                v-model="savedPassword"
                readonly
                class="mt-2 w-full"
                placeholder="Chưa lưu mật khẩu"
              />
            </div>
          </div>

          <div class="mt-4">
            <FormField label="Địa chỉ cổng (nâng cao — để trống dùng cổng chính thức)">
              <InputText
                v-model="cfg.base_url"
                class="w-full"
                placeholder="https://hoadondientu.gdt.gov.vn"
              />
            </FormField>
          </div>

          <div class="mt-4 flex flex-wrap items-center gap-3">
            <Button label="Lưu tài khoản" icon="pi pi-save" :loading="saving" @click="save" />
            <Button
              v-if="configured"
              label="Đăng nhập lại"
              icon="pi pi-sign-in"
              severity="secondary"
              :loading="busy"
              :disabled="busy"
              @click="autoLogin(true)"
            />
            <Button
              v-if="status?.logged_in"
              label="Đăng xuất"
              icon="pi pi-sign-out"
              severity="danger"
              text
              @click="doLogout"
            />
          </div>

          <div class="mt-4 flex items-center gap-3">
            <ToggleSwitch
              v-model="autoChange"
              :disabled="saving || busy"
              :aria-label="'Tự đổi mật khẩu'"
              @change="toggleAutoChange"
            />
            <div>
              <div class="text-sm font-medium">Tự đổi mật khẩu khi sắp hết hạn</div>
              <div class="text-xs text-gray-500">
                Mật khẩu hết hạn ≤ 7 ngày sẽ tự sinh mật khẩu mới (8–15 ký tự, đủ chữ
                hoa/thường/số/ký tự đặc biệt) và đăng nhập lại. Mặc định tắt vì khôi phục mật khẩu
                bị mất rất khó.
              </div>
            </div>
          </div>

          <div v-if="status?.logged_in" class="mt-3">
            <Button
              label="Đổi mật khẩu ngay"
              icon="pi pi-key"
              severity="secondary"
              :loading="changeBusy"
              :disabled="changeBusy"
              @click="changePassword"
            />
          </div>

          <p class="mt-3 text-xs text-gray-500">
            <i class="pi pi-lock" />
            Tài khoản được lưu trong CSDL cục bộ của hồ sơ HKD (không mã hoá) để tự động đăng nhập
            lại khi token hết hạn (~1 ngày).
          </p>
        </template>
      </Card>

      <!-- Trạng thái kết nối -->
      <Card>
        <template #title>Trạng thái kết nối</template>
        <template #content>
          <div v-if="!configured" class="text-sm text-gray-500">
            Chưa cấu hình tài khoản HĐĐT. Nhập tên đăng nhập và mật khẩu ở trên rồi bấm "Lưu tài
            khoản".
          </div>

          <div v-else class="space-y-3">
            <div
              v-if="passwordExpire?.expired"
              class="rounded border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700"
            >
              <i class="pi pi-exclamation-triangle mr-1" />
              <b>Mật khẩu cổng đã hết hạn</b> ({{ fmtDate(passwordExpire.raw) }}). Cổng sẽ từ chối
              đăng nhập đến khi đổi mật khẩu. Đổi mật khẩu trên cổng HĐĐT, nhập mật khẩu mới ở ô bên
              trên rồi bấm "Lưu tài khoản".
            </div>
            <div
              v-else-if="passwordExpire && passwordExpire.daysLeft <= 30"
              class="rounded border border-amber-200 bg-amber-50 px-3 py-2 text-sm text-amber-700"
            >
              <i class="pi pi-clock mr-1" />
              Mật khẩu cổng sắp hết hạn — còn <b>{{ passwordExpire.daysLeft }} ngày</b> (đến
              {{ fmtDate(passwordExpire.raw) }}). Nhớ đổi mật khẩu trên cổng để không bị gián đoạn.
            </div>
            <div
              v-if="configured && !status?.logged_in && lastError"
              class="rounded border border-amber-200 bg-amber-50 px-3 py-2 text-sm text-amber-700"
            >
              <i class="pi pi-info-circle mr-1" />
              Chưa đăng nhập được cổng HĐĐT. Nếu bị từ chối do hết hạn mật khẩu: đổi mật khẩu trên
              cổng, nhập mật khẩu mới ở ô bên trên rồi bấm "Lưu tài khoản".
            </div>

            <div class="grid grid-cols-1 gap-4 text-sm md:grid-cols-4">
              <div>
                <div class="text-gray-500">Tài khoản</div>
                <div class="font-medium">{{ status?.username || cfg.username }}</div>
              </div>
              <div>
                <div class="text-gray-500">Kết nối</div>
                <div class="font-medium">
                  <span v-if="status?.logged_in" class="text-green-600">Đã đăng nhập</span>
                  <span v-else class="text-amber-600">Chưa đăng nhập</span>
                </div>
              </div>
              <div>
                <div class="text-gray-500">Token còn lại</div>
                <div class="font-medium">{{ tokenText }}</div>
              </div>
              <div>
                <div class="text-gray-500">Hạn mật khẩu</div>
                <div class="font-medium">
                  <span v-if="passwordExpire" :class="passwordExpire.expired ? 'text-red-600' : ''">
                    {{ fmtDate(passwordExpire.raw) }}
                    <span v-if="passwordExpire.expired" class="text-xs">(đã hết hạn)</span>
                    <span v-else-if="passwordExpire.daysLeft <= 30" class="text-xs text-amber-600">
                      (còn {{ passwordExpire.daysLeft }} ngày)
                    </span>
                  </span>
                  <span v-else>—</span>
                </div>
              </div>
            </div>

            <div v-if="lastError" class="text-sm text-red-600">
              <i class="pi pi-exclamation-circle" /> {{ lastError }}
            </div>

            <div v-if="taxFields.length" class="space-y-3">
              <div class="text-sm font-medium text-gray-600">
                <i class="pi pi-id-card mr-1" />Thông tin người nộp thuế (từ cổng)
              </div>
              <div class="overflow-hidden rounded border border-gray-200">
                <div
                  v-for="(f, i) in taxFields"
                  :key="i"
                  class="flex border-b border-gray-100 last:border-b-0"
                >
                  <div class="w-44 shrink-0 bg-gray-50 px-3 py-2 text-sm font-medium text-gray-600">
                    {{ f.label }}
                  </div>
                  <div class="flex-1 break-words px-3 py-2 text-sm">{{ f.value }}</div>
                </div>
              </div>
              <details class="group">
                <summary
                  class="cursor-pointer select-none text-sm text-gray-500 hover:text-gray-700"
                >
                  <i class="pi pi-code mr-1" />Dữ liệu thô từ cổng
                </summary>
                <pre
                  class="mt-2 max-h-64 overflow-auto rounded border border-gray-200 bg-gray-50 p-3 text-xs"
                  >{{ userJson }}</pre>
              </details>
            </div>
          </div>
        </template>
      </Card>
    </template>

    <!-- Nhập captcha thủ công -->
    <Dialog
      v-model:visible="manualVisible"
      modal
      header="Nhập captcha"
      :style="{ width: '420px' }"
      @hide="onManualHide"
    >
      <div class="space-y-3">
        <p class="text-sm text-gray-600">
          Không giải được captcha tự động. Nhập 6 ký tự trong ảnh dưới đây:
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
        <Button label="Hủy" severity="secondary" text @click="manualVisible = false" />
        <Button label="Xác nhận" icon="pi pi-check" :loading="manualBusy" @click="submitManual" />
      </template>
    </Dialog>

    <!-- Mật khẩu mới (sau khi đổi) -->
    <Dialog
      v-model:visible="pwDialogVisible"
      modal
      header="Mật khẩu mới"
      :style="{ width: '400px' }"
      :closable="false"
    >
      <div class="space-y-3">
        <p class="text-sm text-gray-600">
          Đã tự đổi mật khẩu cổng HĐĐT thành công. Mật khẩu mới được lưu trong app; nếu bạn cần đăng
          nhập trực tiếp trên cổng web, hãy giữ lại mật khẩu dưới đây:
        </p>
        <div class="flex gap-2">
          <InputText :value="pwDialogNew" readonly class="flex-1 font-mono" />
          <Button icon="pi pi-copy" severity="secondary" @click="copyNewPassword" />
        </div>
      </div>
      <template #footer>
        <Button label="Đã lưu" @click="pwDialogVisible = false" />
      </template>
    </Dialog>
  </div>
</template>
