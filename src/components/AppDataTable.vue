<script lang="ts">
/**
 * AppDataTable — bọc DataTable của PrimeVue với các khả năng dùng chung:
 *
 *  - kéo-resize cột (kèm patch cho WebKitGTK), cột thao tác cố định bên phải.
 *  - selection bằng click (prop `metaKeySelection`, mặc định true = cần Ctrl để
 *    chọn thêm; màn muốn click là toggle luôn thì truyền :meta-key-selection="false").
 *  - sửa ô trực tiếp bằng DOUBLE-TAP (prop `editable`): mọi <Column> có
 *    `:editable="..."` + slot #editor (đúng quy ước của PrimeVue) sẽ được tự bọc
 *    × AppDataTable; click đơn vẫn là chọn/toggle dòng, double-tap mở editor.
 *    Enter = lưu, Esc = hủy, click ra ngoài = lưu; double-tap KHÔNG làm thay đổi
 *    lựa chọn. Khi commit phát sự kiện `cell-save` với { field, data } — data là
 *    bản sao đã sửa; màn hình tự lưu DB + reload trang.
 *  - nút bật/tắt hàng filter theo cột (prop `filterToggle`, mặc định hàng filter
 *    ẩn). Khi bật, đừng truyền `:filter-display` từ màn hình — AppDataTable tự quản.
 *
 * Chế độ dữ liệu tự phát hiện: màn truyền @page/@sort/@filter (như Products) là
 * server-side → lazy + paginator; bảng client-side (`:value=`) tự lọc/phân trang
 * client-side (có `#filter` trên cột thì bật `v-model:filters` như bình thường).
 * `selectionMode="multiple"` chỉ bật khi màn có bind selection (v-model:selection).
 */
import {
  cloneVNode,
  defineComponent,
  h,
  nextTick,
  onBeforeUnmount,
  ref,
  watch,
} from "vue";
import Button from "primevue/button";
import Column from "primevue/column";
import DataTable from "primevue/datatable";

// ---------------------------------------------------------------------------
// Engine resize: làm cho kéo cột hoạt động đúng trong WebKitGTK
// ---------------------------------------------------------------------------
// PrimeVue ghi cả `width: Xpx !important` lẫn `max-width: Xpx !important` vào
// một <style> trong head ⇒ cột bị khoá cứng (max-width clip cố định). Tệ hơn:
// WebKitGTK bỏ qua width-hint trên <th>/<td> trong table-layout:auto, nên chỉ
// có max-width mới có tác dụng (clip cứng). Vá:
//   1) Xoá `max-width` sau mỗi lần ghi style → cột có thể nở theo nội dung.
//   2) Bật `table-layout: fixed` trên <table> ngay khi ghi width (kéo hoặc
//      khôi phục width đã lưu) → các width bind đúng; bảng chưa resize giữ auto.
let resizePatched = false;
function patchDataTableResize() {
  if (resizePatched) return;
  resizePatched = true;
  const stripResizeMaxWidth = (self: unknown) => {
    const styleEl = (self as { styleElement?: HTMLElement | null }).styleElement;
    if (!styleEl?.textContent) return;
    styleEl.textContent = styleEl.textContent.replace(/\s*max-width\s*:\s*[^;}]+;?/gi, "");
  };
  const afterResizeWidthWrite = (self: unknown) => {
    stripResizeMaxWidth(self);
    const table = (self as { $refs?: { table?: HTMLElement | null } }).$refs?.table;
    if (table) table.style.tableLayout = "fixed";
  };
  const dtMethods = (DataTable as unknown as { methods?: Record<string, unknown> }).methods;
  if (!dtMethods) return;
  const origResize = dtMethods.resizeTableCells as ((w: number, n?: number) => void) | undefined;
  if (origResize) {
    dtMethods.resizeTableCells = function (this: unknown, w: number, n?: number) {
      origResize.call(this, w, n);
      afterResizeWidthWrite(this);
    };
  }
  const origAddStyles = dtMethods.addColumnWidthStyles as ((ws: string[]) => void) | undefined;
  if (origAddStyles) {
    dtMethods.addColumnWidthStyles = function (this: unknown, ws: string[]) {
      origAddStyles.call(this, ws);
      afterResizeWidthWrite(this);
    };
  }
}
patchDataTableResize();

