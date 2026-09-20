#!/usr/bin/env python3
"""Generate the reference data for `GlyphTemplateSolver`:

    templates.bin  →  src-tauri/src/hddt/templates.bin
    parity.txt     →  src-tauri/src/hddt/parity.txt

templates.bin format: repeated [label 1 byte][bitmap 288 bytes] (row-major,
MSB-first). parity.txt: one line `label<TAB>path<TAB>hex288` per character — a
fixture for the Rust unit test `parity_with_python_reference` (the Rust port must
render exactly like the Python version).

Data comes from `labeled/<index>_<key>_<ANSWER>.svg`; the characters are taken
from the ANSWER in the filename, glyphs in left → right order.

Usage (only numpy + python3):
    python3 export_templates.py            # writes into src-tauri/src/hddt/
    python3 export_templates.py <outdir>   # write elsewhere (for comparison)
"""
import glob
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from glyph_algo import glyph_paths, pack, render_glyph  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
LABELED = os.path.join(HERE, "labeled")
DEFAULT_OUT = os.path.abspath(os.path.join(HERE, "..", "..", "src", "hddt"))


def main():
    outdir = os.path.abspath(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_OUT
    os.makedirs(outdir, exist_ok=True)

    items = []  # (char, bitmap, path)
    for f in sorted(glob.glob(os.path.join(LABELED, "*.svg"))):
        ans = os.path.basename(f).split("_")[2].split(".")[0]
        if len(ans) != 6:
            print(f"  !! {os.path.basename(f)}: filename not in <i>_<key>_<ANSWER>.svg form — skipped")
            continue
        ds = glyph_paths(open(f).read())
        if len(ds) != len(ans):
            print(f"  !! {os.path.basename(f)}: {len(ds)} glyphs vs {len(ans)} answers — skipped")
            continue
        for ch, d in zip(ans, ds):
            items.append((ch, render_glyph(d), d))

    if not items:
        sys.exit("No labeled data — check the labeled/ directory")

    blob = bytearray()
    for ch, img, _ in items:
        blob.append(ord(ch))
        blob += pack(img)
    with open(os.path.join(outdir, "templates.bin"), "wb") as fh:
        fh.write(bytes(blob))

    # Parity fixture: shortest glyph of EACH character.
    by_char = {}
    for ch, img, d in items:
        if ch not in by_char or len(d) < len(by_char[ch][2]):
            by_char[ch] = (ch, img, d)
    fixtures = [by_char[c] for c in sorted(by_char)]
    with open(os.path.join(outdir, "parity.txt"), "w") as fh:
        for ch, img, d in fixtures:
            fh.write(f"{ch}\t{d}\t{pack(img).hex()}\n")

    chars = sorted(set(c for c, _, _ in items))
    print(f"templates.bin: {len(items)} glyphs, {len(blob)} bytes → {outdir}")
    print(f"parity.txt: {len(fixtures)} glyphs (1 per character)")
    print(f"chars ({len(chars)}): {''.join(chars)}")
    print("counts:", {c: sum(1 for x, _, _ in items if x == c) for c in chars})


if __name__ == "__main__":
    main()
