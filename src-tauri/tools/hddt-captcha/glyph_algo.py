#!/usr/bin/env python3
"""Captcha glyph rasterizer algorithm — the SOURCE OF TRUTH of the pipeline.

The Rust implementation (`packages/hddt-captcha/src/glyph.rs`) is a 1-1 port of this
file; the `parity_with_python_reference` unit test compares each bitmap against
a fixture generated here to guarantee the two never diverge.

The portal's captcha paths only use M / L / Q / Z (already flattened
server-side); the default fill-rule is nonzero. Normalization: bbox → scale the
longest side to 40 → pad 1 → render → center in a 48×48 frame.

Depends only on `numpy`.
"""
import re
import numpy as np


def parse_path(d):
    """M/L/Q/Z → contours (Q flattened into polygons, n=10)."""
    toks = re.findall(r'[MLQZ]|[-+]?[0-9]*\.?[0-9]+', d)
    contours, cur, start = [], None, None
    i = 0

    def num():
        nonlocal i
        v = float(toks[i]); i += 1; return v

    while i < len(toks):
        t = toks[i]
        if t not in 'MLQZ':
            break
        i += 1
        if t == 'M':
            cur = [(num(), num())]; start = cur[0]; contours.append(cur)
        elif t == 'L':
            while i + 1 < len(toks) and toks[i] not in 'MLQZ':
                cur.append((num(), num()))
        elif t == 'Q':
            while i + 3 < len(toks) and toks[i] not in 'MLQZ':
                cx, cy, x, y = num(), num(), num(), num()
                p0 = cur[-1]
                for k in range(1, 11):
                    u = k / 10
                    cur.append(((1 - u) ** 2 * p0[0] + 2 * (1 - u) * u * cx + u * u * x,
                                (1 - u) ** 2 * p0[1] + 2 * (1 - u) * u * cy + u * u * y))
        else:  # Z
            if cur and len(cur) > 1:
                cur.append(start)
    return [c for c in contours if len(c) > 1]


def rasterize(contours, gw, gh):
    """Nonzero scanline fill: per row collect crossings, sort, fill alternating spans."""
    acc = np.zeros((gh, gw), dtype=np.uint8)
    edges = []
    for c in contours:
        for (x0, y0), (x1, y1) in zip(c, c[1:]):
            if abs(y1 - y0) > 1e-12:
                edges.append((x0, y0, x1, y1))
    for y in range(gh):
        yc = y + 0.5
        xs = []
        for (x0, y0, x1, y1) in edges:
            ylo, yhi = (y0, y1) if y0 < y1 else (y1, y0)
            if ylo <= yc < yhi:
                xs.append(x0 + (yc - y0) / (y1 - y0) * (x1 - x0))
        if len(xs) < 2:
            continue
        xs.sort()
        for k in range(0, len(xs) - 1, 2):
            xa = int(np.ceil(xs[k] - 0.5))
            xb = int(np.floor(xs[k + 1] - 0.5))
            if xb >= xa:
                acc[y, max(0, xa):min(gw, xb + 1)] = 1
    return acc


def render_glyph(d):
    """Path → 48×48 bitmap (matches `glyph.rs::render_glyph`)."""
    coords = [tuple(map(float, t[0].strip().split())) for t in
              re.finditer(r'[-+]?[0-9]*\.?[0-9]+[,\s]+[-+]?[0-9]*\.?[0-9]+', d)]
    xs = [c[0] for c in coords]; ys = [c[1] for c in coords]
    x0b, x1, y0b, y1 = min(xs), max(xs), min(ys), max(ys)
    w, h = x1 - x0b, y1 - y0b
    s = 40 / max(w, h)
    W, H = max(2, int(w * s)), max(2, int(h * s))
    sx, sy = (W + 2) / (w + 2), (H + 2) / (h + 2)
    contours = [[((x - x0b + 1) * sx, (y - y0b + 1) * sy) for x, y in c]
                for c in parse_path(d)]
    mask = rasterize(contours, W + 2, H + 2)
    out = np.zeros((48, 48), dtype=np.uint8)
    yy, xx = (48 - mask.shape[0]) // 2, (48 - mask.shape[1]) // 2
    if yy >= 0 and xx >= 0:
        out[yy:yy + mask.shape[0], xx:xx + mask.shape[1]] = mask
    return out


def glyph_paths(svg):
    """Extract the `d` of each glyph (color-filled path), sorted left → right."""
    svg2 = re.sub(r'<path\s+[^>]*stroke="[^"]*"[^>]*/>', '', svg)
    out = []
    for m in re.finditer(r'<path fill="#([0-9a-fA-F]{3,6})"\s+d="([^"]+)"', svg2):
        if m.group(1) in ("888", "777"):
            continue
        xs = [float(t[0].strip().split()[0]) for t in
              re.finditer(r'[-+]?[0-9]*\.?[0-9]+[,\s]+[-+]?[0-9]*\.?[0-9]+', m.group(2))]
        out.append((min(xs), m.group(2)))
    out.sort()
    return [d for _, d in out]


def pack(img):
    """48×48 bitmap → 288 bytes (row-major, MSB-first) — matches templates.bin format."""
    out = bytearray()
    for r in range(48):
        byte = 0
        for c in range(48):
            byte = (byte << 1) | int(img[r, c])
            if c % 8 == 7:
                out.append(byte)
                byte = 0
    return bytes(out)
