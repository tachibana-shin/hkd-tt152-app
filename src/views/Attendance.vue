<script setup lang="ts">
import { api } from "@/db";
import type { AttendanceStatus, Employee } from "@/types";
import { useAuthStore } from "@/stores/auth";

const toast = useToast();
const router = useRouter();
const auth = useAuthStore();

// ─── Trạng thái chấm công (khớp ký hiệu sheet "Cham Cong" bộ mẫu TT152) ───
// Chu kỳ khi click: X → NC → P → H → CT → TS → O → CO → KL → (trống) → X...
const STATUS_CYCLE: AttendanceStatus[] = [
  "X", "NC", "P", "H", "CT", "TS", "O", "CO", "KL", "",
];
/** Các trạng thái tính 1 công (NC tính 0.5, còn lại 0) */
const COUNT_ONE = new Set<AttendanceStatus>(["X", "P", "H", "CT", "TS", "O", "CO"]);

const LABELS: Record<AttendanceStatus, string> = {
  X: "X", NC: "½", P: "P", H: "H", CT: "CT",
  TS: "TS", O: "O", CO: "CO", KL: "KL", "": "",
};

const STYLE: Record<AttendanceStatus, { chip: string; cell: string; text: string; desc: string }> = {
  X:  { chip: "bg-emerald-500", cell: "bg-emerald-100", text: "text-emerald-700 font-bold", desc: "công đủ" },
  NC: { chip: "bg-amber-400",   cell: "bg-amber-100",  text: "text-amber-700",             desc: "nửa công" },
  P:  { chip: "bg-sky-500",     cell: "bg-sky-100",    text: "text-sky-700",               desc: "phép" },
  H:  { chip: "bg-cyan-500",    cell: "bg-cyan-100",   text: "text-cyan-700",              desc: "học tập" },
  CT: { chip: "bg-violet-500",  cell: "bg-violet-100", text: "text-violet-700",            desc: "công tác" },
  TS: { chip: "bg-pink-500",    cell: "bg-pink-100",   text: "text-pink-700",              desc: "thai sản" },
  O:  { chip: "bg-orange-500",  cell: "bg-orange-100", text: "text-orange-700",            desc: "ốm" },
  CO: { chip: "bg-rose-500",    cell: "bg-rose-100",   text: "text-rose-700",              desc: "con ốm" },
  KL: { chip: "bg-slate-400",   cell: "bg-slate-200",  text: "text-slate-500",             desc: "không lương" },
  "": { chip: "bg-gray-200",    cell: "bg-white",      text: "text-gray-300",              desc: "nghỉ / chưa chấm" },
};

const DOW = ["CN", "T2", "T3", "T4", "T5", "T6", "T7"];

