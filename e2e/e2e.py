#!/usr/bin/env python3
"""End-to-end UI tests for HKD TT152.

Drives the real Tauri/webview app through the WebKit remote inspector and
verifies outcomes against the app's SQLite database, which is treated as an
independent source of truth.

Requires the app to be running with WEBKIT_INSPECTOR_HTTP_SERVER pointing at the
inspector port, and E2E_DB pointing at the runtime database. `run_e2e.sh` wires
both up automatically.

Usage:
    python3 e2e.py                  # run every step
    python3 e2e.py login product    # run only the named steps
"""
import json
import os
import sqlite3
import sys
import time
import traceback

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from insp import Remote, value_of  # noqa: E402

_TMPDIR = os.environ.get("TMPDIR") or "/tmp"
DB = os.environ.get(
    "E2E_DB",
    os.path.join(
        _TMPDIR,
        "hkd-tt152-e2e",
        "data",
        "git.shin.hdk-tt152-app",
        "profiles",
        "default",
        "hkd.db",
    ),
)

RESULTS = []


# ─────────────────────────── inspector session ───────────────────────────


class Sess:
    def __init__(self):
        self.r = Remote()
        self.r.call("Runtime.enable")
        self._wait_ready()
        self._inject()

    def _wait_ready(self, timeout=90):
        """Wait for the webview to finish loading the frontend (cold Vite is slow)."""
        end = time.time() + timeout
        while time.time() < end:
            try:
                t = self.js("(document.body && document.body.innerText) || ''")
            except Exception:
                t = ""
            if t and ("Đăng nhập" in t or "Tổng quan" in t or "HKD Kế Toán" in t):
                return t
            time.sleep(0.5)
        return ""

    def js(self, expr, timeout=20):
        resp = self.r.call(
            "Runtime.evaluate",
            {"expression": expr, "returnByValue": True, "includeCommandLineAPI": True},
            timeout=timeout,
        )
        return value_of(resp)

    def wait(self, expr, timeout=12, desc=""):
        end = time.time() + timeout
        last = None
        while time.time() < end:
            last = self.js(expr)
            if last is True or (isinstance(last, str) and last not in ("", "false", "0")):
                return last
            if isinstance(last, (int, float)) and last not in (0,):
                return last
            time.sleep(0.25)
        raise AssertionError(f"timeout waiting for: {desc or expr} (last={last!r})")

    def text(self):
        return self.js("document.body.innerText")

    def _content_text(self):
        """Text of the router-view area (excludes the sidebar/header)."""
        return self.js(
            "(() => { const m=document.querySelector('main > div.flex-1');"
            " return m ? m.innerText.trim() : ''; })()"
        )

    def nav(self, path):
        if self.js("typeof window.__E") != "object":
            self._inject()
        self.js("window.__E.noAnim && window.__E.noAnim()")
        before = self._content_text()
        self.js(
            f"history.pushState({{}}, '', '{path}');"
            "window.dispatchEvent(new PopStateEvent('popstate'));"
        )
        # Route components are lazy imports, so the router-view keeps the old page
        # until the new component resolves. Wait for the content to actually change
        # instead of sleeping a fixed amount (which is flaky when chunks load slowly).
        deadline = time.time() + 20
        while time.time() < deadline:
            t = self._content_text()
            if t and t != before:
                time.sleep(0.3)
                break
            time.sleep(0.2)
        return self.text()

    def set_file(self, selector, path):
        """Attach a real file to an <input type=file>.

        WebKitGTK does not support DOM.setFileInputFiles, so build the File via
        DataTransfer: read the file -> base64 -> create a File inside the page ->
        assign it to input.files -> dispatch `change` so the component reacts.
        """
        import base64

        try:
            with open(path, "rb") as f:
                b64 = base64.b64encode(f.read()).decode()
        except OSError as e:
            return f"READ ERR: {e}"
        name = os.path.basename(path)
        expr = (
            "(() => { const sel=%s; const inp=document.querySelector(sel);"
            " if(!inp) return 'NO NODE';"
            " const bin=atob(%s); const arr=new Uint8Array(bin.length);"
            " for(let i=0;i<bin.length;i++) arr[i]=bin.charCodeAt(i);"
            " const file=new File([arr], %s,"
            " {type:'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet'});"
            " const dt=new DataTransfer(); dt.items.add(file); inp.files=dt.files;"
            " inp.dispatchEvent(new Event('change', {bubbles:true}));"
            " return 'ok'; })()"
        ) % (json.dumps(selector), json.dumps(b64), json.dumps(name))
        res = self.js(expr, timeout=30)
        if res != "ok":
            return res
        return "ok"

    def _inject(self):
        self.js(
            """
            window.__E = {
              set(el, v){
                const d = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(el), 'value');
                if (d && d.set) d.set.call(el, v); else el.value = v;
                el.dispatchEvent(new Event('input', {bubbles:true}));
                el.dispatchEvent(new Event('change', {bubbles:true}));
              },
              type(el, v){
                el.focus();
                const d = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(el), 'value');
                if (d && d.set) d.set.call(el, ''); else el.value = '';
                try { el.setSelectionRange(0, el.value.length); } catch (e) {}
                let ok = false;
                try { ok = document.execCommand('insertText', false, String(v)); } catch (e) { ok = false; }
                if (!ok) window.__E.set(el, v);
                el.dispatchEvent(new Event('change', {bubbles:true}));
                return el.value;
              },
              dlg(){ return [...document.querySelectorAll('.p-dialog')].filter(d => d.offsetParent !== null).pop() || null; },
              noAnim(){
                // Do NOT disable transitions globally: PrimeVue relies on
                // transitionend to unmount dialogs/toasts, so disabling them would
                // leave elements stuck and block the UI.
                return 'ok';
              },
              // Block the OS print dialog (it would hang the test) and count calls.
              stubPrint(){
                window.print = function(){ window.__printCalls = (window.__printCalls || 0) + 1; };
                return 'ok';
              },
              byText(sel, text, root){
                root = root || document;
                return [...root.querySelectorAll(sel)]
                  .find(e => e.innerText.trim() === text) || null;
              },
              fieldBox(label, root){
                root = root || document;
                const l = [...root.querySelectorAll('label')]
                  .find(x => x.innerText.trim().toLowerCase().includes(label.toLowerCase()));
                return l ? l.parentElement : null;
              },
              inputOf(label, root){
                const b = window.__E.fieldBox(label, root);
                return b ? b.querySelector('input,textarea') : null;
              },
              textInputs(root){
                root = root || document;
                return [...root.querySelectorAll('input')].filter(i => i.type === 'text' || i.type === '');
              },
              asyncSelect(selectIndex, optionText){
                const sels = [...document.querySelectorAll('.p-select')];
                const s = sels[selectIndex];
                if (!s) return 'NO SELECT ' + selectIndex;
                s.click();
                return 'opened ' + selectIndex;
              },
              setNumber(el, value){
                const root = el.closest('.p-inputnumber') || el.parentElement;
                const inst = root && root.__vueParentComponent;
                if (inst && inst.proxy && inst.proxy.$emit) {
                  inst.proxy.$emit('update:modelValue', value);
                  return 'emit:' + value;
                }
                return 'type:' + window.__E.type(el, value);
              },
              clickOption(optionText){
                const opts = [...document.querySelectorAll('.p-select-option, .p-select-item')]
                  .filter(o => o.offsetParent !== null);
                const o = opts.find(x => x.innerText.trim().includes(optionText));
                if (!o) return 'NO OPTION ' + optionText + ' | have: ' + opts.map(x=>x.innerText.trim()).join('; ');
                const mk = (t) => new MouseEvent(t, {bubbles:true, cancelable:true, view:window});
                ['pointerdown','mousedown','pointerup','mouseup','click'].forEach(t => o.dispatchEvent(mk(t)));
                return 'picked ' + optionText;
              },
            };
            window.__E.stubPrint();
            window.__E.noAnim()
            """
        )


