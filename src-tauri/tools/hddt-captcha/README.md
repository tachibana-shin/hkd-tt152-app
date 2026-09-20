# HDDT captcha reference-data pipeline

Offline tooling that generates `templates.bin` + `parity.txt` for the Rust solver
`src-tauri/src/hddt/solver.rs::GlyphTemplateSolver`.

## Why this works

The `hoadondientu.gdt.gov.vn` captcha is 6 SVG glyphs rendered with the
**portal's own font** (not matching any system font), but it is the **same font
every time**, only position/scale differ. So collecting labeled samples → raster
each glyph to a 48×48 bitmap → IoU matching solves it **fully offline**, with no
paid service required.

The actual charset has only **29 characters**:
`23456789ABCDEFGHJKMNQRSTVWXYZ` (the server excludes `0 1 I L O P U` — the
confusable ones). Measured on the live portal: 8/8 automatic logins succeeded;
leave-one-out 222/222.

## Components

| File | Role |
|---|---|
| `glyph_algo.py` | **Source of truth** of the rasterizer (paths M/L/Q/Z, nonzero fill, normalize to 40px + pad 1, center in 48×48). The Rust implementation is a 1-1 port of this file. |
| `export_templates.py` | Generates `templates.bin` + `parity.txt` from `labeled/`. Writes straight into `src-tauri/src/hddt/`. |
| `evaluate.py` | Re-classifies the labeled set through `templates.bin` + leave-one-out. Expects 100%. |
| `collect_captchas.py` | Fetches fresh captchas from the portal (urllib, no browser) to grow the sample set. |
| `labeled/` | Labeled samples `<index>_<key>_<ANSWER>.svg` (37 files, 222 glyphs). |

## Data format

- `templates.bin`: repeated `[label 1 byte][bitmap 288 bytes]` — bitmap 48×48 =
  2304 bits, row-major, MSB-first. Currently 222 entries.
- `parity.txt`: one line `label<TAB>path<TAB>hex(288 bytes)`. The Rust unit test
  `parity_with_python_reference` re-renders each path and compares against the
  hex → catches any divergence between the Python and Rust implementations.

## Workflow

Only `python3` + `numpy` are required.

```bash
cd src-tauri/tools/hddt-captcha

# 1) (optional) fetch fresh captchas → new/
python3 collect_captchas.py 6

# 2) Open the images in new/, read the captcha, rename to
#    <index>_<key>_<ANSWER>.svg and move into labeled/  (unlabeled files
#    in labeled/ are skipped by the exporter)

# 3) Regenerate the reference data
python3 export_templates.py

# 4) Verify
python3 evaluate.py
cd ../../ && cargo test --lib hddt
```

### Adding new samples to increase coverage

1. `python3 collect_captchas.py <n>` → `new/`.
2. Open each SVG in a browser and read its 6 characters (note: only characters
   from the 29-character charset above can appear), then rename
   `new/<index>_<key>.svg` → `labeled/<index>_<key>_<ANSWER>.svg` (exactly 6
   characters).
3. Re-run `export_templates.py` → `evaluate.py` → `cargo test`.

## Notes

- The captcha paths only use `M/L/Q/Z`; if the portal ever changes the format
  (adds another command), update **both** `glyph_algo.py` and `glyph.rs`, then
  re-run the parity test.
- `new/` and `__pycache__/` are not committed.
- This is reference data and contains **no login credentials**.