// ─── Kỳ chấm công ───
const now = new Date();
const curPeriod = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}`;
const period = ref(curPeriod);

/** Danh sách kỳ gợi ý: 24 tháng trước → 12 tháng sau (vẫn cho nhập tay) */
function buildPeriodOptions(): string[] {
  const opts: string[] = [];
  const d = new Date(now.getFullYear(), now.getMonth() - 24, 1);
  const end = new Date(now.getFullYear(), now.getMonth() + 12, 1);
  while (d <= end) {
    opts.push(`${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}`);
    d.setMonth(d.getMonth() + 1);
  }
  return opts;
}
const periodOptions = buildPeriodOptions();

// ─── Lưới ngày ───
interface DayInfo {
  day: number;
  date: string; // YYYY-MM-DD
  dow: number; // 0 = Chủ nhật
  isSunday: boolean;
}
const days = computed<DayInfo[]>(() => {
  const m = /^(\d{4})-(\d{2})$/.exec(period.value);
  if (!m) return [];
  const year = Number(m[1]);
  const month = Number(m[2]) - 1;
  const total = new Date(year, month + 1, 0).getDate();
  const out: DayInfo[] = [];
  for (let d = 1; d <= total; d++) {
    const dow = new Date(year, month, d).getDay();
    out.push({
      day: d,
      date: `${m[1]}-${m[2]}-${String(d).padStart(2, "0")}`,
      dow,
      isSunday: dow === 0,
    });
  }
  return out;
});

// ─── Dữ liệu ───
interface GridRow {
  code: string;
  name: string;
  department: string;
  cells: AttendanceStatus[]; // cells[day-1] = trạng thái
}
const employees = ref<Employee[]>([]);
const rows = ref<GridRow[]>([]);
const loading = ref(false);
const saving = ref(false);

function isStatus(s: string): s is AttendanceStatus {
  return (STATUS_CYCLE as readonly string[]).includes(s);
}

/** Xây lại lưới từ dữ liệu chấm công hiện tại của kỳ (lọc nhân viên đang làm việc) */
async function refill(): Promise<void> {
  if (!days.value.length) {
    rows.value = [];
    return;
  }
  const att = await api.getAttendance(period.value);
  const map = new Map<string, AttendanceStatus>();
  for (const a of att) {
    if (isStatus(a.status)) map.set(`${a.employee_code}::${a.work_date}`, a.status);
  }
  rows.value = employees.value
    .filter((e) => !e.left_on)
    .map((e) => ({
      code: e.code,
      name: e.name,
      department: e.department,
      cells: days.value.map((d) => map.get(`${e.code}::${d.date}`) ?? ""),
    }));
}

async function reload() {
  loading.value = true;
  try {
    employees.value = await api.getEmployees();
    await refill();
  } catch (e) {
    toast.add({ severity: "error", summary: "Không tải được dữ liệu", detail: String(e) });
  } finally {
    loading.value = false;
  }
}

async function loadAttendance() {
  if (!/^\d{4}-\d{2}$/.test(period.value)) return;
  loading.value = true;
  try {
    await refill();
  } catch (e) {
    toast.add({ severity: "error", summary: "Không tải được chấm công", detail: String(e) });
  } finally {
    loading.value = false;
  }
}

watch(period, () => {
  void loadAttendance();
});

// ─── Tương tác ô ───
function cycle(row: GridRow, idx: number) {
  if (!auth.canAccounting) return;
  const cur = row.cells[idx];
  const next = STATUS_CYCLE[(STATUS_CYCLE.indexOf(cur) + 1) % STATUS_CYCLE.length];
  row.cells[idx] = next;
}

function fillMonth(row: GridRow) {
  if (!auth.canAccounting) return;
  days.value.forEach((d, idx) => {
    row.cells[idx] = d.isSunday ? "" : "X";
  });
}

function clearRow(row: GridRow) {
  if (!auth.canAccounting) return;
  for (let i = 0; i < row.cells.length; i++) row.cells[i] = "";
}

// ─── Tổng hợp ───
function rowTotals(row: GridRow): { work: number; unpaid: number } {
  let work = 0;
  let unpaid = 0;
  for (const s of row.cells) {
    if (COUNT_ONE.has(s)) work += 1;
    else if (s === "NC") work += 0.5;
    else if (s === "KL") unpaid += 1;
  }
  return { work: Math.round(work * 10) / 10, unpaid };
}
const totals = computed(() => new Map(rows.value.map((r) => [r.code, rowTotals(r)])));

// ─── Hiển thị ô ───
function cellBg(row: GridRow, d: DayInfo, idx: number): string {
  const s = row.cells[idx];
  if (s) return STYLE[s].cell;
  return d.isSunday ? "bg-slate-100/80" : "bg-white";
}
function cellText(row: GridRow, idx: number): string {
  const s = row.cells[idx];
  return s ? STYLE[s].text : "text-gray-300";
}
function cellTitle(row: GridRow, idx: number, d: DayInfo): string {
  return `Ngày ${d.date} · ${STYLE[row.cells[idx]].desc}`;
}

// ─── Lưu ───
async function save() {
  if (!auth.canAccounting) return;
  if (!/^\d{4}-\d{2}$/.test(period.value)) {
    toast.add({ severity: "warn", summary: "Kỳ không hợp lệ", detail: "Kỳ chấm công phải có dạng YYYY-MM" });
    return;
  }
  if (!rows.value.length) {
    toast.add({ severity: "warn", summary: "Chưa có nhân viên", detail: "Không có nhân viên đang làm việc để chấm công" });
    return;
  }
  saving.value = true;
  try {
    // Gửi TOÀN BỘ lưới — mọi ô, kể cả ô trống (status "")
    const entries: { employee_code: string; work_date: string; status: string }[] = [];
    for (const r of rows.value) {
      days.value.forEach((d, idx) => {
        entries.push({ employee_code: r.code, work_date: d.date, status: r.cells[idx] });
      });
    }
    const res = await api.saveAttendance(period.value, entries);

    let detail = res;
    try {
      const j = JSON.parse(res) as { ok?: boolean; saved?: number };
      const saved = Number(j.saved) || 0;
      if (j.ok !== undefined) detail = `Đã lưu ${saved} ô cho kỳ ${period.value}`;
    } catch {
      /* res không phải JSON — dùng nguyên văn làm detail */
    }
    toast.add({ severity: "success", summary: "Đã lưu chấm công", detail });
    await refill();
  } catch (e) {
    toast.add({ severity: "error", summary: "Không lưu được chấm công", detail: String(e) });
  } finally {
    saving.value = false;
  }
}

onMounted(() => {
  void reload();
});
</script>

<template>
  <div class="space-y-4">
    <Toolbar>
      <template #start>
        <div class="flex items-center gap-2">
          <i-mdi-calendar-check class="text-xl text-primary-500" />
          <h3 class="font-semibold">Chấm công</h3>
        </div>
      </template>
    </Toolbar>

    <SectionCard>
      <template #icon><i-mdi-calendar-check class="text-sky-500" /></template>
      <template #title>
        <span>Bảng chấm công</span>
        <span class="text-xs text-gray-400">({{ rows.length }} NV)</span>
      </template>
        <div class="flex flex-wrap items-end gap-3">
          <div>
            <label class="mb-1 block text-xs text-gray-500">Kỳ chấm công (YYYY-MM)</label>
            <Select v-model="period" :options="periodOptions" editable placeholder="YYYY-MM" class="w-44" />
          </div>
          <Button label="Tải lại" icon="pi pi-refresh" severity="secondary" :loading="loading" @click="reload" />
          <Button
            v-if="auth.canAccounting"
            label="Lưu chấm công"
            icon="pi pi-save"
            severity="success"
            :loading="saving"
            :disabled="!rows.length || loading"
            @click="save"
          />
          <Tag
            v-if="!auth.canAccounting"
            value="Chỉ đọc"
            severity="secondary"
            icon="pi pi-eye"
          />
        </div>

        <!-- Chú giải màu -->
        <div class="mt-3 flex flex-wrap items-center gap-x-4 gap-y-1.5 text-xs">
          <span class="text-gray-500">Ký hiệu:</span>
          <span v-for="st in STATUS_CYCLE" :key="st" class="inline-flex items-center gap-1">
            <span class="inline-block h-3.5 w-3.5 rounded border border-black/10" :class="STYLE[st].chip" />
            <span :title="STYLE[st].desc">{{ st ? LABELS[st] : "Trống" }}</span>
          </span>
        </div>

        <!-- Lưới chấm công -->
        <div class="mt-3 max-h-[540px] overflow-auto rounded-lg border">
          <table class="w-full border-collapse text-sm">
            <thead class="sticky top-0 z-10">
              <tr>
                <th class="whitespace-nowrap border-b bg-gray-50 px-2 py-1.5 text-left text-xs font-semibold text-gray-600">
                  Mã NV
                </th>
                <th class="whitespace-nowrap border-b bg-gray-50 px-2 py-1.5 text-left text-xs font-semibold text-gray-600">
                  Họ tên
                </th>
                <th class="whitespace-nowrap border-b bg-gray-50 px-2 py-1.5 text-left text-xs font-semibold text-gray-600">
                  Bộ phận
                </th>
                <th
                  v-for="d in days"
                  :key="d.day"
                  class="border-b px-0.5 py-1 text-center text-xs font-semibold"
                  :class="d.isSunday ? 'bg-rose-50 text-rose-600' : 'bg-gray-50 text-gray-600'"
                >
                  <div>{{ d.day }}</div>
                  <div class="text-[10px] font-normal">{{ DOW[d.dow] }}</div>
                </th>
                <th class="whitespace-nowrap border-b bg-gray-50 px-2 py-1.5 text-center text-xs font-semibold text-emerald-700">
                  Công
                </th>
                <th class="whitespace-nowrap border-b bg-gray-50 px-2 py-1.5 text-center text-xs font-semibold text-gray-600">
                  Nghỉ
                </th>
                <th class="whitespace-nowrap border-b bg-gray-50 px-2 py-1.5 text-center text-xs font-semibold text-gray-600">
                  Tiện ích
                </th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="r in rows" :key="r.code" class="border-b last:border-b-0">
                <td class="whitespace-nowrap px-2 py-1 font-medium">{{ r.code }}</td>
                <td class="whitespace-nowrap px-2 py-1">{{ r.name }}</td>
                <td class="whitespace-nowrap px-2 py-1 text-gray-500">{{ r.department || "—" }}</td>
                <td
                  v-for="(d, idx) in days"
                  :key="d.day"
                  class="p-0 text-center"
                  :class="cellBg(r, d, idx)"
                >
                  <button
                    type="button"
                    class="block h-7 w-full text-xs leading-7 transition-colors hover:bg-black/5"
                    :class="cellText(r, idx)"
                    :title="cellTitle(r, idx, d)"
                    @click="cycle(r, idx)"
                  >
                    {{ LABELS[r.cells[idx]] }}
                  </button>
                </td>
                <td class="whitespace-nowrap px-2 py-1 text-center font-bold text-emerald-700">
                  {{ totals.get(r.code)?.work.toFixed(1) }}
                </td>
                <td class="whitespace-nowrap px-2 py-1 text-center text-slate-500">
                  {{ totals.get(r.code)?.unpaid }}
                </td>
                <td class="whitespace-nowrap px-2 py-1">
                  <div class="flex justify-center gap-1">
                    <Button v-if="auth.canAccounting" label="Cả tháng" icon="pi pi-calendar" size="small" severity="secondary" text :disabled="loading || saving" v-tooltip.top="'Đặt X vào các ngày (trừ Chủ nhật)'" @click="fillMonth(r)" />
                    <Button v-if="auth.canAccounting" label="Xóa" icon="pi pi-trash" size="small" severity="danger" text :disabled="loading || saving" v-tooltip.top="'Xóa toàn bộ chấm công hàng này'" @click="clearRow(r)" />
                  </div>
                </td>
              </tr>
              <tr v-if="!rows.length">
                <td :colspan="days.length + 6" class="px-3 py-8 text-center text-gray-400">
                  Chưa có nhân viên — thêm ở màn Bảng lương.
                </td>
              </tr>
            </tbody>
          </table>
        </div>

        <p class="mt-3 flex flex-wrap items-center gap-3 text-xs text-gray-400">
          <span>Công = X/P/H/CT/TS/O/CO (1) + NC (0.5).</span>
          <Button label="Mở Bảng lương" icon="pi pi-arrow-right" size="small" text @click="router.push('/payroll')" />
        </p>
    </SectionCard>
  </div>
</template>