def check(name, cond, detail=""):
    RESULTS.append((name, bool(cond), detail))
    tag = "PASS" if cond else "FAIL"
    print(f"[{tag}] {name}" + (f"  — {detail}" if (detail and not cond) else ""))


def wait_ok(s, expr, timeout=12, desc=""):
    """Like Sess.wait, but never raises — records a FAIL and returns False."""
    try:
        s.wait(expr, timeout=timeout, desc=desc)
        return True
    except AssertionError as e:
        check(desc or expr, False, str(e))
        return False


def wait_btn(s, label, timeout=12):
    """Wait until a visible button with the exact `label` appears."""
    return wait_ok(
        s,
        "[...document.querySelectorAll('button')]"
        f".some(b=>b.offsetParent!==null && b.innerText.trim()==={json.dumps(label)})",
        timeout=timeout,
        desc=f"button '{label}' ready",
    )


# ─────────────────────────── DB helpers (source of truth) ───────────────────────────


def db(fn):
    con = sqlite3.connect(DB)
    try:
        return fn(con)
    finally:
        con.close()


def scalar(sql, args=()):
    return db(lambda c: (c.execute(sql, args).fetchone() or [None])[0])


def row(sql, args=()):
    con = sqlite3.connect(DB)
    con.row_factory = sqlite3.Row
    try:
        r = con.execute(sql, args).fetchone()
        return dict(r) if r else None
    finally:
        con.close()


# ─────────────────────────── steps ───────────────────────────


def step_login(s):
    t = s.text()
    check("login: shows the login form", "Đăng nhập" in t)

    # 1) Wrong password must be rejected and keep us on the login screen.
    s.js(
        "(() => { const ins=document.querySelectorAll('input');"
        "window.__E.set(ins[0],'admin'); window.__E.set(ins[1],'sai-mat-khau');"
        "document.querySelector('button[type=submit]').click(); return 'ok'; })()"
    )
    s.wait(
        "[...document.querySelectorAll('.p-toast-message')]"
        ".some(e => e.innerText.includes('Đăng nhập thất bại'))",
        timeout=12,
        desc="login-failed toast",
    )
    time.sleep(0.4)
    check("login: wrong password is rejected", "Tài khoản mặc định" in s.text())

    # 2) Correct credentials reach the app.
    s.js(
        "(() => { const ins=document.querySelectorAll('input');"
        "window.__E.set(ins[0],'admin'); window.__E.set(ins[1],'admin123');"
        "document.querySelector('button[type=submit]').click(); return 'ok'; })()"
    )
    s.wait(
        "[...document.querySelectorAll('button')]"
        ".some(b=>b.innerText.trim()==='Tổng quan')",
        timeout=15,
        desc="left the login screen",
    )
    time.sleep(0.6)
    t = s.text()
    check(
        "login: reaches the app after sign-in",
        "Tài khoản mặc định" not in t and ("Tổng quan" in t or "Danh mục" in t),
        t[:200],
    )
    check("DB: admin user exists", scalar("SELECT COUNT(*) FROM app_user WHERE username='admin'") == 1)


