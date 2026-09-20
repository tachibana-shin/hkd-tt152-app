#!/usr/bin/env python3
"""Evaluate the reference data quality on the labeled set:

  1. `templates.bin`: re-classify every labeled glyph → accuracy (expect 100%).
  2. Leave-one-out: remove the sample under test from the references, then
     classify it again → a more honest accuracy (no self-match).

Uses the same rendering algorithm as Rust (`glyph_algo`), so it reflects the
actual behavior of `GlyphTemplateSolver`. Only numpy is required.
"""
import glob
import os
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from glyph_algo import glyph_paths, render_glyph  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
LABELED = os.path.join(HERE, "labeled")
TEMPLATES = os.path.abspath(
    sys.argv[1] if len(sys.argv) > 1 else os.path.join(HERE, "..", "..", "src", "hddt", "templates.bin")
)
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
        out.append((chr(data[i]), unpack(data[i + 1:i + 1 + MASK_BYTES])))
        i += 1 + MASK_BYTES
    return out


def load_labeled():
    items = []  # (char, bitmap)
    for f in sorted(glob.glob(os.path.join(LABELED, "*.svg"))):
        ans = os.path.basename(f).split("_")[2].split(".")[0]
        if len(ans) != 6:
            continue
        glyphs = glyph_paths(open(f).read())
        if len(glyphs) != len(ans):
            print(f"  !! {os.path.basename(f)}: {len(glyphs)} glyphs vs {len(ans)} — skipped")
            continue
        for ch, d in zip(ans, glyphs):
            items.append((ch, render_glyph(d)))
    return items


def iou(a, b):
    inter = int((a & b).sum())
    union = int((a | b).sum())
    return inter / union if union else 0.0


def classify(img, refs):
    best_ch, best_s = "?", -1.0
    for ch, imgs in refs.items():
        s = max(iou(img, r) for r in imgs)
        if s > best_s:
            best_s, best_ch = s, ch
    return best_ch


def main():
    templates = load_templates(TEMPLATES)
    items = load_labeled()
    print(f"templates: {len(templates)} glyphs | labeled: {len(items)} glyphs")

    refs_all = {}
    for ch, img in templates:
        refs_all.setdefault(ch, []).append(img)

    # 1) re-classify through templates.bin
    ok = sum(classify(img, refs_all) == ch for ch, img in items)
    print(f"\n[templates.bin] re-classify labeled: {ok}/{len(items)} = {ok / len(items) * 100:.1f}%")

    # 2) leave-one-out
    ok_loo = 0
    for i, (ch, img) in enumerate(items):
        refs = {}
        for j, (c2, im2) in enumerate(items):
            if j != i:
                refs.setdefault(c2, []).append(im2)
        ok_loo += classify(img, refs) == ch
    print(f"[leave-one-out] {ok_loo}/{len(items)} = {ok_loo / len(items) * 100:.1f}%")

    if ok != len(items) or ok_loo != len(items):
        sys.exit("WARNING: accuracy < 100% — check the data/algorithm")


if __name__ == "__main__":
    main()