/** pt dùng chung cho cột "cố định bên phải" (không cuộn khi bảng tràn ngang).
 *  Cũng được tự áp dụng cho slot #actions; export ra để dùng với <Column> thường. */
export const fixedRightPt: Record<string, { class: string }> = {
  headerCell: { class: "p-col-fixed-right" },
  bodyCell: { class: "p-col-fixed-right" },
};

export default defineComponent({
  name: "AppDataTable",
  inheritAttrs: false,
  props: {
    /** Bật kéo-resize cột (mặc định bật — mọi bảng dữ liệu đều muốn). */
    resizableColumns: { type: Boolean, default: true },
    /** Tiêu đề cột thao tác cố định bên phải (mặc định rỗng). */
    actionsHeader: { type: String, default: "" },
    /** Bề rộng (px) cột thao tác cố định bên phải. Nhận cả number lẫn string. */
    actionsWidth: { type: [String, Number], default: 140 },
    /**
     * Chọn nhiều dòng bằng click: `false` = click là chọn/toggle luôn (như example
     * Row Selection của PrimeVue); `true` = cần giữ Ctrl/Cmd khi click để chọn thêm.
     */
    metaKeySelection: { type: Boolean, default: true },
    /**
     * Tên cột là khóa của dòng (mặc định 'id') — dùng để so khớp ô đang sửa
     * và để AppDataTable giữ nguyên lựa chọn qua các thao tác.
     */
    dataKey: { type: String, default: undefined },
    /**
     * Bật sửa ô trực tiếp bằng double-tap (mặc định BẬT cho mọi bảng dùng
     * AppDataTable). Các <Column> phải khai `:editable` + slot #editor như quy
     * ước PrimeVue; AppDataTable tự quản lý trạng thái sửa (draft, focus,
     * click-away) và phát `cell-save` khi commit. Cột không khai `:editable`
     * thì giữ nguyên như bình thường.
     */
    editable: { type: Boolean, default: true },
    /**
     * Hiện nút "Bộ lọc" bật/tắt hàng filter theo cột (mặc định BẬT; hàng filter
     * mặc định ẩn). Nút được chèn vào đầu vùng header của bảng; cột nào có
     * `#filter` (hoặc prop `filter`) thì hiện ô lọc tương ứng.
     */
    filterToggle: { type: Boolean, default: true },
    /** Model của selection (v-model:selection). */
    selection: { type: null, default: undefined },
  },
  emits: ["update:selection", "cell-save"],
  setup(props, { slots, attrs, emit }) {
    const hasActionsSlot = !!slots.actions;
    const forwardedSlots = Object.keys(slots).filter(
      (n) => n !== "default" && n !== "actions" && n !== "header",
    );

    // ─── Hàng filter theo cột (prop filterToggle; mặc định ẩn) ───
    const filterRowVisible = ref(false);
    const toggleFilterRow = () => (filterRowVisible.value = !filterRowVisible.value);

    // ─── CELL EDITING ENGINE (double-tap; draft; Enter/Esc/click-away) ───
    interface EditingCell {
      rowKey: string | number;
      field: string;
      draft: Record<string, unknown>;
    }
    const editingCell = ref<EditingCell | null>(null);

    const isCellEditing = (rowKey: string | number, field: string) =>
      editingCell.value?.rowKey === rowKey && editingCell.value.field === field;

    // Cửa sổ double-click chuột thật của OS thường ~500ms; 350ms dung hòa
    // giữa "không ngộ nhận 2 lần click riêng" và "double-click chuột vẫn bắt được".
    const DOUBLE_TAP_MS = 350;
    let lastTap: { rowKey: string | number; field: string; t: number } = {
      rowKey: "",
      field: "",
      t: 0,
    };
    let selectionSnapshot: unknown = null;
    let tapResetTimer: ReturnType<typeof setTimeout> | null = null;
    let outsideCommitHandler: ((e: MouseEvent) => void) | null = null;

    function rowKeyOf(data: unknown) {
      if (!data || typeof data !== "object") return "";
      const keyField = props.dataKey ?? "id";
      const v = (data as Record<string, unknown>)[keyField] ?? (data as Record<string, unknown>).id;
      return (v as string | number) ?? JSON.stringify(data);
    }

    /** Bản sao nông đủ để khôi phục selection (giữ nguyên tham chiếu dòng). */
    function cloneSelection(v: unknown): unknown {
      if (Array.isArray(v)) return [...v];
      if (v && typeof v === "object") return { ...(v as object) };
      return v;
    }

    function handleCellClick(rowData: unknown, field: string, e: MouseEvent) {
      const rowKey = rowKeyOf(rowData);
      // Click trong chính ô đang sửa → chỉ thao tác editor, không toggle chọn.
      if (isCellEditing(rowKey, field)) {
        e.stopPropagation();
        return;
      }
      const now = Date.now();
      const isDoubleTap =
        lastTap.rowKey === rowKey &&
        lastTap.field === field &&
        now - lastTap.t <= DOUBLE_TAP_MS;
      lastTap = { rowKey, field, t: now };
      if (tapResetTimer) clearTimeout(tapResetTimer);

      if (isDoubleTap) {
        // Cú click thứ 2: chặn khỏi <tr> để không toggle chọn; khôi phục trạng
        // thái chọn trước double-tap (chụp ở click 1); rồi mở editor.
        e.stopPropagation();
        if (selectionSnapshot !== null) emit("update:selection", selectionSnapshot);
        selectionSnapshot = null;
        lastTap = { rowKey: "", field: "", t: 0 };
        startCellEdit(rowData, field);
        return;
      }

      // Click đơn: chụp trạng thái chọn để double-tap sau khôi phục.
      selectionSnapshot = cloneSelection(props.selection);
      tapResetTimer = setTimeout(() => {
        selectionSnapshot = null;
        lastTap = { rowKey: "", field: "", t: 0 };
      }, DOUBLE_TAP_MS + 50);
    }

    function startCellEdit(rowData: unknown, field: string) {
      if (!props.editable) return;
      if (editingCell.value) commitCellEdit(); // đang sửa ô khác → lưu trước
      editingCell.value = { rowKey: rowKeyOf(rowData), field, draft: { ...(rowData as object) } };
      nextTick(attachOutsideCommit);
    }

    // Mousedown ra ngoài ô đang sửa = lưu (giống click-away của PrimeVue).
    function attachOutsideCommit() {
      detachOutsideCommit();
      outsideCommitHandler = (e: MouseEvent) => {
        const activeCell = document.querySelector('[data-editing-cell="true"]');
        if (activeCell && !activeCell.contains(e.target as Node)) commitCellEdit();
      };
      document.addEventListener("mousedown", outsideCommitHandler);
    }
    function detachOutsideCommit() {
      if (outsideCommitHandler) {
        document.removeEventListener("mousedown", outsideCommitHandler);
        outsideCommitHandler = null;
      }
    }

    function cancelCellEdit() {
      detachOutsideCommit();
      editingCell.value = null; // draft bỏ đi — dòng trong bảng giữ giá trị cũ
    }

    function commitCellEdit() {
      const cell = editingCell.value;
      if (!cell) return;
      detachOutsideCommit();
      editingCell.value = null;
      emit("cell-save", { field: cell.field, data: { ...cell.draft } });
    }

    onBeforeUnmount(() => {
      detachOutsideCommit();
      if (tapResetTimer) clearTimeout(tapResetTimer);
    });

    // ─── Khung ô editable: view (body gốc) hoặc editor (#editor + draft) ───
    const CellEditableFrame = defineComponent({
      // Ở đây các hàm engine truyền qua props cho gọn (không cần provide/inject).
      props: {
        data: { type: null, required: true },
        rowKey: { type: null, required: true },
        field: { type: String, required: true },
        body: { type: Function, default: null },
        editor: { type: Function, required: true },
        slotProps: { type: Object, default: null },
        isEditing: { type: Function, required: true },
        currentDraft: { type: Function, required: true },
        onCellClick: { type: Function, required: true },
        commit: { type: Function, required: true },
        cancel: { type: Function, required: true },
      },
      setup(p) {
        const root = ref<HTMLElement | null>(null);
        watch(
          () => p.isEditing(p.rowKey, p.field),
          (on) => {
            if (on) {
              nextTick(() => root.value?.querySelector<HTMLInputElement>("input")?.focus());
            }
          },
        );
        return () => {
          const editing = p.isEditing(p.rowKey, p.field);
          const child = editing
            ? p.editor({ ...p.slotProps, data: p.currentDraft() })
            : p.body
              ? p.body(p.slotProps)
              : [String((p.data as Record<string, unknown>)?.[p.field] ?? "")];
          return h(
            "div",
            {
              ref: root,
              class: ["w-full", "cursor-pointer"],
              ...(editing ? { "data-editing-cell": "true" } : {}),
              onClick: (e: MouseEvent) => p.onCellClick(p.data, p.field, e),
              onKeydown: (e: KeyboardEvent) => {
                if (e.key === "Enter") p.commit();
                else if (e.key === "Escape") p.cancel();
              },
            },
            child,
          );
        };
      },
    });

    // ─── Bọc các <Column> editable của màn hình (thay #body bằng khung trên) ───
    function transformColumns() {
      const init = slots.default ? slots.default() : [];
      if (!props.editable) return init;
      return init.map((vnode: any) => {
        if (!vnode || vnode.type !== Column) return vnode;
        const vp = vnode.props || {};
        const field = vp.field || vp.columnKey;
        const children: any = vnode.children;
        if (!field || !vp.editable || !children || !children.editor) return vnode;
        return cloneVNode(vnode, vp, {
          ...children,
          body: (slotProps: any) =>
            h(CellEditableFrame, {
              data: slotProps.data,
              rowKey: rowKeyOf(slotProps.data),
              field,
              body: children.body || null,
              editor: children.editor,
              slotProps,
              isEditing: isCellEditing,
              currentDraft: () => editingCell.value?.draft,
              onCellClick: handleCellClick,
              commit: commitCellEdit,
              cancel: cancelCellEdit,
            }),
        });
      });
    }

    // ─── Header: nút bộ lọc + header của màn hình (khi filterToggle) ───
    function headerContent() {
      const screenHeader = slots.header ? slots.header() : [];
      if (!props.filterToggle) return screenHeader;
      const children: any[] = [
        h(Button, {
          type: "button",
          size: "small",
          label: filterRowVisible.value ? "Ẩn bộ lọc" : "Bộ lọc",
          icon: filterRowVisible.value ? "pi pi-filter-slash" : "pi pi-filter",
          text: !filterRowVisible.value,
          severity: filterRowVisible.value ? "primary" : "secondary",
          onClick: toggleFilterRow,
        }),
      ];
      if (screenHeader.length) {
        children.push(h("div", { class: "flex-1 min-w-[240px]" }, screenHeader));
      }
      return h("div", { class: "flex flex-wrap items-center gap-2" }, children);
    }

    return () =>
      h(
        DataTable,
        {
          ...attrs,
          selection: props.selection,
          "onUpdate:selection": (v: unknown) => emit("update:selection", v),
          resizableColumns: props.resizableColumns,
          size: "large",
          showGridlines: true,
          stripedRows: true,
          // Chế độ dữ liệu tự phát hiện: màn có @page/@sort/@filter (như Products)
          // là server-side → lazy + paginator; bảng client-side (`:value=`) thì
          // để DataTable tự lọc/phân trang client-side. selectionMode chỉ bật khi
          // màn có bind selection (v-model:selection).
          lazy: !!attrs.onPage,
          paginator: !!attrs.onPage || !!attrs.paginator,
          selectionMode: props.selection != null ? "multiple" : undefined,
          metaKeySelection: props.metaKeySelection,
          dataKey: props.dataKey,
          ...(props.filterToggle
            ? { filterDisplay: filterRowVisible.value ? "row" : undefined }
            : {}),
        } as any,
        {
          default: () => [
            ...transformColumns(),
            ...(hasActionsSlot
              ? [
                  h(
                    Column,
                    {
                      header: props.actionsHeader,
                      style: { width: props.actionsWidth + "px" },
                      pt: fixedRightPt,
                    },
                    { body: (sp: any) => slots.actions?.(sp) },
                  ),
                ]
              : []),
          ],
          header: headerContent,
          ...Object.fromEntries(forwardedSlots.map((n) => [n, slots[n]])),
        },
      );
  },
});
</script>