def step_product(s):
    s.nav("/products")
    t = s.text()
    check("product: opens the products page", "Danh mục sản phẩm" in t, t[:200])
    s.js(
        "(() => { const b=[...document.querySelectorAll('button')]"
        ".find(x=>x.innerText.trim()==='Thêm sản phẩm'); if(!b) return 'NO BTN'; b.click(); return 'ok'; })()"
    )
    s.wait("!!window.__E.dlg()", desc="product dialog open")
    s.js(
        "(() => { const d=window.__E.dlg();"
        "const ins=[...d.querySelectorAll('input')].filter(i=>i.type==='text'||i.type==='');"
        "window.__E.set(ins[0],'SP001'); window.__E.set(ins[1],'Cái'); window.__E.set(ins[2],'Sản phẩm test E2E');"
        "return 'filled '+ins.length; })()"
    )
    r = s.js(
        "(() => { const d=window.__E.dlg();"
        "const b=[...d.querySelectorAll('button')].find(x=>x.innerText.trim()==='Lưu');"
        "if(!b) return 'NO SAVE'; b.click(); return 'ok'; })()"
    )
    check("product: clicks Save", r == "ok", r)
    time.sleep(1.2)
    s.wait("!window.__E.dlg()", desc="dialog closed")
    p = row("SELECT code,name,unit FROM product WHERE code='SP001'")
    check("DB: product SP001 saved", p is not None, str(p))
    if p:
        check("DB: correct name/unit", p["name"] == "Sản phẩm test E2E" and p["unit"] == "Cái", str(p))


def _dump_state(s, tag):
    toast = s.js(
        "[...document.querySelectorAll('.p-toast-message')]"
        ".map(e=>e.innerText.replace(/\\n/g,' | ')).join(' || ')"
    )
    label = s.js(
        "(() => { const d=window.__E.dlg();"
        " return (d && d.querySelector('.p-datatable-tbody .p-select-label')||{}).innerText || ''; })()"
    )
    print(f"   [{tag}] toast: {toast!r}")
    print(f"   [{tag}] selected product: {label!r}")


def _click_dialog_btn(s, label):
    return s.js(
        "(() => { const d=window.__E.dlg(); if(!d) return 'NO DIALOG';"
        f" const b=[...d.querySelectorAll('button')].find(x=>x.innerText.trim()==={json.dumps(label)});"
        " if(!b) return 'NO BTN'; b.click(); return 'ok'; })()"
    )


def _click_visible_btn(s, label, contains=False):
    """Click a visible button matching `label` exactly (or containing it)."""
    match = (
        f"x.innerText.trim().includes({json.dumps(label)})"
        if contains
        else f"x.innerText.trim()==={json.dumps(label)}"
    )
    return s.js(
        "(() => { const bs=[...document.querySelectorAll('button')]"
        ".filter(b=>b.offsetParent!==null);"
        f" const b=bs.find(x=>{match});"
        " if(!b) return 'NO BTN: '+bs.map(x=>x.innerText.trim()).filter(Boolean).join('; ');"
        " b.click(); return 'ok'; })()"
    )


def _force_close_dialog(s):
    """Close the open dialog (Hủy/Đóng/Quay lại or the X button) so it never blocks later steps."""
    return s.js(
        "(() => { const d=window.__E.dlg(); if(!d) return 'none';"
        " const b=[...d.querySelectorAll('button')]"
        ".find(x=>['Hủy','Đóng','Quay lại'].includes(x.innerText.trim()));"
        " if(b){ b.click(); return 'clicked ' + b.innerText.trim(); }"
        " const x=d.querySelector('.p-dialog-close-button, .p-dialog-header-close');"
        " if(x){ x.click(); return 'closed X'; }"
        " return 'no close button'; })()"
    )


def _pick_row_option(s, option_text, select_index=0):
    info = s.js(
        "(() => { const d=window.__E.dlg();"
        " const row=d.querySelector('.p-datatable-tbody > tr'); if(!row) return 'NO ROW';"
        " const sels=[...row.querySelectorAll('.p-select')];"
        " return JSON.stringify({n:sels.length, labels:sels.map(x=>(x.querySelector('.p-select-label')||{}).innerText)}); })()"
    )
    print(f"   [row selects] {info}")
    r = s.js(
        "(() => { const d=window.__E.dlg();"
        " const row=d.querySelector('.p-datatable-tbody > tr'); if(!row) return 'NO ROW';"
        f" const sel=[...row.querySelectorAll('.p-select')][{select_index}]; if(!sel) return 'NO SELECT';"
        " sel.click(); return 'ok'; })()"
    )
    if r != "ok":
        return r
    time.sleep(0.7)
    opts = s.js(
        "JSON.stringify([...document.querySelectorAll('.p-select-option, .p-select-item')]"
        ".map(o=>({t:o.innerText.trim(), vis:o.offsetParent!==null})))"
    )
    print(f"   [options] {opts}")
    res = s.js(f"window.__E.clickOption({json.dumps(option_text)})")
    time.sleep(0.4)
    print(f"   [after pick] {res}")
    return res


def _pick_dialog_select(s, option_text, select_index):
    r = s.js(
        "(() => { const d=window.__E.dlg(); if(!d) return 'NO DIALOG';"
        f" const sel=[...d.querySelectorAll('.p-select')][{select_index}]; if(!sel) return 'NO SELECT';"
        " sel.click(); return 'ok'; })()"
    )
    if r != "ok":
        return r
    time.sleep(0.7)
    return s.js(f"window.__E.clickOption({json.dumps(option_text)})")


def _set_row_numbers(s, values):
    arr = json.dumps([str(v) for v in values])
    return s.js(
        "(() => { const d=window.__E.dlg();"
        " const row=d.querySelector('.p-datatable-tbody > tr'); if(!row) return 'NO ROW';"
        f" const vals={arr};"
        " const n=[...row.querySelectorAll('.p-inputnumber input')];"
        " const res=[]; vals.forEach((v,i)=>{ if(n[i]) res.push(window.__E.setNumber(n[i], Number(v))); });"
        " return JSON.stringify(res); })()"
    )


def _catalog_tab(s, label):
    return s.js(
        "(() => { const t=[...document.querySelectorAll('.p-tab')]"
        f".find(x=>x.innerText.trim().includes({json.dumps(label)}));"
        " if(!t) return 'NO TAB'; t.click(); return 'ok'; })()"
    )


