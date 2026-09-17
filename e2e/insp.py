#!/usr/bin/env python3
"""Talk to the WebKit webview via the remote-inspector Target forwarding protocol.

Usage:
  insp.py <js-expression>          # Runtime.evaluate in the page, print value
  insp.py --html                   # print outerHTML (truncated)
  insp.py --text                   # print body innerText
  insp.py --raw <js-expression>    # dump raw inner responses
Env: INSPECT_HOST/INSPECT_PORT/INSPECT_PATH
"""
import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cdp import Cdp, recv_message  # transport


class Remote:
    def __init__(self):
        self.c = Cdp()
        self.target_id = None
        self._id = 0
        self._collect_targets(2.0)

    def _collect_targets(self, seconds):
        end = time.time() + seconds
        self.c.s.settimeout(0.5)
        while time.time() < end:
            try:
                op, payload = recv_message(self.c.s)
            except Exception:
                continue
            if op != 0x1:
                continue
            try:
                obj = json.loads(payload.decode())
            except Exception:
                continue
            if obj.get("method") == "Target.targetCreated":
                ti = obj["params"]["targetInfo"]
                if ti.get("type") == "page":
                    self.target_id = ti["targetId"]
        self.c.s.settimeout(15)

    def call(self, method, params=None, timeout=10):
        if not self.target_id:
            raise RuntimeError("no page target found")
        self._id += 1
        inner = {"id": self._id, "method": method, "params": params or {}}
        self.c.send("Target.sendMessageToTarget",
                    {"targetId": self.target_id, "message": json.dumps(inner)})
        end = time.time() + timeout
        while time.time() < end:
            op, payload = recv_message(self.c.s)
            if op != 0x1:
                continue
            obj = json.loads(payload.decode())
            if obj.get("method") == "Target.dispatchMessageFromTarget":
                msg = json.loads(obj["params"]["message"])
                if msg.get("id") == self._id:
                    return msg
        raise TimeoutError(f"{method} timed out")


def value_of(resp):
    if "error" in resp:
        return "ERROR: " + json.dumps(resp["error"], ensure_ascii=False)
    res = resp.get("result", {})
    if res.get("wasThrown"):
        return "THROWN: " + json.dumps(res, ensure_ascii=False)
    r = res.get("result", {})
    return r.get("value", json.dumps(r, ensure_ascii=False))


def main():
    args = sys.argv[1:]
    raw = False
    if args and args[0] == "--raw":
        raw, args = True, args[1:]
    mode = args[0] if args else "text"
    if mode.startswith("@"):
        with open(mode[1:], "r", encoding="utf-8") as f:
            mode = f.read()
    r = Remote()
    r.call("Runtime.enable")
    if mode == "--html":
        expr = "document.documentElement.outerHTML"
    elif mode == "--text" or mode == "text":
        expr = "document.body.innerText"
    else:
        expr = mode
    resp = r.call("Runtime.evaluate", {
        "expression": expr, "returnByValue": True,
        "includeCommandLineAPI": True, "awaitPromise": True,
    }, timeout=60)
    if raw:
        print(json.dumps(resp, ensure_ascii=False, indent=2))
    else:
        out = value_of(resp)
        if mode == "--html" and isinstance(out, str):
            print(out[:4000])
        else:
            print(out)


if __name__ == "__main__":
    main()
