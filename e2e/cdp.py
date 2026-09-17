#!/usr/bin/env python3
"""CDP client for WebKitGTK's remote inspector HTTP endpoint.

Flow: Target.getTargets -> Target.attachToTarget(flatten) -> sessioned Runtime.evaluate.

Usage:
  cdp.py <js-expression>              # evaluate JS in page, print value
  cdp.py --targets                    # list targets
  cdp.py --raw <js-expression>        # dump all messages
  cdp.py --dom                        # print document.documentElement.outerHTML length + body text
"""
import base64
import json
import os
import socket
import struct
import sys

HOST = os.environ.get("INSPECT_HOST", "127.0.0.1")
PORT = int(os.environ.get("INSPECT_PORT", "9223"))
PATH = os.environ.get("INSPECT_PATH", "/socket/1/1/WebPage")


def recv_exact(sock, n):
    buf = b""
    while len(buf) < n:
        chunk = sock.recv(n - len(buf))
        if not chunk:
            raise EOFError("closed")
        buf += chunk
    return buf


def send_frame(sock, payload, opcode=0x1):
    data = payload.encode("utf-8") if isinstance(payload, str) else payload
    mask = os.urandom(4)
    ln = len(data)
    h = bytearray([0x80 | opcode])
    if ln < 126:
        h.append(0x80 | ln)
    elif ln < 65536:
        h.append(0x80 | 126); h += struct.pack(">H", ln)
    else:
        h.append(0x80 | 127); h += struct.pack(">Q", ln)
    h += mask
    sock.sendall(bytes(h) + bytes(b ^ mask[i % 4] for i, b in enumerate(data)))


def recv_message(sock):
    frag, op0 = b"", None
    while True:
        b1, b2 = recv_exact(sock, 2)
        fin, op = b1 & 0x80, b1 & 0x0F
        masked, ln = b2 & 0x80, b2 & 0x7F
        if ln == 126:
            ln = struct.unpack(">H", recv_exact(sock, 2))[0]
        elif ln == 127:
            ln = struct.unpack(">Q", recv_exact(sock, 8))[0]
        m = recv_exact(sock, 4) if masked else None
        data = recv_exact(sock, ln) if ln else b""
        if m:
            data = bytes(b ^ m[i % 4] for i, b in enumerate(data))
        if op == 0x9:
            send_frame(sock, data, 0xA); continue
        if op == 0xA:
            continue
        if op == 0x8:
            raise EOFError("closed")
        if op in (0x1, 0x2):
            op0, frag = op, data
        elif op == 0x0:
            frag += data
        if fin:
            return op0, frag


class Cdp:
    def __init__(self):
        self.s = socket.create_connection((HOST, PORT), timeout=15)
        key = base64.b64encode(os.urandom(16)).decode()
        self.s.sendall((
            f"GET {PATH} HTTP/1.1\r\nHost: {HOST}:{PORT}\r\nUpgrade: websocket\r\n"
            f"Connection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
        ).encode())
        hdr = b""
        while b"\r\n\r\n" not in hdr:
            hdr += self.s.recv(1)
        if b"101" not in hdr.split(b"\r\n")[0]:
            raise RuntimeError(hdr.decode(errors="replace"))
        self.id = 0
        self.log = []
        self.session = None

    def send(self, method, params=None, session=None):
        self.id += 1
        msg = {"id": self.id, "method": method, "params": params or {}}
        if session:
            msg["sessionId"] = session
        send_frame(self.s, json.dumps(msg))
        return self.id

    def wait(self, want_id):
        while True:
            op, payload = recv_message(self.s)
            if op != 0x1:
                continue
            try:
                obj = json.loads(payload.decode("utf-8"))
            except Exception:
                continue
            self.log.append(obj)
            if obj.get("id") == want_id:
                return obj


def attach(c):
    if c.send("Target.getTargets") and True:
        r = c.wait(c.id)
    targets = r.get("result", {}).get("targetInfos", [])
    page = next((t for t in targets if t.get("type") == "page"), None)
    if not page:
        page = targets[0] if targets else None
    if not page:
        raise RuntimeError("no targets: " + json.dumps(r))
    tid = page["targetId"]
    i = c.send("Target.attachToTarget", {"targetId": tid, "flatten": True})
    ar = c.wait(i)
    sid = ar.get("result", {}).get("sessionId")
    c.session = sid
    return page, sid


def evaluate(c, expr):
    i = c.send("Runtime.evaluate", {
        "expression": expr, "returnByValue": True,
        "includeCommandLineAPI": True, "awaitPromise": True,
    }, session=c.session)
    r = c.wait(i)
    res = r.get("result", {})
    if res.get("exceptionDetails"):
        return "THROWN: " + json.dumps(res["exceptionDetails"], ensure_ascii=False)
    rr = res.get("result", {})
    return rr.get("value", json.dumps(rr, ensure_ascii=False))


def main():
    args = sys.argv[1:]
    raw = False
    if args and args[0] == "--raw":
        raw, args = True, args[1:]
    if args and args[0] == "--targets":
        c = Cdp()
        c.send("Target.getTargets")
        r = c.wait(c.id)
        print(json.dumps(r, ensure_ascii=False, indent=2))
        return
    expr = args[0] if args else "document.body.innerText"
    c = Cdp()
    page, sid = attach(c)
    if raw:
        evaluate(c, expr)
        print(json.dumps(c.log, ensure_ascii=False, indent=2))
        return
    print(f"[attached {page.get('targetId')} {page.get('url','')} session={sid}]")
    print(evaluate(c, expr))


if __name__ == "__main__":
    main()