def _catalog_create(s, btn, code, name, mst=""):
    s.js(
        "(() => { const b=[...document.querySelectorAll('button')]"
        f".find(x=>x.innerText.trim()==={json.dumps(btn)}); if(!b) return 'NO'; b.click(); return 'ok'; }})()"
    )
    s.wait("!!window.__E.dlg()", desc=f"dialog {btn}")
    print(
        "   [catalog dlg] "
        + str(
            s.js(
                "(() => { const d=window.__E.dlg(); if(!d) return 'NO DIALOG';"
                " return JSON.stringify({labels:[...d.querySelectorAll('label')].map(l=>l.innerText.trim()),"
                " vals:[...d.querySelectorAll('input')].map(i=>i.value)}); })()"
            )
        )
    )
    r = s.js(
        "(() => { const d=window.__E.dlg();"
        " const set=(l,v)=>{const el=window.__E.inputOf(l,d); if(el) window.__E.set(el,v);};"
        f" set('Mã',{json.dumps(code)}); set('Tên',{json.dumps(name)});"
        + (f" set('MST',{json.dumps(mst)});" if mst else "")
        + " return 'ok'; })()"
    )
    check(f"{btn}: fills the form", r == "ok", r)
    print(
        "   [after fill] "
        + str(
            s.js(
                "(() => { const d=window.__E.dlg();"
                " return JSON.stringify([...d.querySelectorAll('input')].map(i=>i.value)); })()"
            )
        )
    )
    check(f"{btn}: clicks Save", _click_dialog_btn(s, "Lưu") == "ok")
    time.sleep(1.2)
    print(
        "   [toast] "
        + str(
            s.js(
                "[...document.querySelectorAll('.p-toast-message')].map(e=>e.innerText.replace(/\\n/g,' | ')).join(' || ')"
            )
        )
    )
    print(
        "   [dialogs] "
        + str(
            s.js(
                "JSON.stringify([...document.querySelectorAll('.p-dialog')].map(d=>({"
                "title:(d.querySelector('.p-dialog-title')||{}).innerText,"
                "vis:d.offsetParent!==null,"
                "disp:getComputedStyle(d).display,"
                "mask:(d.closest('.p-dialog-mask')?getComputedStyle(d.closest('.p-dialog-mask')).display:'-')"
                "})))"
            )
        )
    )
    s.wait("!window.__E.dlg()", desc=f"dialog {btn} closed")


def step_warehouse(s):
    time.sleep(0.5)
    t = s.nav("/catalogs")
    check("catalogs: opens the catalogs page", "Kho hàng" in t, t[:200])

    _catalog_create(s, "Thêm kho", "KHO1", "Kho chính")
    w = row("SELECT id,code,name FROM warehouse WHERE code='KHO1'")
    check("DB: warehouse KHO1 saved", w is not None, str(w))

    check("catalogs: opens the Customers tab", _catalog_tab(s, "Khách hàng") == "ok")
    time.sleep(0.5)
    _catalog_create(s, "Thêm khách hàng", "KH001", "Khách A", "0123456789")
    c = row("SELECT code,name FROM customer WHERE code='KH001'")
    check("DB: customer KH001 saved", c is not None, str(c))

    check("catalogs: opens the Suppliers tab", _catalog_tab(s, "Nhà cung cấp") == "ok")
    time.sleep(0.5)
    _catalog_create(s, "Thêm nhà cung cấp", "NCC001", "NCC A")
    sp = row("SELECT code,name FROM supplier WHERE code='NCC001'")
    check("DB: supplier NCC001 saved", sp is not None, str(sp))


def step_inbound(s):
    time.sleep(0.5)
    t = s.nav("/inbound")
    check("inbound: opens the goods-receipt page", "Phiếu nhập kho" in t, t[:200])
    s.js(
        "(() => { const b=[...document.querySelectorAll('button')]"
        ".find(x=>x.innerText.trim()==='Tạo phiếu nhập'); if(!b) return 'NO'; b.click(); return 'ok'; })()"
    )
    s.wait("!!window.__E.dlg()", desc="receipt dialog open")
    check("inbound: clicks Add row", _click_dialog_btn(s, "Thêm dòng") == "ok")
    time.sleep(0.6)
    r = _pick_row_option(s, "Sản phẩm test E2E")
    check("inbound: picks a product", str(r).startswith("picked"), r)
    time.sleep(0.5)
    r = _set_row_numbers(s, [2, 100000])
    print(f"   [set nums] {r}")
    print(
        "   [nums now] "
        + str(
            s.js(
                "(() => { const d=window.__E.dlg(); const row=d.querySelector('.p-datatable-tbody > tr');"
                " return JSON.stringify([...row.querySelectorAll('.p-inputnumber input')].map(i=>i.value)); })()"
            )
        )
    )
    time.sleep(0.4)
    check("inbound: clicks Save voucher", _click_dialog_btn(s, "Lưu phiếu") == "ok")
    time.sleep(1.6)
    _dump_state(s, "inbound")
    s.wait("!window.__E.dlg()", desc="receipt dialog closed")
    je = row(
        "SELECT quantity,unit_price,amount FROM journal_entry"
        " WHERE entry_type='PN' AND product_code='SP001' ORDER BY id DESC LIMIT 1"
    )
    check("DB: PN receipt posted for SP001", je is not None, str(je))
    if je:
        check(
            "DB: PN qty/price/amount correct",
            je["quantity"] == 2.0 and je["unit_price"] == 100000.0 and je["amount"] == 200000.0,
            str(je),
        )
    lot = scalar(
        "SELECT COALESCE(SUM(sl.quantity),0) FROM stock_lot sl"
        " JOIN product p ON p.id=sl.product_id WHERE p.code='SP001' AND sl.depleted=0"
    )
    check("DB: stock lot SP001 = 2", lot == 2.0, str(lot))


