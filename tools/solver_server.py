#!/usr/bin/env python3
"""Solve HDDT captcha SVG posted by the browser. Listens on 127.0.0.1:8765.

POST /solve  body = svg (xml) or data:image/svg+xml;base64,...
Returns the 6-char answer as text/plain.
"""
import base64
import http.server
import os
import sys
import urllib.parse

import numpy as np

TOOLS = "./src-tauri/tools/hddt-captcha"
sys.path.insert(0, TOOLS)
from glyph_algo import glyph_paths, render_glyph  # noqa: E402

TEMPLATES = "./packages/hddt-captcha/src/templates.bin"
MASK_BYTES = 288


def unpack(raw):
    img = np.zeros((48, 48), dtype=np.uint8)
    for n in range(48 * 48):
        if (raw[n // 8] >> (7 - (n % 8))) & 1:
            img[n // 48, n % 48] = 1
    return img


def load_templates(path):
    data = open(path, "rb").read()
    out = []
    i = 0
    while i + 1 + MASK_BYTES <= len(data):
        out.append((chr(data[i]), unpack(data[i + 1 : i + 1 + MASK_BYTES])))
        i += 1 + MASK_BYTES
    return out


def iou(a, b):
    inter = int((a & b).sum())
    union = int((a | b).sum())
    return inter / union if union else 0.0


REFS = {}
for _ch, _img in load_templates(TEMPLATES):
    REFS.setdefault(_ch, []).append(_img)


def solve(svg):
    glyphs = glyph_paths(svg)
    ans = ""
    scores = []
    for d in glyphs:
        m = render_glyph(d)
        best, bs = "?", -1.0
        for ch, imgs in REFS.items():
            s = max(iou(m, r) for r in imgs)
            if s > bs:
                bs, best = s, ch
        ans += best
        scores.append(round(bs, 3))
    return ans, scores


class Handler(http.server.BaseHTTPRequestHandler):
    def _cors(self):
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Methods", "POST, OPTIONS")
        self.send_header("Access-Control-Allow-Headers", "*")

    def do_OPTIONS(self):
        self.send_response(204)
        self._cors()
        self.end_headers()

    def do_POST(self):
        n = int(self.headers.get("Content-Length", "0"))
        body = self.rfile.read(n).decode("utf-8", "replace").strip()
        if body.startswith("data:image/svg"):
            body = base64.b64decode(body.split(",", 1)[1]).decode("utf-8", "replace")
        try:
            ans, scores = solve(body)
            out = ans
            sys.stderr.write(f"solve -> {ans} {scores}\n")
            sys.stderr.flush()
        except Exception as e:  # noqa: BLE001
            out = "ERROR: " + str(e)
        data = out.encode()
        self.send_response(200)
        self._cors()
        self.send_header("Content-Type", "text/plain; charset=utf-8")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def log_message(self, *a):
        pass


if __name__ == "__main__":
    print("solver server on http://127.0.0.1:8765", flush=True)
    http.server.HTTPServer(("127.0.0.1", 8765), Handler).serve_forever()