def step_outbound(s):
    time.sleep(0.5)
    t = s.nav("/outbound")
    check("outbound: opens the goods-issue page", "Phiếu xuất kho" in t, t[:200])
    s.js(
        "(() => { const b=[...document.querySelectorAll('button')]"
        ".find(x=>x.innerText.trim()==='Tạo phiếu xuất'); if(!b) return 'NO'; b.click(); return 'ok'; })()"
    )
    s.wait("!!window.__E.dlg()", desc="issue dialog open")
    check("outbound: clicks Add row", _click_dialog_btn(s, "Thêm dòng") == "ok")
    time.sleep(0.6)
    r = _pick_row_option(s, "Sản phẩm test E2E")
    check("outbound: picks a product", str(r).startswith("picked"), r)
    time.sleep(0.5)
    r2 = _pick_row_option(s, "Phân phối, cung cấp hàng hóa", select_index=1)
    check("outbound: picks the PPHH industry", str(r2).startswith("picked"), r2)
    time.sleep(0.5)
    _set_row_numbers(s, [1, 150000])
    time.sleep(0.4)
    check("outbound: clicks Save voucher", _click_dialog_btn(s, "Lưu phiếu") == "ok")
    time.sleep(1.6)
    _dump_state(s, "outbound")
    s.wait("!window.__E.dlg()", desc="issue dialog closed")
    je = row(
        "SELECT quantity,unit_price,amount,industry_code FROM journal_entry"
        " WHERE entry_type='PX' AND product_code='SP001' ORDER BY id DESC LIMIT 1"
    )
    check("DB: PX sale posted for SP001", je is not None, str(je))
    if je:
        check(
            "DB: PX qty/amount/industry correct",
            je["quantity"] == 1.0 and je["amount"] == 150000.0 and je["industry_code"] == "PPHH",
            str(je),
        )
    rem = scalar(
        "SELECT COALESCE(SUM(sl.quantity),0) FROM stock_lot sl"
        " JOIN product p ON p.id=sl.product_id WHERE p.code='SP001' AND sl.depleted=0"
    )
    check("DB: FIFO consumed a lot, 1 left", rem == 1.0, str(rem))


def step_invoice(s):
    time.sleep(0.5)
    t = s.nav("/invoices")
    check("invoice: opens the invoices page", "Hóa đơn" in t, t[:200])
    wait_btn(s, "Lập hóa đơn nháp")
    r = _click_visible_btn(s, "Lập hóa đơn nháp")
    check("invoice: opens the draft dialog", r == "ok", r)
    if not wait_ok(s, "!!window.__E.dlg()", 10, "invoice dialog open"):
        return
    r = _pick_dialog_select(s, "Khách A", 0)
    check("invoice: picks a customer", str(r).startswith("picked"), r)
    time.sleep(0.4)
    check("invoice: clicks Add row", _click_dialog_btn(s, "Thêm dòng") == "ok")
    time.sleep(0.6)
    r = _pick_row_option(s, "Sản phẩm test E2E")
    check("invoice: picks a product", str(r).startswith("picked"), r)
    time.sleep(0.5)
    _set_row_numbers(s, [1, 150000])
    time.sleep(0.4)
    check("invoice: clicks Create invoice", _click_dialog_btn(s, "Lập hóa đơn") == "ok")
    time.sleep(1.6)
    if not wait_ok(s, "!window.__E.dlg()", 8, "invoice dialog closed"):
        _force_close_dialog(s)
    inv = row("SELECT number,customer,total,vat_amount,status FROM invoice ORDER BY id DESC LIMIT 1")
    check("DB: draft invoice saved", inv is not None, str(inv))
    if inv:
        check(
            "DB: invoice customer + total correct",
            inv["customer"] == "Khách A" and inv["total"] == 150000.0,
            str(inv),
        )


def step_cash(s):
    time.sleep(0.5)
    t = s.nav("/cash")
    check("cash: opens the cash page", "Thu" in t and "Chi" in t, t[:200])
    s.js(
        "(() => { const b=[...document.querySelectorAll('button')]"
        ".find(x=>x.innerText.trim()==='Tạo phiếu'); if(!b) return 'NO'; b.click(); return 'ok'; })()"
    )
    s.wait("!!window.__E.dlg()", desc="cash dialog open")
    r = s.js(
        "(() => { const d=window.__E.dlg();"
        " const el=window.__E.inputOf('Số tiền',d); if(!el) return 'NO FIELD';"
        " return window.__E.setNumber(el, 500000); })()"
    )
    check("cash: enters an amount", str(r).startswith(("emit", "type")), r)
    time.sleep(0.4)
    check("cash: clicks Save voucher", _click_dialog_btn(s, "Lưu phiếu") == "ok")
    time.sleep(1.6)
    s.wait("!window.__E.dlg()", desc="cash dialog closed")
    je = row(
        "SELECT entry_type,amount,debit_account,credit_account FROM journal_entry"
        " WHERE entry_type='PT' ORDER BY id DESC LIMIT 1"
    )
    check("DB: PT receipt posted", je is not None, str(je))
    if je:
        check("DB: PT amount correct", je["amount"] == 500000.0, str(je))


def step_employee(s):
    time.sleep(0.5)
    t = s.nav("/payroll")
    check("employee: opens the payroll page", "Bảng lương" in t or "Nhân viên" in t, t[:200])
    wait_btn(s, "Thêm nhân viên")
    s.js(
        "(() => { const b=[...document.querySelectorAll('button')]"
        ".find(x=>x.innerText.trim()==='Thêm nhân viên'); if(!b) return 'NO'; b.click(); return 'ok'; })()"
    )
    if not wait_ok(s, "!!window.__E.dlg()", 10, "employee dialog open"):
        return
    r = s.js(
        "(() => { const d=window.__E.dlg();"
        " const set=(l,v)=>{const el=window.__E.inputOf(l,d); if(el) window.__E.set(el,v);};"
        " set('Mã NV','NV001'); set('Họ và tên','Nguyễn Văn A'); set('Bộ phận','Kế toán');"
        " const num=(l,v)=>{const el=window.__E.inputOf(l,d); if(el) window.__E.setNumber(el,v);};"
        " num('Lương theo HĐ',5000000); num('Lương đóng BH',5000000);"
        " return 'ok'; })()"
    )
    check("employee: fills the form", r == "ok", r)
    r = _click_dialog_btn(s, "Lưu")
    check("employee: clicks Save", r == "ok", r)
    time.sleep(1.4)
    if not wait_ok(s, "!window.__E.dlg()", 8, "employee dialog closed"):
        _force_close_dialog(s)
    e = row("SELECT code,name,basic_salary,bh_salary FROM employee WHERE code='NV001'")
    check("DB: employee NV001 saved", e is not None, str(e))
    if e:
        check(
            "DB: contract/insurance salary correct",
            e["basic_salary"] == 5000000.0 and e["bh_salary"] == 5000000.0,
            str(e),
        )


def step_attendance(s):
    time.sleep(0.5)
    t = s.nav("/attendance")
    check("attendance: opens the attendance page", "Bảng chấm công" in t, t[:200])
    s.wait(
        "[...document.querySelectorAll('button')]"
        ".some(b=>b.innerText.trim()==='Cả tháng' && !b.disabled)",
        desc="'Cả tháng' button ready",
    )
    r = s.js(
        "(() => { const b=[...document.querySelectorAll('button')]"
        ".find(x=>x.innerText.trim()==='Cả tháng'); if(!b) return 'NO'; b.click(); return 'ok'; })()"
    )
    check("attendance: clicks 'Cả tháng' (fills X)", r == "ok", r)
    time.sleep(0.8)
    tot = s.js(
        "(() => { const tr=document.querySelector('table tbody tr'); if(!tr) return '';"
        " const tds=[...tr.querySelectorAll('td')];"
        " return tds.length ? tds[tds.length-3].innerText.trim() : ''; })()"
    )
    try:
        totf = float(tot)
    except (TypeError, ValueError):
        totf = 0.0
    check("attendance: total days > 0", totf > 0, f"days={tot!r}")
    r = s.js(
        "(() => { const b=[...document.querySelectorAll('button')]"
        ".find(x=>x.innerText.trim()==='Lưu chấm công'); if(!b) return 'NO';"
        " if(b.disabled) return 'DISABLED'; b.click(); return 'ok'; })()"
    )
    check("attendance: clicks Save attendance", r == "ok", r)
    time.sleep(1.6)
    n = scalar("SELECT COUNT(*) FROM attendance WHERE status='X'")
    check("DB: attendance rows with X exist", (n or 0) >= 20, f"X={n}")


def step_payroll(s):
    time.sleep(0.5)
    t = s.nav("/payroll")
    check("payroll: opens the payroll page", "Bảng lương" in t, t[:200])
    r = s.js(
        "(() => { const b=[...document.querySelectorAll('button')]"
        ".find(x=>x.innerText.trim()==='Lập bảng lương'); if(!b) return 'NO'; b.click(); return 'ok'; })()"
    )
    check("payroll: clicks Build payroll", r == "ok", r)
    time.sleep(1.5)
    s.wait(
        "[...document.querySelectorAll('button')]"
        ".some(b=>b.innerText.trim()==='Lấy công từ chấm công' && !b.disabled)",
        desc="payroll table has rows",
    )
    r = s.js(
        "(() => { const b=[...document.querySelectorAll('button')]"
        ".find(x=>x.innerText.trim()==='Lấy công từ chấm công'); if(!b) return 'NO'; b.click(); return 'ok'; })()"
    )
    check("payroll: clicks Pull attendance days", r == "ok", r)
    time.sleep(1.5)
    wd = s.js(
        "(() => { const t=[...document.querySelectorAll('.p-datatable')]"
        ".find(d => d.offsetParent !== null && d.querySelector('.p-inputnumber input'));"
        " const i = t ? t.querySelector('.p-inputnumber input') : null;"
        " return i ? i.value : ''; })()"
    )
    digits = "".join(ch for ch in str(wd) if ch.isdigit() or ch == ".")
    try:
        wdf = float(digits or 0)
    except ValueError:
        wdf = 0.0
    check("payroll: attendance days pulled > 0", wdf > 0, f"work_days={wd!r}")
    r = s.js(
        "(() => { const b=[...document.querySelectorAll('button')]"
        ".find(x=>x.innerText.trim()==='Lưu bảng lương'); if(!b) return 'NO';"
        " if(b.disabled) return 'DISABLED'; b.click(); return 'ok'; })()"
    )
    check("payroll: clicks Save payroll", r == "ok", r)
    time.sleep(1.8)
    p = row(
        "SELECT period,work_days,gross_salary,bh_employee,net_pay FROM payroll"
        " ORDER BY id DESC LIMIT 1"
    )
    check("DB: payroll saved", p is not None, str(p))
    if p:
        check("DB: work days match attendance (>=20)", p["work_days"] >= 20, str(p))
        check("DB: net_pay > 0", (p["net_pay"] or 0) > 0, str(p))


def step_print(s):
    time.sleep(0.4)
    s.js("window.__printCalls = 0; window.__E.stubPrint(); 'ok'")
    # Unknown voucher (navigate away first so the view remounts).
    s.nav("/")
    t2 = s.nav("/print/ZZ999")
    check("print: unknown voucher reports an error", "Không tìm thấy chứng từ" in t2, t2[:200])
    # PN001 was created by the `inbound` step.
    s.nav("/")
    t = s.nav("/print/PN001")
    check("print: opens receipt PN001", "PHIẾU NHẬP KHO" in t, t[:200])
    check("print: uses form 01-VT", "01-VT" in t, t[:200])
    check("print: has the product line", "Sản phẩm test E2E" in t, t[:200])
    check("print: total 200.000", "200.000" in t, t[:300])
    check("print: amount in words", "Hai trăm nghìn đồng chẵn" in t, t[-300:])
    # Auto-print runs on mount via a setTimeout(400ms), so wait for the call.
    try:
        s.wait("window.__printCalls >= 1", timeout=8, desc="print call")
        printed = True
    except AssertionError:
        printed = False
    n = s.js("window.__printCalls")
    check("print: calls the print command", printed and isinstance(n, (int, float)) and n >= 1, str(n))


def step_backup(s):
    time.sleep(0.4)
    t = s.nav("/accounting")
    check("backup: opens the accounting page", "Thuế" in t or "Kế toán" in t, t[:200])
    wait_btn(s, "Sao lưu dữ liệu")
    r = _click_visible_btn(s, "Sao lưu dữ liệu")
    check("backup: opens the backup dialog", r == "ok", r)
    if not wait_ok(s, "!!window.__E.dlg()", 10, "backup dialog"):
        return
    check("backup: clicks Create backup", _click_dialog_btn(s, "Tạo sao lưu") == "ok")
    pat = r"hkd-backup-\d{8}-\d{6}\.db"
    found = wait_ok(
        s,
        "(() => { const d=window.__E.dlg(); if(!d) return false;"
        f" return [...d.querySelectorAll('.p-datatable-tbody td')].some(td=>/{pat}/.test(td.innerText)); }})()",
        timeout=12,
        desc="backup appears in the list",
    )
    name = ""
    if found:
        name = s.js(
            "(() => { const d=window.__E.dlg();"
            " const c=[...d.querySelectorAll('.p-datatable-tbody td')].map(td=>td.innerText.trim())"
            f".find(x=>/{pat}/.test(x)); return c||''; }})()"
        )
    check("backup: list has a valid filename", bool(name), name)
    app_dir = os.path.dirname(os.path.dirname(os.path.dirname(DB)))
    fpath = os.path.join(app_dir, "backups", name) if name else ""
    check("backup: file exists on disk", bool(fpath) and os.path.isfile(fpath), fpath)
    if fpath and os.path.isfile(fpath):
        check("backup: file > 4KB", os.path.getsize(fpath) > 4096, str(os.path.getsize(fpath)))
    check(
        "DB: audit log has 'backup'",
        (scalar("SELECT COUNT(*) FROM audit_log WHERE action='backup'") or 0) >= 1,
    )
    check("backup: closes the dialog", _click_dialog_btn(s, "Đóng") == "ok")
    if not wait_ok(s, "!window.__E.dlg()", 8, "backup dialog closed"):
        _force_close_dialog(s)


def step_users(s):
    time.sleep(0.4)
    t = s.nav("/users")
    check("users: opens the users page", "Người dùng" in t, t[:200])
    wait_btn(s, "Thêm người dùng")
    r = _click_visible_btn(s, "Thêm người dùng")
    check("users: opens the add form", r == "ok", r)
    if not wait_ok(s, "!!window.__E.dlg()", 10, "add-user dialog"):
        return
    r = s.js(
        "(() => { const d=window.__E.dlg();"
        " const set=(l,v)=>{const el=window.__E.inputOf(l,d); if(!el) return false; window.__E.set(el,v); return true;};"
        " return JSON.stringify([set('Tên đăng nhập','kho01'), set('Họ tên','Kho 01'), set('Mật khẩu','kho1234')]); })()"
    )
    check("users: fills the form", "false" not in str(r), r)
    time.sleep(0.3)
    r = _pick_dialog_select(s, "Kho (kho)", 0)
    check("users: picks the Kho role", str(r).startswith("picked"), r)
    time.sleep(0.4)
    check("users: clicks Save", _click_dialog_btn(s, "Lưu") == "ok")
    time.sleep(1.4)
    if not wait_ok(s, "!window.__E.dlg()", 8, "user dialog closed"):
        _force_close_dialog(s)
    u = row("SELECT username,role,active,password_hash FROM app_user WHERE username='kho01'")
    check("DB: user kho01 saved", u is not None, str(u))
    if u:
        check("DB: role kho + active", u["role"] == "kho" and u["active"] == 1, str(u))
        check(
            "DB: password is not stored in plain text",
            u["password_hash"] not in ("kho1234", "admin123") and len(u["password_hash"]) >= 32,
            str(u["password_hash"])[:16],
        )


def step_import(s):
    time.sleep(0.4)
    path = os.environ.get("E2E_IMPORT_FILE", "")
    if not path or not os.path.isfile(path):
        check("import: test Excel file present", False, f"missing E2E_IMPORT_FILE={path!r}")
        return
    before = scalar("SELECT COUNT(*) FROM journal_entry") or 0
    t = s.nav("/import")
    check("import: opens the import page", "Nhập liệu" in t, t[:200])
    r = s.set_file("input[type=file]", path)
    check("import: attaches the Excel file", r == "ok", r)
    time.sleep(0.6)
    wait_btn(s, "Đọc file")
    check("import: clicks Read file", _click_visible_btn(s, "Đọc file") == "ok")
    if not wait_ok(
        s,
        "[...document.querySelectorAll('.p-tag')]"
        ".some(e=>e.innerText.includes('Nhận diện được: 2 dòng'))",
        timeout=15,
        desc="recognizes 2 rows",
    ):
        return
    tags = s.js("JSON.stringify([...document.querySelectorAll('.p-tag')].map(e=>e.innerText.trim()))")
    check("import: preview shows 2 rows", "Nhận diện được: 2 dòng" in tags, tags)
    r = _click_visible_btn(s, "Nhập 2 dòng vào phần mềm")
    check("import: clicks Import rows", r == "ok", r)
    if not wait_ok(
        s,
        "[...document.querySelectorAll('button')]"
        ".some(b=>b.offsetParent!==null && b.innerText.trim()==='Nhập dữ liệu')",
        timeout=10,
        desc="import confirmation box",
    ):
        return
    check("import: confirms the import", _click_visible_btn(s, "Nhập dữ liệu") == "ok")
    time.sleep(2.0)
    after = scalar("SELECT COUNT(*) FROM journal_entry") or 0
    check("import: writes 2 extra journal rows", after - before == 2, f"{before}->{after}")
    pn = row(
        "SELECT voucher_no,product_code,quantity,amount,note"
        " FROM journal_entry WHERE voucher_no='PN101'"
    )
    check(
        "import: entry PN101 correct",
        pn is not None and pn["amount"] == 300000.0 and pn["product_code"] == "SP001",
        str(pn),
    )
    px = row(
        "SELECT voucher_no,customer_code,quantity,amount"
        " FROM journal_entry WHERE voucher_no='PX101'"
    )
    check(
        "import: entry PX101 correct",
        px is not None and px["customer_code"] == "KH001",
        str(px),
    )
    check(
        "DB: audit log has 'import'",
        (scalar("SELECT COUNT(*) FROM audit_log WHERE action='import'") or 0) >= 1,
    )


def step_role(s):
    time.sleep(0.4)
    r = s.js(
        "(() => { const b=[...document.querySelectorAll('button')]"
        ".find(x=>x.getAttribute('aria-label')==='Đăng xuất'); if(!b) return 'NO'; b.click(); return 'ok'; })()"
    )
    check("role: logs out admin", r == "ok", r)
    s.wait(
        "[...document.querySelectorAll('button')].some(b=>b.innerText.trim()==='Đăng nhập')",
        timeout=12,
        desc="back to the login screen",
    )
    s.js(
        "(() => { const ins=document.querySelectorAll('input');"
        "window.__E.set(ins[0],'kho01'); window.__E.set(ins[1],'kho1234');"
        "document.querySelector('button[type=submit]').click(); return 'ok'; })()"
    )
    s.wait(
        "[...document.querySelectorAll('button')].some(b=>b.innerText.trim()==='Tổng quan')",
        timeout=15,
        desc="kho01 reaches the app",
    )
    time.sleep(0.6)
    aside = s.js("document.querySelector('aside').innerText")
    check("role: kho menu hides 'Người dùng'", "Người dùng" not in aside, aside)
    check("role: kho menu hides 'Bảng lương'", "Bảng lương" not in aside, aside)
    check("role: kho menu hides 'Kế toán HKD'", "Kế toán HKD" not in aside, aside)
    check(
        "role: kho menu shows 'Nhập kho' + 'Sản phẩm'",
        "Nhập kho" in aside and "Sản phẩm" in aside,
        aside,
    )
    hdr = s.js("document.querySelector('header').innerText")
    check("role: header shows the Kho role", "Kho" in hdr, hdr)

    # Restore the admin session for a clean end state.
    s.js(
        "(() => { const b=[...document.querySelectorAll('button')]"
        ".find(x=>x.getAttribute('aria-label')==='Đăng xuất'); if(b) b.click(); return 'ok'; })()"
    )
    s.wait(
        "[...document.querySelectorAll('button')].some(b=>b.innerText.trim()==='Đăng nhập')",
        timeout=12,
        desc="kho01 logged out",
    )
    s.js(
        "(() => { const ins=document.querySelectorAll('input');"
        "window.__E.set(ins[0],'admin'); window.__E.set(ins[1],'admin123');"
        "document.querySelector('button[type=submit]').click(); return 'ok'; })()"
    )
    s.wait(
        "[...document.querySelectorAll('button')].some(b=>b.innerText.trim()==='Tổng quan')",
        timeout=15,
        desc="admin logged back in",
    )
    check(
        "role: admin can log back in",
        "Người dùng" in s.js("document.querySelector('aside').innerText"),
    )


def step_reports_smoke(s):
    for path, needle in [("/ledger", "Nhật ký"), ("/accounting", "Thuế"), ("/inventory", "Tồn")]:
        t = s.nav(path)
        check(f"report {path}: loads", needle in t, t[:150])


STEPS = {
    "login": step_login,
    "warehouse": step_warehouse,
    "product": step_product,
    "inbound": step_inbound,
    "outbound": step_outbound,
    "print": step_print,
    "invoice": step_invoice,
    "cash": step_cash,
    "backup": step_backup,
    "employee": step_employee,
    "attendance": step_attendance,
    "payroll": step_payroll,
    "import": step_import,
    "users": step_users,
    "reports": step_reports_smoke,
    "role": step_role,
}


def main():
    names = sys.argv[1:] or list(STEPS)
    s = Sess()
    try:
        for n in names:
            if n not in STEPS:
                print(f"(skipping unknown step: {n})")
                continue
            print(f"\n── {n} ──")
            try:
                STEPS[n](s)
            except Exception as e:
                check(f"{n}: no runtime error", False, f"{type(e).__name__}: {e}")
                traceback.print_exc()
    finally:
        passed = sum(1 for _, ok, _ in RESULTS if ok)
        print(f"\n═══ RESULT: {passed}/{len(RESULTS)} passed ═══")
        for name, ok, det in RESULTS:
            if not ok:
                print(f"  FAIL {name} — {det}")
    sys.exit(0 if all(ok for _, ok, _ in RESULTS) else 1)


if __name__ == "__main__":
    main()
